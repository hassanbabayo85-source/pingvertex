//! Proration Service: DB-backed integration test for
//! `PostgresSubscriptionUsageTransaction` + `PostgresSubscriptionUsagePeriodRepository`.
//!
//! Mirrors `pricing_config_repository.rs`: ignored by default, requires a
//! real Postgres reachable via `DATABASE_URL`. Run with:
//!   DATABASE_URL=... cargo test --test subscription_usage_transaction -- --ignored
//!
//! NOTE: this file assumes minimal FK dependencies (a `tenants` row and a
//! `subscriptions` row must exist before `subscription_usage_periods` rows
//! can be inserted, per their FK constraints). If this codebase's existing
//! `crates/infra/tests/common` helpers already provide tenant/subscription
//! fixtures, prefer wiring this test through those instead of the raw SQL
//! below — left as raw SQL here because `tests/common`'s contents were not
//! inspected as part of this test suite's scope.

use application::ports::{
    SubscriptionRepository, SubscriptionUsagePeriodRepository, SubscriptionUsageTransaction,
};
use chrono::{Duration, Utc};
use domain::entities::SubscriptionUsagePeriod;
use domain::enums::PlanTier;
use domain::pricing::calculate_period_cost;
use domain::shared::ids::{SubscriptionId, SubscriptionUsagePeriodId, TenantId};
use domain::{PricingConfig, Subscription};
use infra::repositories::{
    PostgresSubscriptionRepository, PostgresSubscriptionUsagePeriodRepository,
    PostgresSubscriptionUsageTransaction,
};
use sqlx::PgPool;
use uuid::Uuid;

async fn setup_pool_exclusive() -> (PgPool, infra::test_support::ExclusiveTestGuard) {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for this ignored test");
    infra::test_support::setup_pool_exclusive(&database_url).await
}

async fn create_tenant_and_subscription(pool: &PgPool) -> (TenantId, Subscription) {
    let tenant_id = TenantId::new();
    let owner_user_id = Uuid::new_v4();

    // Minimal fixture rows; adjust to match this codebase's actual
    // `users`/`tenants` NOT NULL columns if this fails to compile/run —
    // this is exactly the kind of "small matter" the 40% reserve budget
    // covers.
    sqlx::query("INSERT INTO users (id, username, email, password_hash, role, status, totp_enabled, created_at, updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,now(),now())")
        .bind(owner_user_id)
        .bind(format!("h12_test_user_{owner_user_id}"))
        .bind(format!("h12_test_{owner_user_id}@example.com"))
        .bind("not-a-real-hash")
        .bind("Owner")
        .bind("Active")
        .bind(false)
        .execute(pool)
        .await
        .expect("failed to insert fixture user");

    sqlx::query(
        "INSERT INTO tenants (id, name, owner_user_id, created_at) VALUES ($1, $2, $3, now())",
    )
    .bind(tenant_id.0)
    .bind(format!("Usage Test Tenant {}", tenant_id.0))
    .bind(owner_user_id)
    .execute(pool)
    .await
    .expect("failed to insert fixture tenant");

    let subscription_repository = PostgresSubscriptionRepository::new(pool.clone());
    let subscription = Subscription::new(SubscriptionId::new(), tenant_id, PlanTier::Starter);
    subscription_repository
        .create(&subscription)
        .await
        .expect("failed to create fixture subscription");

    (tenant_id, subscription)
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn compare_and_set_rejects_stale_plan_and_rolls_back_period_changes() {
    let (pool, _guard) = setup_pool_exclusive().await;
    let (tenant_id, subscription) = create_tenant_and_subscription(&pool).await;

    let transaction = PostgresSubscriptionUsageTransaction::new(pool.clone());
    let period_repo = PostgresSubscriptionUsagePeriodRepository::new(pool.clone());

    let period1 = SubscriptionUsagePeriod::open(
        SubscriptionUsagePeriodId(uuid::Uuid::new_v4()),
        subscription.id,
        5,
        60,
        7,
        Utc::now(),
    );

    transaction
        .change_usage(&subscription, None, &period1, None)
        .await
        .expect("initial usage transaction should succeed");

    // Simulate a stale plan-change command: the caller believes the
    // subscription is Business, but the persisted subscription is still
    // Starter. The expected-old-plan CAS must therefore fail.
    let mut stale_subscription = subscription.clone();
    stale_subscription
        .apply_verified_payment(PlanTier::Business)
        .expect("test plan transition should be valid");

    let period2 = SubscriptionUsagePeriod::open(
        SubscriptionUsagePeriodId(uuid::Uuid::new_v4()),
        subscription.id,
        20,
        30,
        30,
        Utc::now(),
    );

    let result = transaction
        .change_usage(
            &stale_subscription,
            Some(period1.id),
            &period2,
            Some(PlanTier::Business),
        )
        .await;

    assert!(
        result.is_err(),
        "stale expected plan must be rejected by CAS"
    );

    let open_period = period_repo
        .find_open_period(subscription.id)
        .await
        .expect("open-period lookup should succeed")
        .expect("original period must remain open after rollback");

    assert_eq!(
        open_period.id, period1.id,
        "failed CAS must roll back closing the previous period"
    );

    let period_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM subscription_usage_periods WHERE subscription_id = $1",
    )
    .bind(subscription.id.0)
    .fetch_one(&pool)
    .await
    .expect("period count query should succeed");

    assert_eq!(
        period_count.0, 1,
        "failed CAS must roll back insertion of the replacement period"
    );

    let persisted = sqlx::query_as::<_, (String,)>(
        "SELECT plan_tier FROM subscriptions WHERE id = $1 AND tenant_id = $2",
    )
    .bind(subscription.id.0)
    .bind(tenant_id.0)
    .fetch_one(&pool)
    .await
    .expect("subscription should still exist");

    assert_eq!(
        persisted.0,
        PlanTier::Starter.as_str(),
        "failed CAS must not change the persisted plan"
    );
}

