use std::{
    collections::HashMap,
    error::Error,
    sync::Arc,
    time::{Duration, Instant},
};

use application::ports::{
    CheckRepository, DiagnosticReportRepository, IdGenerator, MonitorRepository,
};
use chrono::{Duration as ChronoDuration, Utc};
#[allow(unused_imports)]
use domain::shared::ids::{MonitorId, TenantId};
use domain::{Check, CheckStatus, DiagnosticReport, ErrorType, MonitorKind};
use metrics::{counter, histogram};
use reqwest::Client;
use tokio::{task::JoinSet, time};
use tracing::{error, info, warn};

use crate::{checker::check_monitor, incident_manager::IncidentManager};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Default)]
struct MonitorCheckState {
    last_checked_at: Option<Instant>,
}

/// Groups the dependencies that stay constant across every sweep, so
/// `process_sweep` doesn't need to take each one as a separate argument.
/// Add new shared dependencies here instead of widening the function signature.
struct SweepContext<'a, M: ?Sized, C: ?Sized, G: ?Sized, D: ?Sized> {
    monitor_repository: &'a M,
    check_repository: &'a C,
    incident_manager: &'a IncidentManager,
    id_generator: &'a G,
    client: &'a Client,
    diagnostic_report_repository: &'a D,
}

/// Runs the worker loop forever on a fixed interval.
pub async fn run(
    monitor_repository: Arc<dyn MonitorRepository>,
    check_repository: Arc<dyn CheckRepository>,
    incident_manager: IncidentManager,
    id_generator: Arc<dyn IdGenerator>,
    client: Client,
    diagnostic_report_repository: Arc<dyn DiagnosticReportRepository>,
) -> Result<()> {
    info!("Worker scheduler started");

    let mut interval = time::interval(Duration::from_secs(30));
    let mut states: HashMap<MonitorId, MonitorCheckState> = HashMap::new();

    let ctx = SweepContext {
        monitor_repository: monitor_repository.as_ref(),
        check_repository: check_repository.as_ref(),
        incident_manager: &incident_manager,
        id_generator: id_generator.as_ref(),
        client: &client,
        diagnostic_report_repository: diagnostic_report_repository.as_ref(),
    };

    loop {
        interval.tick().await;

        process_sweep(&ctx, &mut states, Instant::now(), run_check).await?;
    }
}

fn run_check<'a>(
    client: &'a reqwest::Client,
    monitor: &'a domain::Monitor,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<domain::CheckResult>> + Send + 'a>> {
    match monitor.kind {
        MonitorKind::Http => Box::pin(check_monitor(client, monitor)),
        MonitorKind::Tcp => Box::pin(crate::tcp_checker::check_tcp(monitor)),
        MonitorKind::Dns => Box::pin(crate::dns_checker::check_dns(monitor)),
        // Heartbeat monitors are filtered out of the check_fn sweep path in
        // `process_sweep` (they are evaluated separately, without ever
        // producing a Check/CheckResult). This arm exists only so the match
        // is exhaustive; reaching it is a bug elsewhere in the dispatch.
        MonitorKind::Heartbeat => Box::pin(async {
            Err(
                "heartbeat monitors must not go through the active check_fn path"
                    .to_string()
                    .into(),
            )
        }),
        // Flow monitors are handled by the durable scheduler's
        // `FlowCheckExecutor`, not this legacy active-sweep path.
        // This arm exists only for exhaustiveness; reaching it is a
        // dispatch bug.
        MonitorKind::Flow => Box::pin(async {
            Err(
                "flow monitors must not go through the legacy active check_fn path"
                    .to_string()
                    .into(),
            )
        }),
    }
}

async fn evaluate_monitor_incident<C: CheckRepository + ?Sized>(
    check_repository: &C,
    incident_manager: &IncidentManager,
    monitor: &domain::Monitor,
) -> Result<()> {
    let recent_checks = check_repository.list_by_monitor(monitor.id).await?;
    let status = domain::evaluate_multi_region_status(&monitor.regions, &recent_checks, Utc::now());

    match status {
        CheckStatus::Down => incident_manager.open_if_needed(monitor).await?,
        CheckStatus::Up => incident_manager.resolve_if_needed(monitor).await?,
    }

    Ok(())
}

