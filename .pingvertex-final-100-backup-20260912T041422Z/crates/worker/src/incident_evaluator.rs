use std::{error::Error, sync::Arc};

use application::ports::MonitorRepository;
use domain::Monitor;

use crate::incident_manager::IncidentManager;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// Evaluates a single check outcome against consecutive-failure/success
/// thresholds and decides whether to open, resolve, or extend an
/// incident.
///
/// `IncidentManager::open_if_needed`/`resolve_if_needed` already make the
/// per-monitor "is there already an open incident" decision (and enforce
/// dedup via the database's partial unique index on `incidents`) — this
/// type only adds the "has this failure/success run crossed the
/// configured threshold yet" gate in front of them, so a single flaky
/// check does not open or close an incident on its own.
#[derive(Clone)]
pub struct IncidentEvaluator {
    monitor_repository: Arc<dyn MonitorRepository>,
    incident_manager: IncidentManager,
    failure_threshold: u32,
    recovery_threshold: u32,
}

impl IncidentEvaluator {
    pub fn new(
        monitor_repository: Arc<dyn MonitorRepository>,
        incident_manager: IncidentManager,
        failure_threshold: u32,
        recovery_threshold: u32,
    ) -> Self {
        Self {
            monitor_repository,
            incident_manager,
            failure_threshold,
            recovery_threshold,
        }
    }

