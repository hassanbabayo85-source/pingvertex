use std::sync::Arc;

use application::ports::{IncidentRepository, MonitorRepository};
use domain::shared::ids::{MonitorId, TenantId, UserId};
use domain::{CheckInterval, ExpectedStatus, HttpMethod, MonitorBuilder, MonitorName, Timeout};
use infra::{
    InMemoryMonitorRepository, InMemoryNotificationChannelRepository, NoopNotificationService,
    PostgresIncidentRepository, PostgresMonitorRepository, UuidIdGenerator,
};
use sqlx::{PgPool, query};
use uuid::Uuid;
use worker::{IncidentEvaluator, IncidentManager};

/// B23 concurrency regression test: simulates many concurrent check
/// results for the SAME monitor all landing on the same instant, and
/// confirms the incident-open path (threshold crossing +
/// `IncidentManager::open_if_needed` + the database's partial unique
/// index) still produces exactly one open incident — never a duplicate.
///
/// Uses the shared exclusive integration-test harness so schema setup and
/// test-data reset are serialized across test binaries.
async fn setup_pool_exclusive() -> (PgPool, infra::test_support::ExclusiveTestGuard) {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/uptime_monitor".to_string()
    });

    infra::test_support::setup_pool_exclusive(&database_url).await
}

async fn insert_user(pool: &PgPool, user_id: UserId) -> TenantId {
    let email = format!("{}@example.test", user_id.0);

    query(
        r#"
        INSERT INTO users (id, email, password_hash, full_name, created_at, updated_at)
        VALUES ($1, $2, 'not-a-real-hash', 'Test User', NOW(), NOW())
        ON CONFLICT (id) DO NOTHING
        "#,
    )
    .bind(user_id.0)
    .bind(email)
    .execute(pool)
    .await
    .unwrap();

    let tenant_id = TenantId::new();
    let name = format!("Tenant {}", user_id.0);

    query(
        r#"
        INSERT INTO tenants (id, name, owner_user_id, created_at)
        VALUES ($1, $2, $3, NOW())
        ON CONFLICT (id) DO NOTHING
        "#,
    )
    .bind(tenant_id.0)
    .bind(name)
    .bind(user_id.0)
    .execute(pool)
    .await
    .unwrap();

    query(
        r#"
        INSERT INTO tenant_memberships (tenant_id, user_id, role, invited_at, joined_at)
        VALUES ($1, $2, 'Owner', NOW(), NOW())
        ON CONFLICT (tenant_id, user_id) DO NOTHING
        "#,
    )
    .bind(tenant_id.0)
    .bind(user_id.0)
    .execute(pool)
    .await
    .unwrap();

    tenant_id
}

#[tokio::test]
async fn concurrent_failures_open_exactly_one_incident() {
    let (pool, _guard) = setup_pool_exclusive().await;

    let monitor_repository = PostgresMonitorRepository::new(pool.clone());
    let incident_repository = PostgresIncidentRepository::new(pool.clone());

    let user_id = UserId(Uuid::new_v4());
    let tenant_id = insert_user(&pool, user_id).await;

    let mut monitor = MonitorBuilder::new(MonitorId(Uuid::new_v4()), user_id, tenant_id)
        .name(MonitorName::new("Concurrency Target").unwrap())
        .target("http://example.com".to_string())
        .method(HttpMethod::Get)
        .expected_status(ExpectedStatus::new(200).unwrap())
        .interval(CheckInterval::new(60).unwrap())
        .timeout(Timeout::new(5).unwrap())
        .build();
    monitor.tenant_id = tenant_id;

    monitor_repository.save(&monitor).await.unwrap();

    let notification_channel_repository = Arc::new(InMemoryNotificationChannelRepository::new(
        InMemoryMonitorRepository::new(),
    ));
    let notification_service = Arc::new(NoopNotificationService::new());
    let id_generator = Arc::new(UuidIdGenerator::new());

    let incident_manager = IncidentManager::new(
        Arc::new(incident_repository.clone()),
        id_generator,
        notification_channel_repository,
        notification_service,
    );

    // Failure threshold of 1 so every one of the concurrent evaluations
    // below independently races to open an incident the instant it
    // records its failure — the worst case for the dedup mechanism.
    let evaluator = IncidentEvaluator::new(Arc::new(monitor_repository), incident_manager, 1, 2);

    let mut tasks = Vec::new();
    for _ in 0..20 {
        let evaluator = evaluator.clone();
        let monitor = monitor.clone();
        tasks.push(tokio::spawn(async move {
            evaluator.evaluate(&monitor, false).await.unwrap();
        }));
    }

    for task in tasks {
        task.await.unwrap();
    }

    let incidents = incident_repository
        .list_by_monitor(monitor.id)
        .await
        .unwrap();
    let open_incidents: Vec<_> = incidents.iter().filter(|i| i.is_open()).collect();

    assert_eq!(
        open_incidents.len(),
        1,
        "expected exactly one open incident after concurrent failure evaluations, got {}",
        open_incidents.len()
    );
}
