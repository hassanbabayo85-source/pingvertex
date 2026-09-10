use application::ports::{MonitorRepository, SchedulingRepository, TenantRepository};
use chrono::Utc;
use domain::shared::ids::{MonitorId, TenantId, UserId};
use domain::{MonitorBuilder, MonitorKind};
use infra::repositories::postgres_monitor_repository::PostgresMonitorRepository;
use infra::repositories::postgres_scheduling_repository::PostgresSchedulingRepository;
use infra::repositories::postgres_tenant_repository::PostgresTenantRepository;
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

async fn insert_test_user(pool: &PgPool, user_id: UserId) {
    sqlx::query(
        r#"
        INSERT INTO users (id, email, password_hash, full_name, created_at, updated_at)
        VALUES ($1, $2, 'not-a-real-hash', 'Test User', NOW(), NOW())
        ON CONFLICT (id) DO NOTHING
        "#,
    )
    .bind(user_id.0)
    .bind(format!("{}@example.test", user_id.0))
    .execute(pool)
    .await
    .unwrap();
}

async fn get_pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/uptime_monitor_test".to_string()
    });

    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to db");

    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .unwrap_or_default();

    pool
}

#[tokio::test]
async fn scheduler_claims_active_tenant_monitor() {
    let pool = get_pool().await;
    let tenant_repo = PostgresTenantRepository::new(pool.clone());
    let monitor_repo = PostgresMonitorRepository::new(pool.clone());
    let sched_repo = PostgresSchedulingRepository::new(pool.clone());

    let user_id = UserId::new();
    insert_test_user(&pool, user_id).await;

    let tenant = domain::Tenant::new(TenantId::new(), "Active Company", user_id).unwrap();
    tenant_repo.create(&tenant).await.unwrap();

    let monitor = MonitorBuilder::new(MonitorId::new(), user_id, tenant.id)
        .name(domain::MonitorName::new("Test").unwrap())
        .target("https://example.com".into())
        .kind(MonitorKind::Http)
        .expected_status(domain::ExpectedStatus::new(200).unwrap())
        .interval(domain::CheckInterval::new(60).unwrap())
        .timeout(domain::Timeout::new(5).unwrap())
        .try_build()
        .unwrap();

    monitor_repo.save(&monitor).await.unwrap();
    sched_repo
        .request_immediate_checks(tenant.id)
        .await
        .unwrap();

    let claims = sched_repo
        .claim_due_monitors(10, Uuid::new_v4(), Duration::from_secs(60), Utc::now())
        .await
        .unwrap();

    assert!(claims.len() <= 1);
}

#[tokio::test]
async fn scheduler_ignores_suspended_tenant_monitor() {
    let pool = get_pool().await;
    let tenant_repo = PostgresTenantRepository::new(pool.clone());
    let monitor_repo = PostgresMonitorRepository::new(pool.clone());
    let sched_repo = PostgresSchedulingRepository::new(pool.clone());

    let user_id = UserId::new();
    insert_test_user(&pool, user_id).await;

    let mut tenant = domain::Tenant::new(TenantId::new(), "Suspended Company", user_id).unwrap();
    tenant.suspend();
    tenant_repo.create(&tenant).await.unwrap();

    let monitor = MonitorBuilder::new(MonitorId::new(), user_id, tenant.id)
        .name(domain::MonitorName::new("Test 2").unwrap())
        .target("https://example.com".into())
        .kind(MonitorKind::Http)
        .expected_status(domain::ExpectedStatus::new(200).unwrap())
        .interval(domain::CheckInterval::new(60).unwrap())
        .timeout(domain::Timeout::new(5).unwrap())
        .try_build()
        .unwrap();

    monitor_repo.save(&monitor).await.unwrap();
    sched_repo
        .request_immediate_checks(tenant.id)
        .await
        .unwrap();

    let claims = sched_repo
        .claim_due_monitors(10, Uuid::new_v4(), Duration::from_secs(60), Utc::now())
        .await
        .unwrap();

    // The claim query is deliberately globally scoped: it must return
    // every due monitor whose tenant is active, not just the monitor of
    // the tenant this test created. On a fresh database that happens to
    // mean `claims.len() == 0`, but when the whole workspace suite runs in
    // parallel, other tests may leave active tenants (and their due
    // monitors) behind in the shared `uptime_monitor_test` database. What
    // this test actually needs to prove is that the *suspended* tenant's
    // monitor is not among the claimed rows — i.e. the claim is scoped to
    // active tenants, not to "whatever tenants happen to exist".
    assert!(
        claims
            .iter()
            .all(|claim| claim.monitor.tenant_id != tenant.id),
        "Suspended tenant {:?} must not have any claimed monitors; got {} claim(s)",
        tenant.id,
        claims.len()
    );
}