/// Evaluates a `MonitorKind::Heartbeat` monitor's up/down status WITHOUT
/// performing any active network check and WITHOUT saving a `Check`/
/// `CheckResult` — heartbeat monitors are passive; the "check" is the
/// external ping to `GET /heartbeat/{token}` which updates
/// `last_heartbeat_at`. Here we only decide whether the monitor is overdue
/// and, if so, mirror what `evaluate_monitor_incident` does for the other
/// kinds by opening/resolving an incident.
async fn evaluate_heartbeat_monitor(
    incident_manager: &IncidentManager,
    monitor: &domain::Monitor,
) -> Result<()> {
    let interval_seconds = monitor
        .heartbeat_interval_seconds
        .ok_or("heartbeat_interval_seconds is missing for a Heartbeat monitor")?;
    let grace_period_seconds = monitor
        .heartbeat_grace_period_seconds
        .ok_or("heartbeat_grace_period_seconds is missing for a Heartbeat monitor")?;

    let overdue = match monitor.last_heartbeat_at {
        // No heartbeat received yet: treat as immediately overdue.
        None => true,
        Some(last_heartbeat_at) => {
            let deadline = last_heartbeat_at
                + ChronoDuration::seconds(
                    i64::from(interval_seconds) + i64::from(grace_period_seconds),
                );
            Utc::now() > deadline
        }
    };

    if overdue {
        incident_manager.open_if_needed(monitor).await?;
    } else {
        incident_manager.resolve_if_needed(monitor).await?;
    }

    Ok(())
}

