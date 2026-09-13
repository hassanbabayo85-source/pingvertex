use application::ports::{UsageEventRepository, WalletRepository};
use application::{ChargeForUsageInput, ChargeForUsageUseCase};
use chrono::{Duration, Utc};
use infra::{
    InMemoryPAYGPricingRepository, InMemoryUsageEventRepository, InMemoryWalletRepository,
};
use std::sync::Arc;
use uuid::Uuid;

async fn setup(
    price: i64,
) -> (
    Arc<InMemoryWalletRepository>,
    Arc<InMemoryPAYGPricingRepository>,
    Arc<InMemoryUsageEventRepository>,
    Uuid,
) {
    let wallet = Arc::new(InMemoryWalletRepository::default());
    let pricing = Arc::new(InMemoryPAYGPricingRepository::new());
    let _ = application::ports::PAYGPricingRepository::create_version(
        &*pricing,
        "http_check.monitoring",
        "check",
        price,
        "USD",
        chrono::Utc::now() - chrono::Duration::minutes(1),
    )
    .await;
    let usage = Arc::new(InMemoryUsageEventRepository::default());
    (wallet, pricing, usage, Uuid::new_v4())
}

#[tokio::test]
async fn same_request_id_is_idempotent_under_concurrency() {
    let (wallet, pricing, usage, tenant) = setup(2).await;
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(
            tenant,
            100,
            domain::WalletTransactionType::TopUp,
            "topup",
            None,
        )
        .await
        .unwrap();

    let uc = Arc::new(ChargeForUsageUseCase::new(
        wallet.clone(),
        pricing,
        usage.clone(),
    ));
    let input = ChargeForUsageInput {
        tenant_id: tenant,
        operation: "http_check.monitoring".into(),
        units: 1,
        request_id: "same-request".into(),

        monitor_id: None,
        execution_id: None,
        pricing_context: None,
    };

    let mut tasks = Vec::new();
    for _ in 0..8 {
        let uc = uc.clone();
        let input = input.clone();
        tasks.push(tokio::spawn(async move {
            uc.execute(input, || async { Ok::<_, &'static str>(()) })
                .await
        }));
    }

    let mut successes = 0;
    for task in tasks {
        if task.await.unwrap().is_ok() {
            successes += 1;
        }
    }
    assert_eq!(successes, 8);

    let wallet = wallet.get_or_create(tenant, "USD").await.unwrap();
    assert_eq!(wallet.balance_units, 98);

    let event = usage
        .find_by_request(tenant, "same-request")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event.charge_units, 2);
}

#[tokio::test]
async fn reservation_expiry_does_not_allow_overspending() {
    let (wallet, _pricing, _usage, tenant) = setup(1).await;
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(
            tenant,
            10,
            domain::WalletTransactionType::TopUp,
            "topup",
            None,
        )
        .await
        .unwrap();

    let expired = wallet
        .reserve(
            tenant,
            7,
            "http_check.monitoring",
            100,
            1,
            "USD",
            "expired",
            Utc::now() - Duration::seconds(1),
        )
        .await
        .unwrap();

    assert!(expired.created);

    let next = wallet
        .reserve(
            tenant,
            7,
            "http_check.monitoring",
            100,
            1,
            "USD",
            "fresh",
            Utc::now() + Duration::minutes(1),
        )
        .await
        .unwrap();
    assert!(
        next.created,
        "expired reservation must not permanently lock credits"
    );
}

#[tokio::test]
async fn runtime_operations_are_idempotent_and_cannot_double_charge() {
    let tenant = Uuid::new_v4();
    let wallet = Arc::new(InMemoryWalletRepository::default());
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(
            tenant,
            10_000,
            domain::WalletTransactionType::TopUp,
            "runtime-topup",
            None,
        )
        .await
        .unwrap();

    let now = Utc::now() - Duration::minutes(1);
    let pricing = Arc::new(InMemoryPAYGPricingRepository::new());
    for (op, price) in [
        ("sms", 1000),
        ("rum_event", 1),
        ("network_egress", 100),
        ("heartbeat_processing", 2),
    ] {
        let _ = application::ports::PAYGPricingRepository::create_version(
            &*pricing, op, "unit", price, "USD", now,
        )
        .await;
    }
    let usage = Arc::new(InMemoryUsageEventRepository::default());
    let meter = Arc::new(application::MeterUsageUseCase::new(
        wallet.clone(),
        pricing,
        usage.clone(),
    ));

    let cases = [
        ("sms", 1, 1000),
        ("rum_event", 1, 1),
        ("network_egress", 1, 100),
        ("heartbeat_processing", 1, 2),
    ];

    for (operation, units, expected_charge) in cases {
        let request_id = format!("runtime:{operation}");
        meter
            .execute(
                application::MeterUsageInput {
                    tenant_id: tenant,
                    operation: operation.to_owned(),
                    units,
                    request_id: request_id.clone(),

                    monitor_id: None,
                    execution_id: None,
                    pricing_context: None,
                },
                || async { Ok::<_, &'static str>(()) },
            )
            .await
            .unwrap();
        meter
            .execute::<_, _, &'static str>(
                application::MeterUsageInput {
                    tenant_id: tenant,
                    operation: operation.to_owned(),
                    units,
                    request_id: request_id.clone(),

                    monitor_id: None,
                    execution_id: None,
                    pricing_context: None,
                },
                || async { panic!("idempotent runtime operation was executed twice") },
            )
            .await
            .unwrap();

        let event = usage
            .find_by_request(tenant, &request_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event.charge_units, expected_charge);
    }

    let wallet = wallet.get_or_create(tenant, "USD").await.unwrap();
    assert_eq!(wallet.balance_units, 10_000 - (1000 + 1 + 100 + 2));
}

#[tokio::test]
async fn sms_dispatch_is_metered_once_for_a_retried_request() {
    use application::ports::{NotificationService, NotifyResult};
    use async_trait::async_trait;
    use domain::entities::NotificationChannel;
    use domain::notification::NotificationEvent;
    use domain::{
        CheckInterval, ExpectedStatus, HttpMethod, MonitorBuilder, MonitorName, Timeout, Url,
    };
    use infra::NotificationDispatcher;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Clone)]
    struct CountingService {
        calls: Arc<AtomicUsize>,
    }
    #[async_trait]
    impl NotificationService for CountingService {
        async fn notify(
            &self,
            _channel: &NotificationChannel,
            _event: &NotificationEvent,
        ) -> NotifyResult {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    let tenant = Uuid::new_v4();
    let wallet = Arc::new(InMemoryWalletRepository::default());
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(
            tenant,
            5_000,
            domain::WalletTransactionType::TopUp,
            "sms-topup",
            None,
        )
        .await
        .unwrap();
    let pricing = Arc::new(InMemoryPAYGPricingRepository::new());
    let _ = application::ports::PAYGPricingRepository::create_version(
        &*pricing,
        "sms",
        "message",
        1000,
        "USD",
        chrono::Utc::now() - chrono::Duration::minutes(1),
    )
    .await;
    let usage = Arc::new(InMemoryUsageEventRepository::default());
    let meter = Arc::new(application::MeterUsageUseCase::new(
        wallet.clone(),
        pricing,
        usage.clone(),
    ));
    let calls = Arc::new(AtomicUsize::new(0));
    let service = CountingService {
        calls: calls.clone(),
    };
    let dispatcher = NotificationDispatcher::new(
        Arc::new(service.clone()),
        Arc::new(service.clone()),
        Arc::new(service.clone()),
        Arc::new(service.clone()),
        Arc::new(service.clone()),
        Arc::new(service),
    )
    .with_metering(meter);

    let monitor = MonitorBuilder::new(
        domain::MonitorId(Uuid::new_v4()),
        domain::UserId(Uuid::new_v4()),
        domain::TenantId(tenant),
    )
    .name(MonitorName::new("SMS test").unwrap())
    .url(Url::new("https://example.com").unwrap())
    .method(HttpMethod::Get)
    .expected_status(ExpectedStatus::new(200).unwrap())
    .interval(CheckInterval::new(60).unwrap())
    .timeout(Timeout::new(5).unwrap())
    .build();
    let channel = NotificationChannel::create_sms(
        domain::NotificationChannelId(Uuid::new_v4()),
        monitor.id,
        monitor.user_id,
        domain::TenantId(tenant),
        &domain::value_objects::PhoneNumber::new("+15555550100").unwrap(),
    );
    let event = NotificationEvent::Test(monitor);

    dispatcher
        .notify_with_request_id(&channel, &event, "outbox:sms:1")
        .await
        .unwrap();
    dispatcher
        .notify_with_request_id(&channel, &event, "outbox:sms:1")
        .await
        .unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let wallet = wallet.get_or_create(tenant, "USD").await.unwrap();
    assert_eq!(wallet.balance_units, 4_000);
}

#[tokio::test]
async fn durable_network_egress_retry_does_not_double_charge() {
    let tenant = Uuid::new_v4();
    let wallet = Arc::new(InMemoryWalletRepository::default());
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(
            tenant,
            500,
            domain::WalletTransactionType::TopUp,
            "egress-topup",
            None,
        )
        .await
        .unwrap();
    let pricing = Arc::new(InMemoryPAYGPricingRepository::new());
    let _ = application::ports::PAYGPricingRepository::create_version(
        &*pricing,
        "network_egress",
        "MB",
        100,
        "USD",
        chrono::Utc::now() - chrono::Duration::minutes(1),
    )
    .await;
    let usage = Arc::new(InMemoryUsageEventRepository::default());
    let meter = Arc::new(application::MeterUsageUseCase::new(
        wallet.clone(),
        pricing,
        usage.clone(),
    ));
    let request_id = "network-egress:monitor:retry".to_owned();

    meter
        .execute_recoverable::<_, _, &'static str>(
            application::MeterUsageInput {
                tenant_id: tenant,
                operation: "network_egress".into(),
                units: 1,
                request_id: request_id.clone(),

                monitor_id: None,
                execution_id: None,
                pricing_context: None,
            },
            || async { Ok::<_, &'static str>(()) },
        )
        .await
        .unwrap();
    meter
        .execute_recoverable::<_, _, &'static str>(
            application::MeterUsageInput {
                tenant_id: tenant,
                operation: "network_egress".into(),
                units: 1,
                request_id: request_id.clone(),

                monitor_id: None,
                execution_id: None,
                pricing_context: None,
            },
            || async { panic!("durable retry executed network operation twice") },
        )
        .await
        .unwrap();

    assert_eq!(
        wallet
            .get_or_create(tenant, "USD")
            .await
            .unwrap()
            .balance_units,
        400
    );
    assert_eq!(
        usage
            .find_by_request(tenant, &request_id)
            .await
            .unwrap()
            .unwrap()
            .charge_units,
        100
    );
}

