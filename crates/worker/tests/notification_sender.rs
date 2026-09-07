use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use application::ports::{
    MonitorRepository, NotificationChannelRepository, NotificationOutboxSender,
    NotificationService, NotifyResult,
};
use async_trait::async_trait;
use domain::{
    NotificationOutboxEntry, NotificationOutboxEntryId,
    entities::{CreateEmailChannel, CreateMonitor, Monitor, NotificationChannel},
    enums::{HttpMethod, MonitorKind, NotificationChannelKind, NotificationOutboxEventKind},
    notification::NotificationEvent,
    shared::ids::{MonitorId, NotificationChannelId, TenantId, UserId},
    value_objects::{Email, MonitorName},
};
use infra::InMemoryMonitorRepository;
use infra::InMemoryNotificationChannelRepository;
use uuid::Uuid;
use worker::{RealNotificationOutboxSender, TenantNotificationRateLimiter};

struct MockNotificationService {
    calls: Mutex<Vec<(NotificationChannelKind, NotificationEvent)>>,
}

impl MockNotificationService {
    fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl NotificationService for MockNotificationService {
    async fn notify(
        &self,
        channel: &NotificationChannel,
        event: &NotificationEvent,
    ) -> NotifyResult {
        self.calls
            .lock()
            .unwrap()
            .push((channel.kind, event.clone()));
        Ok(())
    }
}

fn make_monitor(tenant_id: TenantId) -> Monitor {
    Monitor::create(CreateMonitor {
        id: MonitorId(Uuid::new_v4()),
        user_id: UserId(Uuid::new_v4()),
        tenant_id,
        name: MonitorName::new("test-monitor").unwrap(),
        url: "https://example.com".to_string(),
        method: HttpMethod::Get,
        expected_status: 200,
        interval_seconds: 300,
        timeout_seconds: 10,
        regions: vec![],
        kind: MonitorKind::Http,
        tcp_port: None,
        dns_record_type: None,
        dns_expected_value: None,
        heartbeat_token: None,
        heartbeat_interval_seconds: None,
        heartbeat_grace_period_seconds: None,
        last_heartbeat_at: None,
    })
    .unwrap()
}

fn make_email_channel(monitor_id: MonitorId, tenant_id: TenantId) -> NotificationChannel {
    NotificationChannel::create_email(CreateEmailChannel {
        id: NotificationChannelId(Uuid::new_v4()),
        monitor_id,
        user_id: UserId(Uuid::new_v4()),
        tenant_id,
        email: Email::new("alerts@example.com").unwrap(),
    })
}

#[tokio::test]
async fn real_sender_resolves_channel_and_dispatches_event() {
    let tenant_id = TenantId(Uuid::new_v4());
    let monitor = make_monitor(tenant_id);

    // InMemoryNotificationChannelRepository wants the InMemoryMonitorRepository
    // by value, so we create it as a plain value, register the monitor, then
    // move it into the channel repository. After that we no longer need the
    // original monitor repository.
    let monitor_repo = InMemoryMonitorRepository::new();
    <InMemoryMonitorRepository as MonitorRepository>::save(&monitor_repo, &monitor)
        .await
        .unwrap();

    let channel_repo: Arc<dyn NotificationChannelRepository> =
        Arc::new(InMemoryNotificationChannelRepository::new(monitor_repo));

    let channel = make_email_channel(monitor.id, tenant_id);
    channel_repo.create(&channel).await.unwrap();

    let mock_service = Arc::new(MockNotificationService::new());
    let rate_limiter = Arc::new(TenantNotificationRateLimiter::new(
        60,
        Duration::from_secs(60),
    ));
    let sender =
        RealNotificationOutboxSender::new(mock_service.clone(), channel_repo, rate_limiter);

    let entry = NotificationOutboxEntry::new(
        NotificationOutboxEntryId::new(),
        channel.id,
        NotificationOutboxEventKind::MonitorDown,
        serde_json::to_string(&monitor).unwrap(),
        format!("idem-{}", Uuid::new_v4()),
        chrono::Utc::now(),
    );

    sender.send(&entry).await.unwrap();

    let calls = mock_service.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, NotificationChannelKind::Email);
    assert!(matches!(calls[0].1, NotificationEvent::MonitorDown(_)));
}