    /// Records `success` as the outcome of the most recent check for
    /// `monitor`, and opens/resolves/extends the monitor's incident as
    /// appropriate:
    ///
    /// - On failure: atomically increments the monitor's
    ///   consecutive-failure streak. Once the streak reaches
    ///   `failure_threshold`, opens an incident (idempotent/deduplicated
    ///   by `IncidentManager`/the database). Every failure additionally
    ///   advances the open incident's `last_seen_at`, if one exists —
    ///   including failures observed after the incident is already open,
    ///   not just the threshold-crossing one.
    /// - On success: atomically increments the consecutive-success
    ///   streak. Once it reaches `recovery_threshold`, resolves the open
    ///   incident, if any.
    pub async fn evaluate(&self, monitor: &Monitor, success: bool) -> Result<()> {
        let (consecutive_failures, consecutive_successes) = self
            .monitor_repository
            .record_check_outcome(monitor.id, success)
            .await?;

        if success {
            if consecutive_successes >= self.recovery_threshold {
                self.incident_manager.resolve_if_needed(monitor).await?;
            }
        } else {
            if consecutive_failures >= self.failure_threshold {
                self.incident_manager.open_if_needed(monitor).await?;
            }
            self.incident_manager
                .touch_last_seen_if_open(monitor)
                .await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use domain::shared::ids::{IncidentId, MonitorId, TenantId, UserId};
    use domain::{
        CheckInterval, ExpectedStatus, HttpMethod, Incident, Monitor, MonitorBuilder, MonitorName,
        Timeout,
    };
    use uuid::Uuid;

    use application::ports::IncidentRepository;
    use application::ports::incident_repository::RepoResult as IncidentRepoResult;
    use application::ports::monitor_repository::RepoResult as MonitorRepoResult;
    use infra::{
        InMemoryMonitorRepository, InMemoryNotificationChannelRepository, NoopNotificationService,
        UuidIdGenerator,
    };

    fn build_monitor() -> Monitor {
        MonitorBuilder::new(
            MonitorId(Uuid::new_v4()),
            UserId(Uuid::new_v4()),
            TenantId::new(),
        )
        .name(MonitorName::new("Test Monitor").unwrap())
        .target("http://example.com".to_string())
        .method(HttpMethod::Get)
        .expected_status(ExpectedStatus::new(200).unwrap())
        .interval(CheckInterval::new(60).unwrap())
        .timeout(Timeout::new(5).unwrap())
        .build()
    }

    /// Mirrors, purely in memory, what
    /// `PostgresMonitorRepository::record_check_outcome` does atomically
    /// in the database.
    #[derive(Default)]
    struct CounterState {
        consecutive_failures: u32,
        consecutive_successes: u32,
    }

    #[derive(Clone, Default)]
    struct MockMonitorRepository {
        state: Arc<Mutex<CounterState>>,
    }

    #[async_trait]
    impl MonitorRepository for MockMonitorRepository {
        async fn save(&self, _monitor: &Monitor) -> MonitorRepoResult<()> {
            Ok(())
        }
        async fn update(&self, _monitor: &Monitor) -> MonitorRepoResult<()> {
            Ok(())
        }
        async fn delete(&self, _id: MonitorId) -> MonitorRepoResult<bool> {
            Ok(false)
        }
        async fn find_by_id(&self, _id: MonitorId) -> MonitorRepoResult<Option<Monitor>> {
            Ok(None)
        }
        async fn find_all(&self) -> MonitorRepoResult<Vec<Monitor>> {
            Ok(vec![])
        }

        async fn record_check_outcome(
            &self,
            _id: MonitorId,
            success: bool,
        ) -> MonitorRepoResult<(u32, u32)> {
            let mut state = self.state.lock().unwrap();
            if success {
                state.consecutive_successes += 1;
                state.consecutive_failures = 0;
            } else {
                state.consecutive_failures += 1;
                state.consecutive_successes = 0;
            }
            Ok((state.consecutive_failures, state.consecutive_successes))
        }
    }

    #[derive(Default)]
    struct IncidentRepoState {
        open_exists: bool,
        save_calls: usize,
        update_calls: usize,
        touch_calls: usize,
    }

    #[derive(Clone, Default)]
    struct MockIncidentRepository {
        state: Arc<Mutex<IncidentRepoState>>,
    }

    impl MockIncidentRepository {
        fn with_open(open_exists: bool) -> Self {
            let repo = Self::default();
            repo.state.lock().unwrap().open_exists = open_exists;
            repo
        }
        fn save_calls(&self) -> usize {
            self.state.lock().unwrap().save_calls
        }
        fn update_calls(&self) -> usize {
            self.state.lock().unwrap().update_calls
        }
        fn touch_calls(&self) -> usize {
            self.state.lock().unwrap().touch_calls
        }
    }

    #[async_trait]
    impl IncidentRepository for MockIncidentRepository {
        async fn save(&self, _incident: &Incident) -> IncidentRepoResult<()> {
            let mut state = self.state.lock().unwrap();
            state.save_calls += 1;
            state.open_exists = true;
            Ok(())
        }
        async fn update(&self, _incident: &Incident) -> IncidentRepoResult<()> {
            self.state.lock().unwrap().update_calls += 1;
            Ok(())
        }
        async fn find_by_id(&self, _id: IncidentId) -> IncidentRepoResult<Option<Incident>> {
            Ok(None)
        }
        async fn find_open_by_monitor(
            &self,
            monitor_id: MonitorId,
        ) -> IncidentRepoResult<Option<Incident>> {
            if self.state.lock().unwrap().open_exists {
                Ok(Some(Incident::new(IncidentId(Uuid::new_v4()), monitor_id)))
            } else {
                Ok(None)
            }
        }
        async fn list_by_monitor(
            &self,
            _monitor_id: MonitorId,
        ) -> IncidentRepoResult<Vec<Incident>> {
            Ok(vec![])
        }
        async fn touch_last_seen(
            &self,
            _monitor_id: MonitorId,
            _now: chrono::DateTime<chrono::Utc>,
        ) -> IncidentRepoResult<()> {
            let mut state = self.state.lock().unwrap();
            if state.open_exists {
                state.touch_calls += 1;
            }
            Ok(())
        }
    }

    fn build_evaluator(
        monitor_repository: MockMonitorRepository,
        incident_repository: MockIncidentRepository,
        failure_threshold: u32,
        recovery_threshold: u32,
    ) -> IncidentEvaluator {
        let notification_channel_repository = Arc::new(InMemoryNotificationChannelRepository::new(
            InMemoryMonitorRepository::new(),
        ));
        let notification_service = Arc::new(NoopNotificationService::new());

        let incident_manager = IncidentManager::new(
            Arc::new(incident_repository),
            Arc::new(UuidIdGenerator::new()),
            notification_channel_repository,
            notification_service,
        );

        IncidentEvaluator::new(
            Arc::new(monitor_repository),
            incident_manager,
            failure_threshold,
            recovery_threshold,
        )
    }

    #[tokio::test]
    async fn does_not_open_incident_before_failure_threshold() {
        let monitor = build_monitor();
        let monitor_repository = MockMonitorRepository::default();
        let incident_repository = MockIncidentRepository::default();
        let incident_repository_clone = incident_repository.clone();

        let evaluator = build_evaluator(monitor_repository, incident_repository, 3, 2);

        evaluator.evaluate(&monitor, false).await.unwrap();
        evaluator.evaluate(&monitor, false).await.unwrap();

        assert_eq!(incident_repository_clone.save_calls(), 0);
    }

    #[tokio::test]
    async fn opens_incident_once_failure_threshold_is_reached() {
        let monitor = build_monitor();
        let monitor_repository = MockMonitorRepository::default();
        let incident_repository = MockIncidentRepository::default();
        let incident_repository_clone = incident_repository.clone();

        let evaluator = build_evaluator(monitor_repository, incident_repository, 3, 2);

        evaluator.evaluate(&monitor, false).await.unwrap();
        evaluator.evaluate(&monitor, false).await.unwrap();
        evaluator.evaluate(&monitor, false).await.unwrap();

        assert_eq!(incident_repository_clone.save_calls(), 1);
        // The threshold-crossing failure also touches last_seen_at.
        assert_eq!(incident_repository_clone.touch_calls(), 1);
    }

    #[tokio::test]
    async fn touches_last_seen_on_every_failure_while_incident_stays_open() {
        let monitor = build_monitor();
        let monitor_repository = MockMonitorRepository::default();
        let incident_repository = MockIncidentRepository::default();
        let incident_repository_clone = incident_repository.clone();

        let evaluator = build_evaluator(monitor_repository, incident_repository, 2, 2);

        evaluator.evaluate(&monitor, false).await.unwrap();
        evaluator.evaluate(&monitor, false).await.unwrap(); // opens (threshold=2)
        evaluator.evaluate(&monitor, false).await.unwrap();
        evaluator.evaluate(&monitor, false).await.unwrap();

        assert_eq!(incident_repository_clone.save_calls(), 1);
        assert_eq!(incident_repository_clone.touch_calls(), 3);
    }

    #[tokio::test]
    async fn does_not_resolve_before_recovery_threshold() {
        let monitor = build_monitor();
        let monitor_repository = MockMonitorRepository::default();
        let incident_repository = MockIncidentRepository::with_open(true);
        let incident_repository_clone = incident_repository.clone();

        let evaluator = build_evaluator(monitor_repository, incident_repository, 3, 2);

        evaluator.evaluate(&monitor, true).await.unwrap();

        assert_eq!(incident_repository_clone.update_calls(), 0);
    }

    #[tokio::test]
    async fn resolves_once_recovery_threshold_is_reached() {
        let monitor = build_monitor();
        let monitor_repository = MockMonitorRepository::default();
        let incident_repository = MockIncidentRepository::with_open(true);
        let incident_repository_clone = incident_repository.clone();

        let evaluator = build_evaluator(monitor_repository, incident_repository, 3, 2);

        evaluator.evaluate(&monitor, true).await.unwrap();
        evaluator.evaluate(&monitor, true).await.unwrap();

        assert_eq!(incident_repository_clone.update_calls(), 1);
    }

    #[tokio::test]
    async fn a_single_success_resets_the_failure_streak() {
        let monitor = build_monitor();
        let monitor_repository = MockMonitorRepository::default();
        let incident_repository = MockIncidentRepository::default();
        let incident_repository_clone = incident_repository.clone();

        let evaluator = build_evaluator(monitor_repository, incident_repository, 3, 2);

        evaluator.evaluate(&monitor, false).await.unwrap();
        evaluator.evaluate(&monitor, false).await.unwrap();
        evaluator.evaluate(&monitor, true).await.unwrap();
        evaluator.evaluate(&monitor, false).await.unwrap();
        evaluator.evaluate(&monitor, false).await.unwrap();

        // The streak was reset by the success in between, so 3
        // consecutive failures were never observed — no incident opens.
        assert_eq!(incident_repository_clone.save_calls(), 0);
    }
}
