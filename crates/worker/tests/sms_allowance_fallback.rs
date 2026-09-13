//! GAP 3 Kashi D: dispatcher flow — allowance → wallet → email fallback.
//!
//! Uses `SpyService` mocks for email and SMS so the test can assert
//! *which* channel was invoked, without any real SMTP/Twilio I/O.

use std::sync::{Arc, Mutex};

use application::ports::{
    MonitorRepository, NotificationChannelRepository, NotificationService, NotifyResult,
    PAYGPricingRepository, TenantAlertAllowanceRepository, WalletRepository,
};
use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, Utc};
use domain::entities::{CreateEmailChannel, NotificationChannel};
use domain::enums::NotificationChannelKind;
use domain::notification::NotificationEvent;
use domain::shared::ids::{MonitorId, NotificationChannelId, TenantId, UserId};
use domain::value_objects::{
    CheckInterval, Email, ExpectedStatus, MonitorName, PhoneNumber, Timeout,
};
use domain::{HttpMethod, Monitor, MonitorBuilder, MonitorKind, WalletTransactionType};
use infra::{
    InMemoryMonitorRepository, InMemoryNotificationChannelRepository,
    InMemoryPAYGPricingRepository, InMemoryTenantAlertAllowanceRepository,
    InMemoryUsageEventRepository, InMemoryWalletRepository, NotificationDispatcher,
};

// ---------------------------------------------------------------------------
// Spy notification service
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct SpyService {
    label: &'static str,
    calls: Arc<Mutex<Vec<String>>>,
}

impl SpyService {
    fn new(label: &'static str, calls: Arc<Mutex<Vec<String>>>) -> Arc<Self> {
        Arc::new(Self { label, calls })
    }
}

#[async_trait]
impl NotificationService for SpyService {
    async fn notify(
        &self,
        channel: &NotificationChannel,
        _event: &NotificationEvent,
    ) -> NotifyResult {
        self.calls
            .lock()
            .unwrap()
            .push(format!("{}:{}", self.label, channel.kind));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn build_monitor(user_id: UserId, tenant_id: TenantId, name: &str) -> Monitor {
    MonitorBuilder::new(MonitorId::new(), user_id, tenant_id)
        .name(MonitorName::new(name).unwrap())
        .target("https://example.com".into())
        .method(HttpMethod::Get)
        .kind(MonitorKind::Http)
        .expected_status(ExpectedStatus::new(200).unwrap())
        .interval(CheckInterval::new(60).unwrap())
        .timeout(Timeout::new(5).unwrap())
        .try_build()
        .unwrap()
}

fn sms_channel(
    tenant_id: TenantId,
    monitor_id: MonitorId,
    user_id: UserId,
) -> NotificationChannel {
    let phone = PhoneNumber::new("+15555550100").unwrap();
    let mut ch = NotificationChannel::create_sms(
        NotificationChannelId::new(),
        monitor_id,
        user_id,
        tenant_id,
        &phone,
    );
    ch.target = "sms".to_string();
    ch.encrypted_secret = Some(b"encrypted-phone".to_vec());
    ch
}

fn email_channel(
    tenant_id: TenantId,
    monitor_id: MonitorId,
    user_id: UserId,
) -> NotificationChannel {
    NotificationChannel::create_email(CreateEmailChannel {
        id: NotificationChannelId::new(),
        monitor_id,
        user_id,
        tenant_id,
        email: Email::new("alerts@example.com").unwrap(),
    })
}

/// Seeds `count` enabled monitors for the tenant so the allowance
/// formula produces a non-zero SMS limit. 100 monitors → 25 SMS.
async fn seed_monitors(repo: &InMemoryMonitorRepository, user_id: UserId, tenant: TenantId, count: usize) {
    for i in 0..count {
        let m = build_monitor(user_id, tenant, &format!("m{i}"));
        repo.save(&m).await.unwrap();
    }
}

fn event_for(monitor: &Monitor) -> NotificationEvent {
    NotificationEvent::MonitorDown(monitor.clone())
}

// ---------------------------------------------------------------------------
// Test 1 — allowance covers the SMS; the wallet is never touched.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sms_within_allowance_does_not_touch_wallet() {
    let tenant = TenantId::new();
    let user_id = UserId::new();

    let monitor_repo = InMemoryMonitorRepository::new();
    seed_monitors(&monitor_repo, user_id, tenant, 100).await;

    let allowance = Arc::new(InMemoryTenantAlertAllowanceRepository::new());
    let channels = Arc::new(InMemoryNotificationChannelRepository::new(
        monitor_repo.clone(),
    ));

    let wallet = Arc::new(InMemoryWalletRepository::default());
    wallet.get_or_create(tenant.0, "USD").await.unwrap();
    wallet
        .credit(tenant.0, 10_000, WalletTransactionType::TopUp, "seed", None)
        .await
        .unwrap();

    let calls = Arc::new(Mutex::new(Vec::new()));
    let email_spy = SpyService::new("email", calls.clone());
    let sms_spy = SpyService::new("sms", calls.clone());
    let noop = SpyService::new("noop", calls.clone());

    let dispatcher = NotificationDispatcher::new(
        email_spy,
        noop.clone(),
        noop.clone(),
        noop.clone(),
        noop.clone(),
        sms_spy,
    )
    .with_alert_allowance(allowance.clone(), Arc::new(monitor_repo), channels);

    let monitor = build_monitor(user_id, tenant, "primary");
    let channel = sms_channel(tenant, monitor.id, user_id);
    let event = event_for(&monitor);

    dispatcher
        .notify_with_request_id(&channel, &event, "notification:test:1")
        .await
        .unwrap();

    let log = calls.lock().unwrap().clone();
    assert_eq!(log, vec!["sms:sms".to_string()], "SMS spy must be called once");

    // Allowance consumed.
    let a = allowance
        .current_cycle(tenant, 100, Utc::now())
        .await
        .unwrap();
    assert_eq!(a.sms_used, 1);

    // Wallet untouched (no SMS debit).
    let w = wallet.get_or_create(tenant.0, "USD").await.unwrap();
    assert_eq!(w.balance_units, 10_000);
}

// ---------------------------------------------------------------------------
// Test 2 — allowance exhausted, but the wallet has funds: SMS goes via the wallet.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sms_beyond_allowance_uses_wallet_then_succeeds() {
    let tenant = TenantId::new();
    let user_id = UserId::new();