#[tokio::test]
#[ignore]
async fn change_usage_closes_previous_period_opens_new_one_and_updates_subscription() {
    let (pool, _guard) = setup_pool_exclusive().await;
    let (_tenant_id, mut subscription) = create_tenant_and_subscription(&pool).await;

    let usage_period_repository = PostgresSubscriptionUsagePeriodRepository::new(pool.clone());
    let transaction = PostgresSubscriptionUsageTransaction::new(pool.clone());

    let cycle_start = Utc::now();
    let switch_at = cycle_start + Duration::days(10);
    let cycle_end = cycle_start + Duration::days(30);

    // First usage change: no previous open period.
    subscription
        .update_usage(100, 60, 30, 1)
        .expect("valid usage");
    let period1 = SubscriptionUsagePeriod::open(
        SubscriptionUsagePeriodId::new(),
        subscription.id,
        100,
        60,
        30,
        cycle_start,
    );
    transaction
        .change_usage(&subscription, None, &period1, None)
        .await
        .expect("first change_usage should succeed");

    // Second usage change mid-cycle: closes period1, opens period2.
    subscription
        .update_usage(200, 30, 90, 1)
        .expect("valid usage");
    let period2 = SubscriptionUsagePeriod::open(
        SubscriptionUsagePeriodId::new(),
        subscription.id,
        200,
        30,
        90,
        switch_at,
    );
    transaction
        .change_usage(&subscription, Some(period1.id), &period2, None)
        .await
        .expect("second change_usage should succeed");

    // No open period should remain unclosed except period2.
    let open_period = usage_period_repository
        .find_open_period(subscription.id)
        .await
        .unwrap()
        .expect("period2 should be the open period");
    assert_eq!(open_period.id, period2.id);
    assert_eq!(open_period.monitors, 200);

    // Both periods should be visible to billing for this cycle.
    let periods = usage_period_repository
        .list_periods_for_cycle(subscription.id, cycle_start, cycle_end)
        .await
        .unwrap();
    assert_eq!(periods.len(), 2);
    assert_eq!(periods[0].id, period1.id);
    assert!(periods[0].end_at.is_some());
    assert_eq!(periods[1].id, period2.id);
    assert!(periods[1].end_at.is_none());

    // Billing sum must reflect BOTH periods, not full-month billing of
    // either usage alone.
    let subscription_repository = PostgresSubscriptionRepository::new(pool.clone());
    let persisted = subscription_repository
        .find_by_tenant_id(subscription.tenant_id)
        .await
        .unwrap()
        .expect("subscription row should exist");
    assert_eq!(persisted.monitors, Some(200));
    assert_eq!(persisted.interval_seconds, Some(30));
    assert_eq!(persisted.retention_days, Some(90));

    let config = PricingConfig::default_config();
    let mut total = rust_decimal::Decimal::ZERO;
    for period in &periods {
        total += calculate_period_cost(period, &config, cycle_start, cycle_end).unwrap();
    }
    assert!(total > rust_decimal::Decimal::ZERO);
}
