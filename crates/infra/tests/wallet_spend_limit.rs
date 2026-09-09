use application::ports::WalletRepository;
use chrono::{Duration, Utc};
use domain::WalletSettings;
use infra::InMemoryWalletRepository;
use uuid::Uuid;

fn settings_with_limits(
    tenant_id: Uuid,
    daily: Option<i64>,
    monthly: Option<i64>,
) -> WalletSettings {
    WalletSettings {
        tenant_id,
        daily_spend_limit_units: daily,
        monthly_spend_limit_units: monthly,
        ..WalletSettings::default()
    }
}

async fn funded_wallet(wallet: &InMemoryWalletRepository, tenant: Uuid, amount: i64) {
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(
            tenant,
            amount,
            domain::WalletTransactionType::TopUp,
            "seed-topup",
            None,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn reservation_within_daily_limit_is_accepted() {
    let wallet = InMemoryWalletRepository::default();
    let tenant = Uuid::new_v4();
    funded_wallet(&wallet, tenant, 1000).await;
    wallet
        .update_settings(&settings_with_limits(tenant, Some(50), None))
        .await
        .unwrap();

    let outcome = wallet
        .reserve(
            tenant,
            30,
            "http_check.monitoring",
            10,
            1,
            "USD",
            "req-within-limit",
            Utc::now() + Duration::minutes(5),
        )
        .await
        .unwrap();

    assert!(outcome.created);
    assert_eq!(outcome.reservation.amount_units, 30);
}

#[tokio::test]
async fn reservation_exceeding_daily_limit_is_rejected() {
    let wallet = InMemoryWalletRepository::default();
    let tenant = Uuid::new_v4();
    funded_wallet(&wallet, tenant, 1000).await;
    wallet
        .update_settings(&settings_with_limits(tenant, Some(50), None))
        .await
        .unwrap();

    let result = wallet
        .reserve(
            tenant,
            60,
            "http_check.monitoring",
            10,
            1,
            "USD",
            "req-over-daily-limit",
            Utc::now() + Duration::minutes(5),
        )
        .await;

    assert_eq!(
        result.unwrap_err().to_string(),
        "wallet daily spend limit exceeded"
    );
}

#[tokio::test]
async fn reservation_exceeding_monthly_limit_is_rejected() {
    let wallet = InMemoryWalletRepository::default();
    let tenant = Uuid::new_v4();
    funded_wallet(&wallet, tenant, 1000).await;
    wallet
        .update_settings(&settings_with_limits(tenant, None, Some(50)))
        .await
        .unwrap();

    let result = wallet
        .reserve(
            tenant,
            60,
            "http_check.monitoring",
            10,
            1,
            "USD",
            "req-over-monthly-limit",
            Utc::now() + Duration::minutes(5),
        )
        .await;

    assert_eq!(
        result.unwrap_err().to_string(),
        "wallet monthly spend limit exceeded"
    );
}

#[tokio::test]
async fn pending_unexpired_reservations_count_toward_the_limit() {
    let wallet = InMemoryWalletRepository::default();
    let tenant = Uuid::new_v4();
    funded_wallet(&wallet, tenant, 1000).await;
    wallet
        .update_settings(&settings_with_limits(tenant, Some(50), None))
        .await
        .unwrap();

    wallet
        .reserve(
            tenant,
            30,
            "http_check.monitoring",
            10,
            1,
            "USD",
            "req-first",
            Utc::now() + Duration::minutes(5),
        )
        .await
        .unwrap();

    let result = wallet
        .reserve(
            tenant,
            30,
            "http_check.monitoring",
            10,
            1,
            "USD",
            "req-second",
            Utc::now() + Duration::minutes(5),
        )
        .await;

    assert_eq!(
        result.unwrap_err().to_string(),
        "wallet daily spend limit exceeded"
    );
}

#[tokio::test]
async fn expired_reservation_reopen_is_still_subject_to_the_spend_limit() {
    let wallet = InMemoryWalletRepository::default();
    let tenant = Uuid::new_v4();
    funded_wallet(&wallet, tenant, 1000).await;

    wallet
        .reserve(
            tenant,
            60,
            "http_check.monitoring",
            10,
            1,
            "USD",
            "req-will-expire",
            Utc::now() - Duration::seconds(1),
        )
        .await
        .unwrap();

    wallet
        .update_settings(&settings_with_limits(tenant, Some(50), None))
        .await
        .unwrap();

    let result = wallet
        .reserve(
            tenant,
            60,
            "http_check.monitoring",
            10,
            1,
            "USD",
            "req-will-expire",
            Utc::now() + Duration::minutes(5),
        )
        .await;

    assert_eq!(
        result.unwrap_err().to_string(),
        "wallet daily spend limit exceeded"
    );
}

#[tokio::test]
async fn expired_reservation_reopen_succeeds_when_within_the_limit() {
    let wallet = InMemoryWalletRepository::default();
    let tenant = Uuid::new_v4();
    funded_wallet(&wallet, tenant, 1000).await;
    wallet
        .update_settings(&settings_with_limits(tenant, Some(50), None))
        .await
        .unwrap();

    wallet
        .reserve(
            tenant,
            30,
            "http_check.monitoring",
            10,
            1,
            "USD",
            "req-reopen-ok",
            Utc::now() - Duration::seconds(1),
        )
        .await
        .unwrap();

    let outcome = wallet
        .reserve(
            tenant,
            30,
            "http_check.monitoring",
            10,
            1,
            "USD",
            "req-reopen-ok",
            Utc::now() + Duration::minutes(5),
        )
        .await
        .unwrap();

    assert!(outcome.created);
    assert_eq!(
        outcome.reservation.status,
        domain::CreditReservation::PENDING
    );
}

#[tokio::test]
async fn no_spend_limit_configured_behaves_as_before() {
    let wallet = InMemoryWalletRepository::default();
    let tenant = Uuid::new_v4();
    funded_wallet(&wallet, tenant, 1000).await;

    let outcome = wallet
        .reserve(
            tenant,
            900,
            "http_check.monitoring",
            10,
            1,
            "USD",
            "req-no-limit",
            Utc::now() + Duration::minutes(5),
        )
        .await
        .unwrap();

    assert!(outcome.created);
    assert_eq!(outcome.reservation.amount_units, 900);
}
