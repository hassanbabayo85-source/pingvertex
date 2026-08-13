use std::sync::{Arc, Mutex};

use application::ports::{
    IncidentRepository, NotificationChannelRepository, NotificationService, NotifyResult,
};
use async_trait::async_trait;
use domain::{
    CheckInterval, ExpectedStatus, HttpMethod, Incident, Monitor, MonitorBuilder, MonitorName,
    Timeout, Url,
    entities::NotificationChannel,
    notification::NotificationEvent,
    shared::ids::{IncidentId, MonitorId, NotificationChannelId, UserId},
};
use uuid::Uuid;

use worker::IncidentManager;

fn build_monitor() -> Monitor {
    MonitorBuilder::new(MonitorId(Uuid::new_v4()), UserId(Uuid::new_v4()))
        .name(MonitorName::new("Test Monitor").unwrap())
        .url(Url::new("http://example.com".to_string()).unwrap())
        .method(HttpMethod::Get)
        .expected_status(ExpectedStatus::new(200).unwrap())
        .interval(CheckInterval::new(60).unwrap())
        .timeout(Timeout::new(5).unwrap())
        .build()
}

#[derive(Default)]
struct RepoState {
    open_incident_exists: bool,
    save_calls: usize,
    update_calls: usize,
    last_saved: Option<Incident>,
    last_updated: Option<Incident>,
}

#[derive(Clone)]
struct MockIncidentRepository {
    state: Arc<Mutex<RepoState>>,
}

impl MockIncidentRepository {
    fn new(open_incident_exists: bool) -> Self {
        Self {
            state: Arc::new(Mutex::new(RepoState {
                open_incident_exists,
                ..Default::default()
            })),
        }
    }

    fn save_calls(&self) -> usize {
        self.state.lock().unwrap().save_calls
    }

    fn update_calls(&self) -> usize {
        self.state.lock().unwrap().update_calls
    }
}

#[async_trait]
impl IncidentRepository for MockIncidentRepository {
    async fn save(
        &self,
        incident: &Incident,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut state = self.state.lock().unwrap();
        state.save_calls += 1;
        state.last_saved = Some(incident.clone());
        Ok(())
    }

    async fn update(
        &self,
        incident: &Incident,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut state = self.state.lock().unwrap();
        state.update_calls += 1;
        state.last_updated = Some(incident.clone());
        Ok(())
    }

    async fn find_by_id(
        &self,
        _id: IncidentId,
    ) -> Result<Option<Incident>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }

    async fn find_open_by_monitor(
        &self,
        monitor_id: MonitorId,
    ) -> Result<Option<Incident>, Box<dyn std::error::Error + Send + Sync>> {
        let state = self.state.lock().unwrap();

        if state.open_incident_exists {
            Ok(Some(Incident::new(IncidentId(Uuid::new_v4()), monitor_id)))
        } else {
            Ok(None)
        }
    }

    async fn list_by_monitor(
        &self,
        _monitor_id: MonitorId,
    ) -> Result<Vec<Incident>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![])
    }
}

/// Always returns a fixed list of channels for any monitor — good enough
/// for these unit tests, which only care about `IncidentManager` calling
/// `list_enabled_by_monitor` and then `NotificationService::notify` once
/// per returned channel.
#[derive(Clone, Default)]
struct MockNotificationChannelRepository {
    channels: Vec<NotificationChannel>,
}

impl MockNotificationChannelRepository {
    fn with_channels(channels: Vec<NotificationChannel>) -> Self {
        Self { channels }
    }
}

#[async_trait]
impl NotificationChannelRepository for MockNotificationChannelRepository {
    async fn create(
        &self,
        _channel: &NotificationChannel,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    async fn delete_for_user(
        &self,
        _id: NotificationChannelId,
        _user_id: UserId,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        Ok(false)
    }

    async fn find_by_id_for_user(
        &self,
        _id: NotificationChannelId,
        _user_id: UserId,
    ) -> Result<Option<NotificationChannel>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }

    async fn list_by_monitor_for_user(
        &self,
        _monitor_id: MonitorId,
        _user_id: UserId,
    ) -> Result<Option<Vec<NotificationChannel>>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Some(self.channels.clone()))
    }

    async fn list_enabled_by_monitor(
        &self,
        _monitor_id: MonitorId,
    ) -> Result<Vec<NotificationChannel>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.channels.clone())
    }
}

/// Records every `notify` call, and can be configured to always fail —
/// used by `incident_creation_survives_notification_failure` below to
/// prove a broken notification provider can never stop an incident from
/// being persisted.
#[derive(Clone, Default)]
struct MockNotificationService {
    calls: Arc<Mutex<Vec<(NotificationChannelId, &'static str)>>>,
    should_fail: bool,
}

impl MockNotificationService {
    fn new(should_fail: bool) -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            should_fail,
        }
    }

    fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }

    fn calls(&self) -> Vec<(NotificationChannelId, &'static str)> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl NotificationService for MockNotificationService {
    async fn notify(
        &self,
        channel: &NotificationChannel,
        event: &NotificationEvent,
    ) -> NotifyResult {
        let kind = match event {
            NotificationEvent::MonitorDown(_) => "down",
            NotificationEvent::MonitorRecovered(_) => "recovered",
            NotificationEvent::Test(_) => "test",
        };

        self.calls.lock().unwrap().push((channel.id, kind));

        if self.should_fail {
            Err("simulated notification provider failure".into())
        } else {
            Ok(())
        }
    }
}

fn sample_email_channel(monitor: &Monitor) -> NotificationChannel {
    use domain::entities::CreateEmailChannel;
    use domain::value_objects::Email;

    NotificationChannel::create_email(CreateEmailChannel {
        id: NotificationChannelId(Uuid::new_v4()),
        monitor_id: monitor.id,
        user_id: monitor.user_id,
        email: Email::new("owner@example.com").unwrap(),
    })
}

#[tokio::test]
async fn opens_incident_when_none_exists() {
    let monitor = build_monitor();
    let repository = MockIncidentRepository::new(false);
    let id_generator = Arc::new(infra::UuidIdGenerator::new());
    let notification_channel_repository = Arc::new(MockNotificationChannelRepository::default());
    let notification_service = Arc::new(MockNotificationService::new(false));

    let manager = IncidentManager::new(
        Arc::new(repository.clone()),
        id_generator,
        notification_channel_repository,
        notification_service,
    );

    manager.open_if_needed(&monitor).await.unwrap();

    assert_eq!(repository.save_calls(), 1);
    assert_eq!(repository.update_calls(), 0);
}

#[tokio::test]
async fn does_not_open_duplicate_incident() {
    let monitor = build_monitor();
    let repository = MockIncidentRepository::new(true);
    let id_generator = Arc::new(infra::UuidIdGenerator::new());
    let notification_channel_repository = Arc::new(MockNotificationChannelRepository::default());
    let notification_service = Arc::new(MockNotificationService::new(false));

    let manager = IncidentManager::new(
        Arc::new(repository.clone()),
        id_generator,
        notification_channel_repository,
        notification_service.clone(),
    );

    manager.open_if_needed(&monitor).await.unwrap();

    assert_eq!(repository.save_calls(), 0);
    assert_eq!(repository.update_calls(), 0);
    // No NEW incident was opened, so channels must not be notified either.
    assert_eq!(notification_service.call_count(), 0);
}

#[tokio::test]
async fn resolves_open_incident() {
    let monitor = build_monitor();
    let repository = MockIncidentRepository::new(true);
    let id_generator = Arc::new(infra::UuidIdGenerator::new());
    let notification_channel_repository = Arc::new(MockNotificationChannelRepository::default());
    let notification_service = Arc::new(MockNotificationService::new(false));

    let manager = IncidentManager::new(
        Arc::new(repository.clone()),
        id_generator,
        notification_channel_repository,
        notification_service,
    );

    manager.resolve_if_needed(&monitor).await.unwrap();

    assert_eq!(repository.save_calls(), 0);
    assert_eq!(repository.update_calls(), 1);
}