    let monitor_repo = InMemoryMonitorRepository::new();
    // 0 enabled monitors → sms_limit = 0 → allowance cannot cover anything.
    // (No seed_monitors call.)

    let allowance = Arc::new(InMemoryTenantAlertAllowanceRepository::new());
    let channels = Arc::new(InMemoryNotificationChannelRepository::new(
        monitor_repo.clone(),
    ));

    let wallet = Arc::new(InMemoryWalletRepository::default());
    wallet.get_or_create(tenant.0, "USD").await.unwrap();
    wallet
        .credit(tenant.0, 10_000, WalletTransactionType::TopUp, "seed", None)
        .await
        .unwrap();

    // Metering needs a pricing entry for "sms". 1000 nano-USD = $0.000001.
    let pricing = Arc::new(InMemoryPAYGPricingRepository::new());
    pricing
        .create_version(
            "sms",
            "message",
            1_000,
            "USD",
            Utc::now() - ChronoDuration::minutes(1),
        )
        .await
        .unwrap();
    let usage = Arc::new(InMemoryUsageEventRepository::default());
    let metering = Arc::new(application::MeterUsageUseCase::new(
        wallet.clone(),
        pricing,
        usage.clone(),
    ));

    let calls = Arc::new(Mutex::new(Vec::new()));
    let email_spy = SpyService::new("email", calls.clone());
    let sms_spy = SpyService::new("sms", calls.clone());
    let noop = SpyService::new("noop", calls.clone());

    let dispatcher = NotificationDispatcher::new(
        email_spy,
        noop.clone(),
        noop.clone(),
        noop.clone(),
        noop.clone(),
        sms_spy,
    )
    .with_metering(metering)
    .with_alert_allowance(allowance.clone(), Arc::new(monitor_repo), channels);

    let monitor = build_monitor(user_id, tenant, "primary");
    let channel = sms_channel(tenant, monitor.id, user_id);
    let event = event_for(&monitor);

    dispatcher
        .notify_with_request_id(&channel, &event, "notification:test:2")
        .await
        .unwrap();

    let log = calls.lock().unwrap().clone();
    assert_eq!(log, vec!["sms:sms".to_string()]);

    // Wallet was debited exactly 1×1000 nano-USD.
    let w = wallet.get_or_create(tenant.0, "USD").await.unwrap();
    assert_eq!(w.balance_units, 9_000);
}

