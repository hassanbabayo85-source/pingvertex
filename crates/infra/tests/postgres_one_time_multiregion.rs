//! GAP 1 acceptance test: one-time checks fan out to one independent
//! execution per monitor region, and never mutate the recurring
//! scheduler's own state on `monitors`.

use application::ports::{MonitorRepository, SchedulingRepository, TenantRepository};
use domain::shared::ids::{MonitorId, TenantId, UserId};
use domain::value_objects::{CheckInterval, ExpectedStatus, MonitorName, RegionCode, Timeout};
use domain::{Monitor, MonitorBuilder, MonitorKind};
use infra::repositories::postgres_monitor_repository::PostgresMonitorRepository;
use infra::repositories::postgres_scheduling_repository::PostgresSchedulingRepository;
use infra::repositories::postgres_tenant_repository::PostgresTenantRepository;
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

/// Snapshot of a monitor's recurring scheduler columns. Used by the
/// "one-time check must not touch the recurring scheduler" test so the
/// before/after comparison is a single type instead of an inline tuple.
type MonitorSchedulerSnapshot = (
    Option<chrono::DateTime<chrono::Utc>>,
    Option<Uuid>,
    Option<chrono::DateTime<chrono::Utc>>,
    Option<Uuid>,
);

async fn get_pool() -> (PgPool, infra::test_support::ExclusiveTestGuard) {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/uptime_monitor_test".to_string()
    });
    infra::test_support::setup_pool_exclusive(&database_url).await
}

async fn insert_test_user(pool: &PgPool, user_id: UserId) {
    sqlx::query(
        r#"INSERT INTO users (id, email, password_hash, full_name, created_at, updated_at)
           VALUES ($1, $2, 'not-a-real-hash', 'GAP1 Test User', NOW(), NOW())
           ON CONFLICT (id) DO NOTHING"#,
    )
    .bind(user_id.0)
    .bind(format!("{}@example.test", user_id.0))
    .execute(pool)
    .await
    .unwrap();
}

fn build_http_monitor(user_id: UserId, tenant_id: TenantId, regions: &[&str]) -> Monitor {
    let mut monitor = MonitorBuilder::new(MonitorId::new(), user_id, tenant_id)
        .name(MonitorName::new("GAP1 Multi-region").unwrap())
        .target("https://example.com".into())
        .kind(MonitorKind::Http)
        .expected_status(ExpectedStatus::new(200).unwrap())
        .interval(CheckInterval::new(60).unwrap())
        .timeout(Timeout::new(10).unwrap())
        .try_build()
        .unwrap();
    monitor.regions = regions
        .iter()
        .map(|r| RegionCode::new(*r).unwrap())
        .collect();
    monitor
}

async fn seed_tenant_and_monitor(pool: &PgPool, regions: &[&str]) -> (TenantId, UserId, MonitorId) {
    let user_id = UserId::new();
    insert_test_user(pool, user_id).await;

    let tenant_repo = PostgresTenantRepository::new(pool.clone());
    let tenant = domain::Tenant::new(TenantId::new(), "GAP1 Tenant", user_id).unwrap();
    tenant_repo.create(&tenant).await.unwrap();

    let monitor = build_http_monitor(user_id, tenant.id, regions);
    let monitor_repo = PostgresMonitorRepository::new(pool.clone());
    monitor_repo.save(&monitor).await.unwrap();

    (tenant.id, user_id, monitor.id)
}

