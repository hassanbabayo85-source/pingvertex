use application::ports::{UsageEventRepository, WalletRepository};
use application::{ChargeForUsageInput, ChargeForUsageUseCase};
use chrono::{Duration, Utc};
use domain::PAYGPricing;
use infra::{
    InMemoryPAYGPricingRepository, InMemoryUsageEventRepository, InMemoryWalletRepository,
};
use std::sync::Arc;
use uuid::Uuid;

fn setup(
    price: i64,
) -> (
    Arc<InMemoryWalletRepository>,
    Arc<InMemoryPAYGPricingRepository>,
    Arc<InMemoryUsageEventRepository>,
    Uuid,
) {
    let wallet = Arc::new(InMemoryWalletRepository::default());
    let pricing = Arc::new(InMemoryPAYGPricingRepository::new(vec![PAYGPricing {
        id: Uuid::new_v4(),
        version: 1,
        operation: "proxy_check".into(),
        unit_name: "check".into(),
        unit_price: price,
        currency: "USD".into(),
        effective_from: Utc::now() - Duration::minutes(1),
        effective_until: None,
        active: true,
    }]));
    let usage = Arc::new(InMemoryUsageEventRepository::default());
    (wallet, pricing, usage, Uuid::new_v4())
}

#[tokio::test]
async fn same_request_id_is_idempotent_under_concurrency() {
    let (wallet, pricing, usage, tenant) = setup(2);
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
        operation: "proxy_check".into(),
        units: 1,
        request_id: "same-request".into(),
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
    let (wallet, _pricing, _usage, tenant) = setup(1);
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
            "proxy_check",
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
            "proxy_check",
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
    let pricing = Arc::new(InMemoryPAYGPricingRepository::new(
        [
            ("sms", 1000),
            ("rum_event", 1),
            ("storage", 20),
            ("network_egress", 100),
            ("heartbeat_ping", 2),
        ]
        .into_iter()
        .map(|(operation, unit_price)| PAYGPricing {
            id: Uuid::new_v4(),
            version: 1,
            operation: operation.to_owned(),
            unit_name: "unit".to_owned(),
            unit_price,
            currency: "USD".to_owned(),
            effective_from: now,
            effective_until: None,
            active: true,
        })
        .collect(),
    ));
    let usage = Arc::new(InMemoryUsageEventRepository::default());
    let meter = Arc::new(application::MeterUsageUseCase::new(
        wallet.clone(),
        pricing,
        usage.clone(),
    ));

    let cases = [
        ("sms", 1, 1000),
        ("rum_event", 1, 1),
        ("storage", 1, 20),
        ("network_egress", 1, 100),
        ("heartbeat_ping", 1, 2),
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
    assert_eq!(wallet.balance_units, 10_000 - (1000 + 1 + 20 + 100 + 2));
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
    let pricing = Arc::new(InMemoryPAYGPricingRepository::new(vec![PAYGPricing {
        id: Uuid::new_v4(),
        version: 2,
        operation: "sms".into(),
        unit_name: "message".into(),
        unit_price: 1000,
        currency: "USD".into(),
        effective_from: Utc::now() - Duration::minutes(1),
        effective_until: None,
        active: true,
    }]));
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
    let pricing = Arc::new(InMemoryPAYGPricingRepository::new(vec![PAYGPricing {
        id: Uuid::new_v4(),
        version: 1,
        operation: "network_egress".into(),
        unit_name: "MB".into(),
        unit_price: 100,
        currency: "USD".into(),
        effective_from: Utc::now() - Duration::minutes(1),
        effective_until: None,
        active: true,
    }]));
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