// ---------------------------------------------------------------------------
// Test 3 — allowance exhausted AND wallet empty: fall back to email.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sms_beyond_allowance_with_empty_wallet_falls_back_to_email() {
    let tenant = TenantId::new();
    let user_id = UserId::new();

    let monitor_repo = InMemoryMonitorRepository::new();
    // No monitors → sms_limit = 0.

    let allowance = Arc::new(InMemoryTenantAlertAllowanceRepository::new());
    let channels = Arc::new(InMemoryNotificationChannelRepository::new(
        monitor_repo.clone(),
    ));

    let wallet = Arc::new(InMemoryWalletRepository::default());
    wallet.get_or_create(tenant.0, "USD").await.unwrap();
    // Zero balance — no top-up.

    let pricing = Arc::new(InMemoryPAYGPricingRepository::new());
    pricing
        .create_version(
            "sms",
            "message",
            1_000,
            "USD",
            Utc::now() - ChronoDuration::minutes(1),
        )
        .await
        .unwrap();
    let usage = Arc::new(InMemoryUsageEventRepository::default());
    let metering = Arc::new(application::MeterUsageUseCase::new(
        wallet.clone(),
        pricing,
        usage.clone(),
    ));

    // Register an enabled email channel for the tenant so fallback has a target.
    let monitor = build_monitor(user_id, tenant, "primary");
    let email_ch = email_channel(tenant, monitor.id, user_id);
    channels.create(&email_ch).await.unwrap();

    let calls = Arc::new(Mutex::new(Vec::new()));
    let email_spy = SpyService::new("email", calls.clone());
    let sms_spy = SpyService::new("sms", calls.clone());
    let noop = SpyService::new("noop", calls.clone());

    let dispatcher = NotificationDispatcher::new(
        email_spy,
        noop.clone(),
        noop.clone(),
        noop.clone(),
        noop.clone(),
        sms_spy,
    )
    .with_metering(metering)
    .with_alert_allowance(allowance, Arc::new(monitor_repo), channels);

    let sms_ch = sms_channel(tenant, monitor.id, user_id);
    let event = event_for(&monitor);

    dispatcher
        .notify_with_request_id(&sms_ch, &event, "notification:test:3")
        .await
        .unwrap();

    let log = calls.lock().unwrap().clone();
    assert_eq!(
        log,
        vec!["email:email".to_string()],
        "the SMS branch must fall back to email when the allowance is exhausted and the wallet cannot pay"
    );

    // Wallet untouched.
    let w = wallet.get_or_create(tenant.0, "USD").await.unwrap();
    assert_eq!(w.balance_units, 0);
}

// ---------------------------------------------------------------------------
// Test 4 — test-send request_id ("test:" prefix) bypasses the allowance.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_send_bypasses_allowance_and_uses_wallet() {
    let tenant = TenantId::new();
    let user_id = UserId::new();

    let monitor_repo = InMemoryMonitorRepository::new();
    seed_monitors(&monitor_repo, user_id, tenant, 100).await;

    let allowance = Arc::new(InMemoryTenantAlertAllowanceRepository::new());
    // Populate a fresh cycle so we can assert sms_used stays at 0.
    allowance
        .current_cycle(tenant, 100, Utc::now())
        .await
        .unwrap();

    let channels = Arc::new(InMemoryNotificationChannelRepository::new(
        monitor_repo.clone(),
    ));

    let wallet = Arc::new(InMemoryWalletRepository::default());
    wallet.get_or_create(tenant.0, "USD").await.unwrap();
    wallet
        .credit(tenant.0, 10_000, WalletTransactionType::TopUp, "seed", None)
        .await
        .unwrap();

    let pricing = Arc::new(InMemoryPAYGPricingRepository::new());
    pricing
        .create_version(
            "sms",
            "message",
            1_000,
            "USD",
            Utc::now() - ChronoDuration::minutes(1),
        )
        .await
        .unwrap();
    let usage = Arc::new(InMemoryUsageEventRepository::default());
    let metering = Arc::new(application::MeterUsageUseCase::new(
        wallet.clone(),
        pricing,
        usage.clone(),
    ));

    let calls = Arc::new(Mutex::new(Vec::new()));
    let email_spy = SpyService::new("email", calls.clone());
    let sms_spy = SpyService::new("sms", calls.clone());
    let noop = SpyService::new("noop", calls.clone());

    let dispatcher = NotificationDispatcher::new(
        email_spy,
        noop.clone(),
        noop.clone(),
        noop.clone(),
        noop.clone(),
        sms_spy,
    )
    .with_metering(metering)
    .with_alert_allowance(allowance.clone(), Arc::new(monitor_repo), channels);

    let monitor = build_monitor(user_id, tenant, "primary");
    let channel = sms_channel(tenant, monitor.id, user_id);
    let event = event_for(&monitor);

    // request_id starts with "test:" → bypass allowance → wallet path.
    dispatcher
        .notify_with_request_id(&channel, &event, "test:sms:1")
        .await
        .unwrap();

    let log = calls.lock().unwrap().clone();
    assert_eq!(log, vec!["sms:sms".to_string()]);

    // Allowance not consumed.
    let a = allowance
        .current_cycle(tenant, 100, Utc::now())
        .await
        .unwrap();
    assert_eq!(a.sms_used, 0);

    // Wallet was debited.
    let w = wallet.get_or_create(tenant.0, "USD").await.unwrap();
    assert_eq!(w.balance_units, 9_000);
}

// ---------------------------------------------------------------------------
// Unused: silence the unused-import warning on `NotificationChannelKind`.
// ---------------------------------------------------------------------------

#[allow(dead_code)]
fn _unused() -> NotificationChannelKind {
    NotificationChannelKind::Sms
}
