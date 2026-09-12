mod common;
use common::*;

use application::dto::CreateMonitorRequest;
use application::use_cases::{CreateMonitorError, CreateMonitorUseCase};
use domain::HttpMethod;
use domain::shared::ids::UserId;
use infra::{InMemoryMonitorRepository, UuidIdGenerator};

use uuid::Uuid;

#[tokio::test]
async fn create_monitor_persists_monitor() {
    let repository = InMemoryMonitorRepository::new();
    let use_case =
        CreateMonitorUseCase::new(repository.clone(), UuidIdGenerator::new(), new_audit_repo());
    let user_id = UserId(Uuid::new_v4());

    let request = CreateMonitorRequest {
        regions: Vec::new(),
        name: "Google".into(),
        url: "https://google.com".into(),
        method: HttpMethod::Get,
        expected_status: 200,
        interval_seconds: 300,
        timeout_seconds: 10,
        assertions: vec![],
        is_public: false,
        ssl_check_enabled: true,
        ssl_expiry_warning_days: 14,
        kind: domain::MonitorKind::Http,
        tcp_port: None,
        dns_record_type: None,
        dns_expected_value: None,
        heartbeat_interval_seconds: None,
        heartbeat_grace_period_seconds: None,
    };

    let monitor = use_case
        .execute(request, auth_user(user_id, tenant_id()))
        .await
        .unwrap();
    assert_eq!(monitor.name.as_str(), "Google");
}

#[tokio::test]
async fn free_tier_enforces_300_second_minimum_and_10_monitor_quota() {
    let repository = InMemoryMonitorRepository::new();
    let use_case = CreateMonitorUseCase::new(repository, UuidIdGenerator::new(), new_audit_repo());
    let user_id = UserId(Uuid::new_v4());
    let tenant_id = tenant_id();

    let request = |name: String, interval_seconds: u32| CreateMonitorRequest {
        regions: Vec::new(),
        name,
        url: "https://example.com".into(),
        method: HttpMethod::Get,
        expected_status: 200,
        interval_seconds,
        timeout_seconds: 10,
        assertions: vec![],
        is_public: false,
        ssl_check_enabled: true,
        ssl_expiry_warning_days: 14,
        kind: domain::MonitorKind::Http,
        tcp_port: None,
        dns_record_type: None,
        dns_expected_value: None,
        heartbeat_interval_seconds: None,
        heartbeat_grace_period_seconds: None,
    };

    let below_minimum = use_case
        .execute(
            request("free-299".into(), 299),
            auth_user(user_id, tenant_id),
        )
        .await;
    assert!(matches!(
        below_minimum,
        Err(CreateMonitorError::IntervalBelowPlanMinimum { min: 300 })
    ));

    for index in 0..10 {
        use_case
            .execute(
                request(format!("free-{index}"), 300),
                auth_user(user_id, tenant_id),
            )
            .await
            .unwrap();
    }

    let over_quota = use_case
        .execute(
            request("free-11".into(), 300),
            auth_user(user_id, tenant_id),
        )
        .await;
    assert!(matches!(
        over_quota,
        Err(CreateMonitorError::QuotaExceeded { limit: 10 })
    ));
}

#[tokio::test]
async fn funded_payg_wallet_has_no_product_monitor_ceiling_and_allows_fast_interval() {
    use application::ports::WalletRepository;
    use domain::WalletTransactionType;
    use infra::InMemoryWalletRepository;
    let repository = InMemoryMonitorRepository::new();
    let wallet = std::sync::Arc::new(InMemoryWalletRepository::default());
    let tid = tenant_id();
    wallet.get_or_create(tid.0, "USD").await.unwrap();
    wallet
        .credit(
            tid.0,
            5_000_000,
            WalletTransactionType::TopUp,
            "test-payg-fund",
            Some("test"),
        )
        .await
        .unwrap();
    let use_case = CreateMonitorUseCase::new(repository, UuidIdGenerator::new(), new_audit_repo())
        .with_wallet_repository(wallet);
    let uid = UserId(Uuid::new_v4());
    let request = CreateMonitorRequest {
        regions: Vec::new(),
        name: "fast".into(),
        url: "https://example.com".into(),
        method: HttpMethod::Get,
        expected_status: 200,
        interval_seconds: 30,
        timeout_seconds: 10,
        assertions: vec![],
        is_public: false,
        ssl_check_enabled: true,
        ssl_expiry_warning_days: 14,
        kind: domain::MonitorKind::Http,
        tcp_port: None,
        dns_record_type: None,
        dns_expected_value: None,
        heartbeat_interval_seconds: None,
        heartbeat_grace_period_seconds: None,
    };
    use_case
        .execute(request, auth_user(uid, tid))
        .await
        .unwrap();
}

#[tokio::test]
async fn exhausted_payg_wallet_does_not_fall_back_to_free_entitlement() {
    use application::ports::WalletRepository;
    use domain::WalletTransactionType;
    use infra::InMemoryWalletRepository;

    let repository = InMemoryMonitorRepository::new();
    let wallet = std::sync::Arc::new(InMemoryWalletRepository::default());
    let tid = tenant_id();
    wallet.get_or_create(tid.0, "USD").await.unwrap();
    wallet
        .credit(
            tid.0,
            2,
            WalletTransactionType::TopUp,
            "sticky-payg-fund",
            None,
        )
        .await
        .unwrap();
    let reservation = wallet
        .reserve(
            tid.0,
            2,
            "http_check.monitoring",
            2,
            1,
            "USD",
            "sticky-payg-charge",
            chrono::Utc::now() + chrono::Duration::minutes(1),
        )
        .await
        .unwrap();
    wallet
        .capture(reservation.reservation.id, None, &[])
        .await
        .unwrap();

    let use_case = CreateMonitorUseCase::new(repository, UuidIdGenerator::new(), new_audit_repo())
        .with_wallet_repository(wallet);

    let request = CreateMonitorRequest {
        regions: Vec::new(),
        name: "exhausted-payg".into(),
        url: "https://example.com".into(),
        method: HttpMethod::Get,
        expected_status: 200,
        interval_seconds: 30,
        timeout_seconds: 10,
        assertions: vec![],
        is_public: false,
        ssl_check_enabled: true,
        ssl_expiry_warning_days: 14,
        kind: domain::MonitorKind::Http,
        tcp_port: None,
        dns_record_type: None,
        dns_expected_value: None,
        heartbeat_interval_seconds: None,
        heartbeat_grace_period_seconds: None,
    };

    use_case
        .execute(request, auth_user(UserId(Uuid::new_v4()), tid))
        .await
        .expect("an exhausted PAYG wallet remains PAYG; it must not fall back to Free");
}
