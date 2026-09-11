//! Task 14 acceptance test: single-region vs multi-region execution routing.
//!
//! The scheduler and the regional-agent lease repo each claim only the
//! monitors they are responsible for. This test seeds one monitor for
//! each (kind, region count) combination and asserts that the two
//! claim paths partition the set exactly — no monitor can be claimed
//! by both, and no monitor falls through the gap.
//!
//! The contract, read directly from the two claim SQL statements:
//!
//! | kind             | regions | scheduler | agent |
//! |------------------|---------|-----------|-------|
//! | http             | 1       | yes       | no    |
//! | http             | >1      | no        | yes   |
//! | tcp/dns/flow     | any     | yes       | no    |
//!
//! Regional agents currently only run HTTP checks (the agent claim SQL
//! requires `m.kind = 'http'`), so non-HTTP monitors stay on the
//! scheduler regardless of how many regions they list.

use chrono::Utc;
use domain::shared::ids::UserId;
use infra::repositories::postgres_agent_lease_repository::PostgresAgentLeaseRepository;
use infra::repositories::postgres_scheduling_repository::PostgresSchedulingRepository;
use application::ports::{AgentLeaseRepository, SchedulingRepository};
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

mod common;

/// Inserts a due monitor with the given kind and regions. Returns the
/// monitor id so the test can assert which claim path saw it.
async fn insert_monitor(
    pool: &PgPool,
    name: &str,
    user_id: Uuid,
    tenant_id: Uuid,
    kind: &str,
    regions: &[&str],
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO monitors (
            id, user_id, tenant_id, regions, name, url, method,
            expected_status, interval_seconds, timeout_seconds,
            enabled, ssl_check_enabled, ssl_expiry_warning_days, kind,
            created_at, updated_at, next_due_at
        )
        VALUES (
            $1, $2, $3, $4, $5,
            'http://example.com',
            'GET', 200, 60, 10,
            true, true, 14, $6,
            now(), now(), now() - interval '1 minute'
        )
        "#,
    )
    .bind(id)
    .bind(user_id)
    .bind(tenant_id)
    .bind(regions)
    .bind(name)
    .bind(kind)
    .execute(pool)
    .await
    .unwrap();
    id
}

#[tokio::test]
#[serial_test::serial]
async fn execution_routing_partitions_monitors_by_kind_and_regions() {
    let pool = common::setup_pool().await;
    let user_id = Uuid::new_v4();
    let tenant_id = common::insert_user(&pool, UserId(user_id)).await;

    // Four monitors covering every routing branch.
    let http_single = insert_monitor(
        &pool,
        "http-single",
        user_id,
        tenant_id.0,
        "http",
        &["us-east"],
    )
    .await;
    let http_multi = insert_monitor(
        &pool,
        "http-multi",
        user_id,
        tenant_id.0,
        "http",
        &["us-east", "eu-west"],
    )
    .await;
    let tcp_single = insert_monitor(
        &pool,
        "tcp-single",
        user_id,
        tenant_id.0,
        "tcp",
        &["us-east"],
    )
    .await;
    let tcp_multi = insert_monitor(
        &pool,
        "tcp-multi",
        user_id,
        tenant_id.0,
        "tcp",
        &["us-east", "eu-west"],
    )
    .await;

    // ---- Scheduler claim (no region arg; it claims "the scheduler's" set)
    let scheduler = PostgresSchedulingRepository::new(pool.clone());
    let claimed = scheduler
        .claim_due_monitors(100, Uuid::new_v4(), Duration::from_secs(60), Utc::now())
        .await
        .unwrap();
    let scheduler_claimed_ids: std::collections::HashSet<Uuid> =
        claimed.iter().map(|c| c.monitor.id.0).collect();

    // ---- Agent claim from us-east, where http-multi and tcp-multi list us-east
    let agents = PostgresAgentLeaseRepository::new(pool.clone());
    let agent_leases = agents
        .claim_due_checks(
            "agent-us-east-1",
            &domain::RegionCode::new("us-east").unwrap(),
            100,
            Duration::from_secs(120),
            Utc::now(),
        )
        .await
        .unwrap();
    let agent_claimed_ids: std::collections::HashSet<Uuid> =
        agent_leases.iter().map(|l| l.monitor_id.0).collect();

    // ---- Contract assertions ----------------------------------------

    // HTTP single-region: scheduler only.
    assert!(
        scheduler_claimed_ids.contains(&http_single),
        "HTTP single-region must be claimed by the scheduler"
    );
    assert!(
        !agent_claimed_ids.contains(&http_single),
        "HTTP single-region must NOT be claimed by a regional agent"
    );

    // HTTP multi-region: agent only.
    assert!(
        !scheduler_claimed_ids.contains(&http_multi),
        "HTTP multi-region must NOT be claimed by the scheduler"
    );
    assert!(
        agent_claimed_ids.contains(&http_multi),
        "HTTP multi-region must be claimed by a regional agent"
    );

    // TCP single-region: scheduler only.
    assert!(
        scheduler_claimed_ids.contains(&tcp_single),
        "TCP single-region must be claimed by the scheduler"
    );
    assert!(
        !agent_claimed_ids.contains(&tcp_single),
        "TCP single-region must NOT be claimed by a regional agent"
    );

    // TCP multi-region: scheduler only.
    // Regional agents run HTTP checks only (the agent claim SQL requires
    // `m.kind = 'http'`). A TCP monitor listing multiple regions still
    // stays on the scheduler.
    assert!(
        scheduler_claimed_ids.contains(&tcp_multi),
        "TCP multi-region must be claimed by the scheduler (agents run HTTP only)"
    );
    assert!(
        !agent_claimed_ids.contains(&tcp_multi),
        "TCP multi-region must NOT be claimed by a regional agent (agent SQL requires kind='http')"
    );

    // No monitor is claimed by both paths.
    let overlap: Vec<&Uuid> = scheduler_claimed_ids
        .intersection(&agent_claimed_ids)
        .collect();
    assert!(
        overlap.is_empty(),
        "no monitor may be claimed by both the scheduler and an agent; overlap={overlap:?}"
    );
}