#[tokio::test]
async fn wallet_activation_is_sticky_and_does_not_fall_back_to_free() {
    let tenant = Uuid::new_v4();
    let wallet = InMemoryWalletRepository::default();
    let initial = wallet.get_or_create(tenant, "USD").await.unwrap();
    assert!(initial.payg_activated_at.is_none());

    wallet
        .credit(
            tenant,
            2_000_000,
            domain::WalletTransactionType::TopUp,
            "activation-sticky-topup",
            None,
        )
        .await
        .unwrap();
    let funded = wallet.get_or_create(tenant, "USD").await.unwrap();
    assert!(funded.payg_activated_at.is_some());

    let reservation = wallet
        .reserve(
            tenant,
            2_000_000,
            "http_check.monitoring",
            100,
            1,
            "USD",
            "activation-sticky-charge",
            chrono::Utc::now() + chrono::Duration::minutes(1),
        )
        .await
        .unwrap();
    wallet
        .capture(reservation.reservation.id, None, &[])
        .await
        .unwrap();

    let exhausted = wallet.get_or_create(tenant, "USD").await.unwrap();
    assert!(exhausted.payg_activated_at.is_some());
    assert_eq!(exhausted.balance_units, 0);
}

#[tokio::test]
async fn billing_blocks_are_tenant_scoped_and_bulk_enable_is_tenant_scoped() {
    use application::ports::MonitorRepository;
    use domain::{
        CheckInterval, ExpectedStatus, HttpMethod, MonitorBuilder, MonitorId, MonitorName,
        TenantId, Timeout, Url, UserId,
    };
    use infra::InMemoryMonitorRepository;

    let repo = InMemoryMonitorRepository::new();
    let tenant_a = TenantId(Uuid::new_v4());
    let tenant_b = TenantId(Uuid::new_v4());

    let make_monitor = |tenant: TenantId, name: &str| {
        MonitorBuilder::new(MonitorId(Uuid::new_v4()), UserId(Uuid::new_v4()), tenant)
            .name(MonitorName::new(name).unwrap())
            .url(Url::new("https://example.com").unwrap())
            .method(HttpMethod::Get)
            .expected_status(ExpectedStatus::new(200).unwrap())
            .interval(CheckInterval::new(300).unwrap())
            .timeout(Timeout::new(5).unwrap())
            .build()
    };

    let a1 = make_monitor(tenant_a, "a1");
    let a2 = make_monitor(tenant_a, "a2");
    let b1 = make_monitor(tenant_b, "b1");
    repo.save(&a1).await.unwrap();
    repo.save(&a2).await.unwrap();
    repo.save(&b1).await.unwrap();

    repo.set_billing_block(a1.id, tenant_a, "Insufficient wallet balance")
        .await
        .unwrap();
    let blocks = repo.billing_blocks_for_tenant(tenant_a).await.unwrap();
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks.get(&a1.id).unwrap(), "Insufficient wallet balance");

    let affected = repo
        .set_enabled_for_tenant(tenant_a, None, false)
        .await
        .unwrap();
    assert_eq!(affected, 2);
    assert!(!repo.find_by_id(a1.id).await.unwrap().unwrap().enabled);
    assert!(!repo.find_by_id(a2.id).await.unwrap().unwrap().enabled);
    assert!(repo.find_by_id(b1.id).await.unwrap().unwrap().enabled);

    repo.clear_billing_blocks_for_tenant(tenant_a)
        .await
        .unwrap();
    assert!(
        repo.billing_blocks_for_tenant(tenant_a)
            .await
            .unwrap()
            .is_empty()
    );
}