async fn process_sweep<M, C, G, D, F>(
    ctx: &SweepContext<'_, M, C, G, D>,
    states: &mut HashMap<MonitorId, MonitorCheckState>,
    now: Instant,
    check_fn: F,
) -> Result<()>
where
    M: MonitorRepository + ?Sized,
    C: CheckRepository + ?Sized,
    G: IdGenerator + ?Sized,
    D: DiagnosticReportRepository + ?Sized,
    F: for<'a> Fn(
            &'a reqwest::Client,
            &'a domain::Monitor,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<domain::CheckResult>> + Send + 'a>,
        > + Copy
        + Send
        + Sync
        + 'static,
{
    let monitor_repository = ctx.monitor_repository;
    let check_repository = ctx.check_repository;
    let incident_manager = ctx.incident_manager;
    let id_generator = ctx.id_generator;
    let client = ctx.client;
    let diagnostic_report_repository = ctx.diagnostic_report_repository;

    let monitors = monitor_repository.find_all().await?;

    info!(count = monitors.len(), "starting monitor sweep");

    let mut tasks = JoinSet::new();

    for monitor in monitors {
        if !monitor.enabled {
            continue;
        }

        if monitor.kind == MonitorKind::Heartbeat {
            if let Err(err) = evaluate_heartbeat_monitor(incident_manager, &monitor).await {
                error!(
                    monitor_id = %monitor.id.0,
                    error = %err,
                    "failed to evaluate heartbeat monitor"
                );
            }
            continue;
        }

        let state = states.entry(monitor.id).or_default();

        let should_check = state
            .last_checked_at
            .map(|last_checked_at| {
                now.duration_since(last_checked_at)
                    >= Duration::from_secs(monitor.interval_seconds as u64)
            })
            .unwrap_or(true);

        if !should_check {
            info!(
                monitor_id = %monitor.id.0,
                interval_seconds = monitor.interval_seconds,
                "skipping monitor; interval has not elapsed"
            );
            continue;
        }

        state.last_checked_at = Some(now);

        let client = client.clone();

        tasks.spawn(async move {
            let result = check_fn(&client, &monitor).await;
            (monitor, result)
        });
    }

    struct BatchEntry {
        check: Check,
        monitor: domain::Monitor,
        diagnostic_steps: Vec<domain::DiagnosticStep>,
        diagnostic_summary: String,
    }

    let mut batch = Vec::<BatchEntry>::new();

    while let Some(join_result) = tasks.join_next().await {
        match join_result {
            Ok((monitor, Ok(result))) => {
                counter!(
                    "worker_checks_total",
                    "result" => if result.success { "success" } else { "failure" }
                )
                .increment(1);

                histogram!(
                    "worker_check_duration_seconds",
                    "result" => if result.success { "success" } else { "failure" }
                )
                .record(result.latency_ms as f64 / 1000.0);

                info!(
                    monitor_id = %monitor.id.0,
                    monitor_name = %monitor.name.as_str(),
                    success = result.success,
                    status_code = ?result.status_code,
                    latency_ms = result.latency_ms,
                    "monitor check completed"
                );

                info!(
                    monitor_id = %monitor.id.0,
                    "saving check"
                );

                let check_id = id_generator.next_check_id().await;

                let diagnostic_steps = result.diagnostic_steps.clone();
                let diagnostic_summary = result.diagnostic_root_cause_summary.clone();

                let check = Check::new(
                    check_id,
                    monitor.id,
                    result.region.clone(),
                    if result.success {
                        CheckStatus::Up
                    } else {
                        CheckStatus::Down
                    },
                    result.status_code,
                    result.latency_ms,
                    if result.success {
                        ErrorType::None
                    } else {
                        ErrorType::Unknown
                    },
                    result.error,
                )
                .with_cert_info(result.cert_expires_at, result.cert_days_remaining);

                batch.push(BatchEntry {
                    check,
                    monitor: monitor.clone(),
                    diagnostic_steps,
                    diagnostic_summary: diagnostic_summary.unwrap_or_default(),
                });
            }

            Ok((monitor, Err(err))) => {
                counter!("worker_checks_total", "result" => "failure").increment(1);

                warn!(
                    monitor_id = %monitor.id.0,
                    error = %err,
                    "monitor check failed"
                );
            }

            Err(err) => {
                error!(error = %err, "worker task panicked");
            }
        }
    }

    if !batch.is_empty() {
        let checks: Vec<Check> = batch.iter().map(|entry| entry.check.clone()).collect();
        if let Err(err) = check_repository.save_batch(&checks).await {
            counter!("worker_check_save_errors_total").increment(1);
            error!(
                error = %err,
                "failed to save check batch"
            );
        }
    }

    for entry in batch {
        if !entry.diagnostic_steps.is_empty() {
            let diagnostic_report_id = id_generator.next_diagnostic_report_id().await;
            let report = DiagnosticReport::new(
                diagnostic_report_id,
                entry.check.id,
                entry.check.checked_at,
                entry.diagnostic_steps,
                entry.diagnostic_summary,
            );

            if let Err(err) = diagnostic_report_repository.save(&report).await {
                error!(
                    monitor_id = %entry.monitor.id.0,
                    check_id = %entry.check.id.0,
                    error = %err,
                    "failed to save diagnostic report"
                );
            }
        }

        if let Err(err) =
            evaluate_monitor_incident(check_repository, incident_manager, &entry.monitor).await
        {
            error!(
                monitor_id = %entry.monitor.id.0,
                error = %err,
                "failed to evaluate multi-region incident status"
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use application::ports::{CheckRepository, IncidentRepository, MonitorRepository};
    use async_trait::async_trait;
    use domain::shared::ids::{CheckId, IncidentId, MonitorId, UserId};
    use domain::{
        Check, CheckInterval, ExpectedStatus, HttpMethod, Incident, Monitor, MonitorBuilder,
        MonitorName, Timeout, Url,
    };
    use infra::{
        InMemoryMonitorRepository, InMemoryNotificationChannelRepository, NoopNotificationService,
        UuidIdGenerator,
    };
    use uuid::Uuid;

    fn test_notification_deps() -> (
        Arc<InMemoryNotificationChannelRepository>,
        Arc<NoopNotificationService>,
    ) {
        (
            Arc::new(InMemoryNotificationChannelRepository::new(
                InMemoryMonitorRepository::new(),
            )),
            Arc::new(NoopNotificationService::new()),
        )
    }

    #[derive(Clone, Default)]
    struct MockMonitorRepository {
        monitors: Arc<Mutex<Vec<Monitor>>>,
    }

    impl MockMonitorRepository {
        fn with_monitors(monitors: Vec<Monitor>) -> Self {
            Self {
                monitors: Arc::new(Mutex::new(monitors)),
            }
        }
    }

    #[async_trait]
    impl MonitorRepository for MockMonitorRepository {
        async fn save(
            &self,
            monitor: &Monitor,
        ) -> std::result::Result<(), Box<dyn Error + Send + Sync>> {
            self.monitors.lock().unwrap().push(monitor.clone());
            Ok(())
        }

        async fn update(
            &self,
            monitor: &Monitor,
        ) -> std::result::Result<(), Box<dyn Error + Send + Sync>> {
            let mut monitors = self.monitors.lock().unwrap();
            if let Some(slot) = monitors.iter_mut().find(|m| m.id == monitor.id) {
                *slot = monitor.clone();
            }
            Ok(())
        }

        async fn find_by_id(
            &self,
            id: MonitorId,
        ) -> std::result::Result<Option<Monitor>, Box<dyn Error + Send + Sync>> {
            Ok(self
                .monitors
                .lock()
                .unwrap()
                .iter()
                .find(|monitor| monitor.id == id)
                .cloned())
        }

        async fn find_all(
            &self,
        ) -> std::result::Result<Vec<Monitor>, Box<dyn Error + Send + Sync>> {
            Ok(self.monitors.lock().unwrap().clone())
        }

        async fn delete(
            &self,
            id: MonitorId,
        ) -> std::result::Result<bool, Box<dyn Error + Send + Sync>> {
            let mut monitors = self.monitors.lock().unwrap();
            let len_before = monitors.len();
            monitors.retain(|monitor| monitor.id != id);
            Ok(monitors.len() != len_before)
        }
    }

    #[derive(Clone, Default)]
    struct MockCheckRepository {
        saved_checks: Arc<Mutex<Vec<Check>>>,
    }

    impl MockCheckRepository {
        fn save_count(&self) -> usize {
            self.saved_checks.lock().unwrap().len()
        }
    }

    #[async_trait]
    impl CheckRepository for MockCheckRepository {
        async fn save(
            &self,
            check: &Check,
        ) -> std::result::Result<(), Box<dyn Error + Send + Sync>> {
            self.saved_checks.lock().unwrap().push(check.clone());
            Ok(())
        }

        async fn find_by_id(
            &self,
            _id: CheckId,
            _: chrono::DateTime<chrono::Utc>,
        ) -> std::result::Result<Option<Check>, Box<dyn Error + Send + Sync>> {
            Ok(None)
        }

        async fn list_by_monitor(
            &self,
            monitor_id: MonitorId,
        ) -> std::result::Result<Vec<Check>, Box<dyn Error + Send + Sync>> {
            Ok(self
                .saved_checks
                .lock()
                .unwrap()
                .iter()
                .filter(|check| check.monitor_id == monitor_id)
                .cloned()
                .collect())
        }

        async fn list_checks(
            &self,
            _limit: u32,
            _offset: u32,
            _tenant_id: TenantId,
        ) -> std::result::Result<Vec<Check>, Box<dyn Error + Send + Sync>> {
            Ok(vec![])
        }

        async fn delete_by_monitor(
            &self,
            _monitor_id: MonitorId,
        ) -> std::result::Result<u64, Box<dyn Error + Send + Sync>> {
            Ok(0)
        }
    }

    #[derive(Clone, Default)]
    struct MockIncidentRepository {
        save_calls: Arc<Mutex<usize>>,
        update_calls: Arc<Mutex<usize>>,
        open_exists: Arc<Mutex<bool>>,
    }

    impl MockIncidentRepository {
        fn new(open_exists: bool) -> Self {
            Self {
                save_calls: Arc::new(Mutex::new(0)),
                update_calls: Arc::new(Mutex::new(0)),
                open_exists: Arc::new(Mutex::new(open_exists)),
            }
        }

        fn save_count(&self) -> usize {
            *self.save_calls.lock().unwrap()
        }

        fn update_count(&self) -> usize {
            *self.update_calls.lock().unwrap()
        }
    }

    #[async_trait]
    impl IncidentRepository for MockIncidentRepository {
        async fn save(
            &self,
            _incident: &Incident,
        ) -> std::result::Result<(), Box<dyn Error + Send + Sync>> {
            *self.save_calls.lock().unwrap() += 1;
            *self.open_exists.lock().unwrap() = true;
            Ok(())
        }

        async fn update(
            &self,
            _incident: &Incident,
        ) -> std::result::Result<(), Box<dyn Error + Send + Sync>> {
            *self.update_calls.lock().unwrap() += 1;
            Ok(())
        }

        async fn find_by_id(
            &self,
            _id: IncidentId,
        ) -> std::result::Result<Option<Incident>, Box<dyn Error + Send + Sync>> {
            Ok(None)
        }

        async fn find_open_by_monitor(
            &self,
            monitor_id: MonitorId,
        ) -> std::result::Result<Option<Incident>, Box<dyn Error + Send + Sync>> {
            if *self.open_exists.lock().unwrap() {
                Ok(Some(Incident::new(IncidentId(Uuid::new_v4()), monitor_id)))
            } else {
                Ok(None)
            }
        }

        async fn list_by_monitor(
            &self,
            _monitor_id: MonitorId,
        ) -> std::result::Result<Vec<Incident>, Box<dyn Error + Send + Sync>> {
            Ok(vec![])
        }
    }

    #[derive(Clone, Default)]
    struct MockDiagnosticReportRepository {
        saved: Arc<Mutex<Vec<DiagnosticReport>>>,
    }

    impl MockDiagnosticReportRepository {
        fn save_count(&self) -> usize {
            self.saved.lock().unwrap().len()
        }
    }

    #[async_trait]
    impl DiagnosticReportRepository for MockDiagnosticReportRepository {
        async fn save(
            &self,
            report: &DiagnosticReport,
        ) -> std::result::Result<(), Box<dyn Error + Send + Sync>> {
            self.saved.lock().unwrap().push(report.clone());
            Ok(())
        }

        async fn find_by_check_id(
            &self,
            _check_id: CheckId,
        ) -> std::result::Result<Option<DiagnosticReport>, Box<dyn Error + Send + Sync>> {
            Ok(None)
        }
    }

    fn build_monitor(url: String) -> Monitor {
        MonitorBuilder::new(
            MonitorId(Uuid::new_v4()),
            UserId(Uuid::new_v4()),
            TenantId::new(),
        )
        .name(MonitorName::new("Test Monitor").unwrap())
        .url(Url::new(url).unwrap())
        .method(HttpMethod::Get)
        .expected_status(ExpectedStatus::new(200).unwrap())
        .interval(CheckInterval::new(60).unwrap())
        .timeout(Timeout::new(5).unwrap())
        .build()
    }

    fn mock_success<'a>(
        _c: &'a reqwest::Client,
        _m: &'a domain::Monitor,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<domain::CheckResult>> + Send + 'a>>
    {
        Box::pin(async move {
            Ok(domain::CheckResult::success(
                domain::RegionCode::default_region(),
                200,
                10,
            ))
        })
    }

    fn mock_failure<'a>(
        _c: &'a reqwest::Client,
        _m: &'a domain::Monitor,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<domain::CheckResult>> + Send + 'a>>
    {
        Box::pin(async move {
            Ok(domain::CheckResult::failure_with_status(
                domain::RegionCode::default_region(),
                503,
                10,
                "test failure",
            ))
        })
    }

    #[tokio::test]
    async fn saves_a_check_for_a_healthy_monitor() {
        let url = "http://example.com".to_string();
        let monitor = build_monitor(url);
        let monitor_repository =
            Arc::new(MockMonitorRepository::with_monitors(vec![monitor.clone()]));
        let check_repository = Arc::new(MockCheckRepository::default());
        let incident_repository = Arc::new(MockIncidentRepository::new(false));
        let id_generator = Arc::new(UuidIdGenerator::new());
        let diagnostic_report_repository = Arc::new(MockDiagnosticReportRepository::default());
        let (notification_channel_repository, notification_service) = test_notification_deps();
        let incident_manager = IncidentManager::new(
            incident_repository,
            id_generator.clone(),
            notification_channel_repository,
            notification_service,
        );
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let mut states = HashMap::new();

        let ctx = SweepContext {
            monitor_repository: monitor_repository.as_ref(),
            check_repository: check_repository.as_ref(),
            incident_manager: &incident_manager,
            id_generator: id_generator.as_ref(),
            client: &client,
            diagnostic_report_repository: diagnostic_report_repository.as_ref(),
        };

        process_sweep(&ctx, &mut states, Instant::now(), mock_success)
            .await
            .unwrap();

        assert_eq!(check_repository.save_count(), 1);
        assert_eq!(diagnostic_report_repository.save_count(), 0);
    }

    #[tokio::test]
    async fn opens_incident_after_failure_threshold() {
        let url = "http://example.com".to_string();
        let monitor = build_monitor(url);
        let monitor_repository =
            Arc::new(MockMonitorRepository::with_monitors(vec![monitor.clone()]));
        let check_repository = Arc::new(MockCheckRepository::default());
        let incident_repository = Arc::new(MockIncidentRepository::new(false));
        let id_generator = Arc::new(UuidIdGenerator::new());
        let diagnostic_report_repository = Arc::new(MockDiagnosticReportRepository::default());
        let (notification_channel_repository, notification_service) = test_notification_deps();
        let incident_manager = IncidentManager::new(
            incident_repository.clone(),
            id_generator.clone(),
            notification_channel_repository,
            notification_service,
        );
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let mut states = HashMap::new();

        let ctx = SweepContext {
            monitor_repository: monitor_repository.as_ref(),
            check_repository: check_repository.as_ref(),
            incident_manager: &incident_manager,
            id_generator: id_generator.as_ref(),
            client: &client,
            diagnostic_report_repository: diagnostic_report_repository.as_ref(),
        };

        let start = Instant::now();

        for i in 0..3 {
            process_sweep(
                &ctx,
                &mut states,
                start + std::time::Duration::from_secs(60 * i),
                mock_failure,
            )
            .await
            .unwrap();
        }

        assert_eq!(check_repository.save_count(), 3);
        assert_eq!(incident_repository.save_count(), 1);
        assert_eq!(incident_repository.update_count(), 0);
    }

    #[tokio::test]
    async fn resolves_incident_after_success_threshold() {
        let url = "http://example.com".to_string();
        let monitor = build_monitor(url);
        let _monitor_id = monitor.id;
        let monitor_repository =
            Arc::new(MockMonitorRepository::with_monitors(vec![monitor.clone()]));
        let check_repository = Arc::new(MockCheckRepository::default());
        let incident_repository = Arc::new(MockIncidentRepository::new(false));
        let id_generator = Arc::new(UuidIdGenerator::new());
        let diagnostic_report_repository = Arc::new(MockDiagnosticReportRepository::default());
        let (notification_channel_repository, notification_service) = test_notification_deps();
        let incident_manager = IncidentManager::new(
            incident_repository.clone(),
            id_generator.clone(),
            notification_channel_repository,
            notification_service,
        );
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let mut states = HashMap::new();

        incident_manager.open_if_needed(&monitor).await.unwrap();

        let ctx = SweepContext {
            monitor_repository: monitor_repository.as_ref(),
            check_repository: check_repository.as_ref(),
            incident_manager: &incident_manager,
            id_generator: id_generator.as_ref(),
            client: &client,
            diagnostic_report_repository: diagnostic_report_repository.as_ref(),
        };

        let start = Instant::now();

        for i in 0..2 {
            process_sweep(
                &ctx,
                &mut states,
                start + std::time::Duration::from_secs(60 * i),
                mock_success,
            )
            .await
            .unwrap();
        }

        assert_eq!(check_repository.save_count(), 2);
        assert_eq!(incident_repository.update_count(), 2);
    }
}