#[tokio::test]
async fn notifies_every_enabled_channel_when_incident_opens() {
    let monitor = build_monitor();
    let channel = sample_email_channel(&monitor);
    let channel_id = channel.id;

    let repository = MockIncidentRepository::new(false);
    let id_generator = Arc::new(infra::UuidIdGenerator::new());
    let notification_channel_repository =
        Arc::new(MockNotificationChannelRepository::with_channels(vec![
            channel,
        ]));
    let notification_service = Arc::new(MockNotificationService::new(false));

    let manager = IncidentManager::new(
        Arc::new(repository),
        id_generator,
        notification_channel_repository,
        notification_service.clone(),
    );

    manager.open_if_needed(&monitor).await.unwrap();

    assert_eq!(notification_service.call_count(), 1);
    assert_eq!(notification_service.calls(), vec![(channel_id, "down")]);
}

#[tokio::test]
async fn notifies_every_enabled_channel_when_incident_resolves() {
    let monitor = build_monitor();
    let channel = sample_email_channel(&monitor);
    let channel_id = channel.id;

    let repository = MockIncidentRepository::new(true);
    let id_generator = Arc::new(infra::UuidIdGenerator::new());
    let notification_channel_repository =
        Arc::new(MockNotificationChannelRepository::with_channels(vec![
            channel,
        ]));
    let notification_service = Arc::new(MockNotificationService::new(false));

    let manager = IncidentManager::new(
        Arc::new(repository),
        id_generator,
        notification_channel_repository,
        notification_service.clone(),
    );

    manager.resolve_if_needed(&monitor).await.unwrap();

    assert_eq!(notification_service.call_count(), 1);
    assert_eq!(
        notification_service.calls(),
        vec![(channel_id, "recovered")]
    );
}

/// The critical regression test from the feature brief: a failing
/// notification provider (SMTP down, webhook endpoint unreachable, ...)
/// must NEVER prevent the incident itself from being recorded. This
/// mirrors `does_not_open_duplicate_incident` in spirit — both prove
/// `open_if_needed`'s core job (persist the incident) is unaffected by
/// concerns that are logically "downstream" of it.
#[tokio::test]
async fn incident_creation_survives_notification_failure() {
    let monitor = build_monitor();
    let channel = sample_email_channel(&monitor);

    let repository = MockIncidentRepository::new(false);
    let id_generator = Arc::new(infra::UuidIdGenerator::new());
    let notification_channel_repository =
        Arc::new(MockNotificationChannelRepository::with_channels(vec![
            channel,
        ]));
    // Configured to ALWAYS fail.
    let notification_service = Arc::new(MockNotificationService::new(true));

    let manager = IncidentManager::new(
        Arc::new(repository.clone()),
        id_generator,
        notification_channel_repository,
        notification_service.clone(),
    );

    // The key assertion: this must return Ok, not propagate the
    // notification error.
    let result = manager.open_if_needed(&monitor).await;
    assert!(result.is_ok(), "expected Ok(()), got {result:?}");

    // The incident was still saved...
    assert_eq!(repository.save_calls(), 1);
    // ...even though a send was attempted and failed.
    assert_eq!(notification_service.call_count(), 1);
}

/// Same regression, for the resolve path.
#[tokio::test]
async fn incident_resolution_survives_notification_failure() {
    let monitor = build_monitor();
    let channel = sample_email_channel(&monitor);

    let repository = MockIncidentRepository::new(true);
    let id_generator = Arc::new(infra::UuidIdGenerator::new());
    let notification_channel_repository =
        Arc::new(MockNotificationChannelRepository::with_channels(vec![
            channel,
        ]));
    let notification_service = Arc::new(MockNotificationService::new(true));

    let manager = IncidentManager::new(
        Arc::new(repository.clone()),
        id_generator,
        notification_channel_repository,
        notification_service.clone(),
    );

    let result = manager.resolve_if_needed(&monitor).await;
    assert!(result.is_ok(), "expected Ok(()), got {result:?}");

    assert_eq!(repository.update_calls(), 1);
    assert_eq!(notification_service.call_count(), 1);
}
