mod common;

use application::ports::PaymentRepository;
use chrono::Utc;
use domain::entities::Payment;
use infra::repositories::PostgresPaymentRepository;
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires PostgreSQL via DATABASE_URL"]
async fn successful_payment_cannot_be_downgraded_by_failed_retry() {
    let (pool, _guard) = common::setup_pool_exclusive().await;
    let repo = PostgresPaymentRepository::new(pool.clone());

    let tenant_id = Uuid::new_v4();
    let payment_id = Uuid::new_v4();
    let provider_payment_id = "worker8-regression-success-failed";
    let idempotency_key = "worker8-regression-idem-success-failed";

    let succeeded = Payment {
        id: payment_id,
        tenant_id,
        amount_units: 1000,
        currency: "USD".to_string(),
        status: "succeeded".to_string(),
        provider: "test".to_string(),
        provider_payment_id: Some(provider_payment_id.to_string()),
        idempotency_key: idempotency_key.to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    repo.record(&succeeded).await.unwrap();

    let failed_retry = Payment {
        id: Uuid::new_v4(),
        tenant_id,
        amount_units: 1000,
        currency: "USD".to_string(),
        status: "failed".to_string(),
        provider: "test".to_string(),
        provider_payment_id: Some(provider_payment_id.to_string()),
        idempotency_key: idempotency_key.to_string(),
        created_at: succeeded.created_at,
        updated_at: Utc::now(),
    };

    repo.record(&failed_retry).await.unwrap();

    let stored = repo
        .find_by_provider_payment_id("test", provider_payment_id)
        .await
        .unwrap()
        .expect("payment must exist");

    assert_eq!(
        stored.status, "succeeded",
        "a later failed retry must never downgrade a settled payment"
    );

    sqlx::query(
        "DELETE FROM payments
         WHERE tenant_id = $1
           AND idempotency_key = $2",
    )
    .bind(tenant_id)
    .bind(idempotency_key)
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "requires PostgreSQL via DATABASE_URL"]
async fn failed_payment_can_be_upgraded_to_successful() {
    let (pool, _guard) = common::setup_pool_exclusive().await;
    let repo = PostgresPaymentRepository::new(pool.clone());

    let tenant_id = Uuid::new_v4();
    let provider_payment_id = "worker8-regression-failed-success";
    let idempotency_key = "worker8-regression-idem-failed-success";

    let failed = Payment {
        id: Uuid::new_v4(),
        tenant_id,
        amount_units: 2000,
        currency: "USD".to_string(),
        status: "failed".to_string(),
        provider: "test".to_string(),
        provider_payment_id: Some(provider_payment_id.to_string()),
        idempotency_key: idempotency_key.to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    repo.record(&failed).await.unwrap();

    let succeeded = Payment {
        id: Uuid::new_v4(),
        tenant_id,
        amount_units: 2000,
        currency: "USD".to_string(),
        status: "succeeded".to_string(),
        provider: "test".to_string(),
        provider_payment_id: Some(provider_payment_id.to_string()),
        idempotency_key: idempotency_key.to_string(),
        created_at: failed.created_at,
        updated_at: Utc::now(),
    };

    repo.record(&succeeded).await.unwrap();

    let stored = repo
        .find_by_provider_payment_id("test", provider_payment_id)
        .await
        .unwrap()
        .expect("payment must exist");

    assert_eq!(
        stored.status, "succeeded",
        "a failed payment must be allowed to transition to succeeded"
    );

    sqlx::query(
        "DELETE FROM payments
         WHERE tenant_id = $1
           AND idempotency_key = $2",
    )
    .bind(tenant_id)
    .bind(idempotency_key)
    .execute(&pool)
    .await
    .unwrap();
}