#[tokio::test]
#[serial_test::serial]
async fn enqueue_one_time_fans_out_one_execution_per_region() {
    let (pool, _guard) = get_pool().await;
    let (tenant_id, user_id, monitor_id) =
        seed_tenant_and_monitor(&pool, &["us-east", "eu-west", "af-west"]).await;

    let sched = PostgresSchedulingRepository::new(pool.clone());
    let request_id = format!("gap1-fanout-{}", Uuid::new_v4());

    let request = sched
        .enqueue_one_time_check(
            tenant_id,
            monitor_id,
            user_id,
            &request_id,
            &[
                RegionCode::new("us-east").unwrap(),
                RegionCode::new("eu-west").unwrap(),
                RegionCode::new("af-west").unwrap(),
            ],
        )
        .await
        .unwrap();

    let executions = sched
        .get_one_time_check_executions(request.id)
        .await
        .unwrap();

    assert_eq!(executions.len(), 3, "one independent execution per region");
    let regions: Vec<String> = executions.iter().map(|e| e.region.clone()).collect();
    assert!(regions.contains(&"us-east".to_string()));
    assert!(regions.contains(&"eu-west".to_string()));
    assert!(regions.contains(&"af-west".to_string()));

    // Each execution must carry its own unique durable execution_id so
    // wallet idempotency is per-region.
    let mut ids: Vec<Uuid> = executions.iter().map(|e| e.execution_id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 3, "every region has a unique execution_id");
}

#[tokio::test]
#[serial_test::serial]
async fn enqueue_one_time_is_idempotent_on_request_key() {
    let (pool, _guard) = get_pool().await;
    let (tenant_id, user_id, monitor_id) =
        seed_tenant_and_monitor(&pool, &["us-east", "eu-west"]).await;

    let sched = PostgresSchedulingRepository::new(pool.clone());
    let request_id = format!("gap1-idem-{}", Uuid::new_v4());

    let regions = [
        RegionCode::new("us-east").unwrap(),
        RegionCode::new("eu-west").unwrap(),
    ];

    let first = sched
        .enqueue_one_time_check(tenant_id, monitor_id, user_id, &request_id, &regions)
        .await
        .unwrap();
    let second = sched
        .enqueue_one_time_check(tenant_id, monitor_id, user_id, &request_id, &regions)
        .await
        .unwrap();

    assert_eq!(
        first.id, second.id,
        "same idempotency key → same parent request"
    );

    let executions = sched.get_one_time_check_executions(first.id).await.unwrap();
    assert_eq!(
        executions.len(),
        2,
        "replaying the same request must not create additional executions"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn enqueue_one_time_rejects_same_key_for_other_monitor() {
    let (pool, _guard) = get_pool().await;
    let (tenant_id, user_id, monitor_id) = seed_tenant_and_monitor(&pool, &["us-east"]).await;

    // Second monitor on the same tenant.
    let other_monitor = build_http_monitor(user_id, tenant_id, &["us-east"]);
    let monitor_repo = PostgresMonitorRepository::new(pool.clone());
    monitor_repo.save(&other_monitor).await.unwrap();

    let sched = PostgresSchedulingRepository::new(pool.clone());
    let request_id = format!("gap1-conflict-{}", Uuid::new_v4());
    let regions = [RegionCode::new("us-east").unwrap()];

    sched
        .enqueue_one_time_check(tenant_id, monitor_id, user_id, &request_id, &regions)
        .await
        .unwrap();

    let err = sched
        .enqueue_one_time_check(tenant_id, other_monitor.id, user_id, &request_id, &regions)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("idempotency key"),
        "expected idempotency-key conflict, got: {err}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn complete_executions_rolls_up_parent_status() {
    let (pool, _guard) = get_pool().await;
    let (tenant_id, user_id, monitor_id) =
        seed_tenant_and_monitor(&pool, &["us-east", "eu-west"]).await;

    let sched = PostgresSchedulingRepository::new(pool.clone());
    let request_id = format!("gap1-rollup-{}", Uuid::new_v4());
    let regions = [
        RegionCode::new("us-east").unwrap(),
        RegionCode::new("eu-west").unwrap(),
    ];
    let _request = sched
        .enqueue_one_time_check(tenant_id, monitor_id, user_id, &request_id, &regions)
        .await
        .unwrap();

    let owner = Uuid::new_v4();
    let claimed = sched
        .claim_one_time_executions(10, owner, Duration::from_secs(60), chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(claimed.len(), 2);

    // First region succeeds with a billable charge.
    let first = &claimed[0].execution;
    sched
        .complete_one_time_execution(
            first.id,
            owner,
            domain::OneTimeCheckExecution::SUCCEEDED,
            None,
            true,
            true,
            2,
        )
        .await
        .unwrap();

    // Parent is still RUNNING because the second region has not completed.
    let mid = sched
        .get_one_time_check_by_request_id(tenant_id, monitor_id, &request_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(mid.status, domain::OneTimeCheckRequest::RUNNING);

    // Second region fails without a billable attempt.
    let second = &claimed[1].execution;
    sched
        .complete_one_time_execution(
            second.id,
            owner,
            domain::OneTimeCheckExecution::FAILED,
            Some("pre-attempt failure"),
            false,
            false,
            0,
        )
        .await
        .unwrap();

    let final_state = sched
        .get_one_time_check_by_request_id(tenant_id, monitor_id, &request_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        final_state.status,
        domain::OneTimeCheckRequest::SUCCEEDED,
        "at least one successful region → request SUCCEEDED"
    );
    assert!(final_state.completed_at.is_some());
}

#[tokio::test]
#[serial_test::serial]
async fn one_time_enqueue_does_not_touch_recurring_scheduler_state() {
    let (pool, _guard) = get_pool().await;
    let (tenant_id, user_id, monitor_id) =
        seed_tenant_and_monitor(&pool, &["us-east", "eu-west"]).await;

    // Snapshot the recurring scheduling columns before the one-time call.
    let before: MonitorSchedulerSnapshot = sqlx::query_as(
        r#"SELECT next_due_at, lease_owner, lease_expires_at, lease_execution_id
           FROM monitors WHERE id = $1"#,
    )
    .bind(monitor_id.0)
    .fetch_one(&pool)
    .await
    .unwrap();

    let sched = PostgresSchedulingRepository::new(pool.clone());
    let regions = [
        RegionCode::new("us-east").unwrap(),
        RegionCode::new("eu-west").unwrap(),
    ];
    sched
        .enqueue_one_time_check(
            tenant_id,
            monitor_id,
            user_id,
            &format!("gap1-isolated-{}", Uuid::new_v4()),
            &regions,
        )
        .await
        .unwrap();

    let after: MonitorSchedulerSnapshot = sqlx::query_as(
        r#"SELECT next_due_at, lease_owner, lease_expires_at, lease_execution_id
           FROM monitors WHERE id = $1"#,
    )
    .bind(monitor_id.0)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(
        before, after,
        "one-time enqueue must not mutate next_due_at or any lease_* column"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn one_time_executions_are_tenant_isolated() {
    let (pool, _guard) = get_pool().await;

    let (tenant_a, user_a, monitor_a) = seed_tenant_and_monitor(&pool, &["us-east"]).await;
    let (tenant_b, user_b, monitor_b) = seed_tenant_and_monitor(&pool, &["us-east"]).await;

    let sched = PostgresSchedulingRepository::new(pool.clone());

    let request_a = sched
        .enqueue_one_time_check(
            tenant_a,
            monitor_a,
            user_a,
            &format!("gap1-iso-a-{}", Uuid::new_v4()),
            &[RegionCode::new("us-east").unwrap()],
        )
        .await
        .unwrap();
    let request_b = sched
        .enqueue_one_time_check(
            tenant_b,
            monitor_b,
            user_b,
            &format!("gap1-iso-b-{}", Uuid::new_v4()),
            &[RegionCode::new("us-east").unwrap()],
        )
        .await
        .unwrap();

    assert_ne!(request_a.id, request_b.id);

    let execs_a = sched
        .get_one_time_check_executions(request_a.id)
        .await
        .unwrap();
    let execs_b = sched
        .get_one_time_check_executions(request_b.id)
        .await
        .unwrap();

    assert_eq!(execs_a.len(), 1);
    assert_eq!(execs_b.len(), 1);
    assert_ne!(execs_a[0].execution_id, execs_b[0].execution_id);
    assert_eq!(execs_a[0].tenant_id, tenant_a.0);
    assert_eq!(execs_b[0].tenant_id, tenant_b.0);
}
