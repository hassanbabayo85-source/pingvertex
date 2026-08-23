use std::{
    env,
    sync::{Arc, LazyLock, Mutex},
};

use application::ports::{
    EscalationPolicyRepository, IncidentRepository, MaintenanceWindowRepository, MonitorRepository,
    NotificationChannelRepository, NotificationService, NotifyResult,
};
use async_trait::async_trait;
use chrono::Utc;
use domain::{
    CheckInterval, EscalationPolicy, EscalationStep, ExpectedStatus, HttpMethod, MonitorBuilder,
    MonitorName, Timeout,
    entities::{CreateEmailChannel, NotificationChannel},
    notification::NotificationEvent,
    shared::ids::{MonitorId, NotificationChannelId, TenantId, UserId},
    value_objects::Email,
};
use infra::{
    InMemoryEscalationPolicyRepository, InMemoryMaintenanceWindowRepository,
    PostgresIncidentRepository, PostgresMonitorRepository, UuidIdGenerator, create_pool,
};
use sqlx::{PgPool, query};
use uuid::Uuid;
use worker::{EscalationWorker, IncidentManager};

/// Both tests in this file call `setup_pool()`, which does a
/// `DROP SCHEMA public CASCADE` + `CREATE SCHEMA public` + re-run
/// migrations against the SAME `DATABASE_URL`. Cargo runs tests within a
/// single test binary concurrently by default, so without this lock the
/// two tests race on the schema itself (one test's `DROP SCHEMA` can run
/// while the other is mid-`CREATE SCHEMA`, or both `CREATE SCHEMA` at
/// once — the actual failure observed: `"schema \"public\" already
/// exists"`). This mutex serializes each test's full body (setup through
/// assertions) so only one test touches the schema at a time; it does
/// NOT affect the concurrency being tested INSIDE each test (the 20
/// `tokio::spawn` tasks within a single test still run fully
/// concurrently against each other).
static TEST_LOCK: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

/// Self-contained Postgres test harness (drop schema, re-run migrations),
/// mirroring `crates/worker/tests/incident_evaluator_concurrency.rs`.
async fn setup_pool() -> PgPool {
    let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/uptime_monitor".to_string()
    });

    let pool = create_pool(&database_url).await.unwrap();

    query("DROP SCHEMA IF EXISTS public CASCADE")
        .execute(&pool)
        .await
        .unwrap();
    query("CREATE SCHEMA public").execute(&pool).await.unwrap();

    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();

    pool
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

fn build_monitor(user_id: UserId, tenant_id: TenantId) -> domain::Monitor {
    MonitorBuilder::new(MonitorId(Uuid::new_v4()), user_id, tenant_id)
        .name(MonitorName::new("Escalation Concurrency Target").unwrap())
        .target("http://example.com".to_string())
        .method(HttpMethod::Get)
        .expected_status(ExpectedStatus::new(200).unwrap())
        .interval(CheckInterval::new(60).unwrap())
        .timeout(Timeout::new(5).unwrap())
        .build()
}

/// Records every `notify` call with an atomic counter — used to assert
/// exactly one (or zero) notifications were sent despite many concurrent
/// sweeps/opens.
#[derive(Clone, Default)]
struct CountingNotificationService {
    calls: Arc<Mutex<usize>>,
}

impl CountingNotificationService {
    fn call_count(&self) -> usize {
        *self.calls.lock().unwrap()
    }
}

#[async_trait]
impl NotificationService for CountingNotificationService {
    async fn notify(
        &self,
        _channel: &NotificationChannel,
        _event: &NotificationEvent,
    ) -> NotifyResult {
        *self.calls.lock().unwrap() += 1;
        Ok(())
    }
}

/// Returns a fixed single-channel list for any monitor — enough for
/// these tests, which only need one enabled channel to exist.
#[derive(Clone)]
struct FixedChannelRepository {
    channel: NotificationChannel,
}

#[async_trait]
impl NotificationChannelRepository for FixedChannelRepository {
    async fn create(
        &self,
        _channel: &NotificationChannel,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    async fn delete_for_user(
        &self,
        _id: NotificationChannelId,
        _user_id: UserId,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        Ok(false)
    }

    async fn find_by_id_for_user(
        &self,
        _id: NotificationChannelId,
        _user_id: UserId,
    ) -> Result<Option<NotificationChannel>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }

    async fn list_by_monitor_for_user(
        &self,
        _monitor_id: MonitorId,
        _user_id: UserId,
    ) -> Result<Option<Vec<NotificationChannel>>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Some(vec![self.channel.clone()]))
    }

    async fn list_enabled_by_monitor(
        &self,
        _monitor_id: MonitorId,
    ) -> Result<Vec<NotificationChannel>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![self.channel.clone()])
    }
}

fn sample_channel(monitor: &domain::Monitor) -> NotificationChannel {
    NotificationChannel::create_email(CreateEmailChannel {
        id: NotificationChannelId(Uuid::new_v4()),
        monitor_id: monitor.id,
        user_id: monitor.user_id,
        tenant_id: monitor.tenant_id,
        email: Email::new("owner@example.com").unwrap(),
    })
}

/// B28 concurrency regression test: many overlapping `EscalationWorker`
/// sweeps racing to send the SAME escalation step for the SAME incident
/// must result in exactly ONE notification being sent — not one per
/// sweep. Escalation-worker analogue of B23's
/// `concurrent_failures_open_exactly_one_incident`. The fix under test is
/// `IncidentRepository::try_claim_escalation_step`'s conditional
/// `UPDATE ... WHERE escalation_step_index = $expected`, the same
/// claim-before-act shape as B23's `save_and_report` / `resolve_if_open`
/// and B25's outbox `claim_next`.
#[tokio::test]
async fn concurrent_sweeps_do_not_double_fire_escalation_step() {
    let _guard = TEST_LOCK.lock().await;

    let pool = setup_pool().await;

    let monitor_repository = PostgresMonitorRepository::new(pool.clone());
    let incident_repository = PostgresIncidentRepository::new(pool.clone());
    let escalation_policy_repository = InMemoryEscalationPolicyRepository::new();

    let user_id = UserId(Uuid::new_v4());
    let tenant_id = insert_user(&pool, user_id).await;

    let mut monitor = build_monitor(user_id, tenant_id);
    monitor.tenant_id = tenant_id;
    monitor_repository.save(&monitor).await.unwrap();

    let channel = sample_channel(&monitor);
    let channel_id = channel.id;

    // Zero-delay single-step policy: eligible for escalation immediately.
    let policy = EscalationPolicy::new(
        domain::shared::ids::EscalationPolicyId::new(),
        user_id,
        tenant_id,
        Some(monitor.id),
        vec![EscalationStep {
            delay_minutes: 0,
            channel_id,
        }],
    )
    .unwrap();
    escalation_policy_repository.create(&policy).await.unwrap();

    // Open the incident directly (bypassing IncidentManager — this test
    // is only about the escalation-claim race, not incident creation,
    // which B23 already covers).
    let mut incident =
        domain::Incident::new(domain::shared::ids::IncidentId(Uuid::new_v4()), monitor.id);
    incident.started_at = Utc::now() - chrono::Duration::minutes(5);
    incident_repository
        .save_and_report(&incident)
        .await
        .unwrap();

    let notification_service = Arc::new(CountingNotificationService::default());
    let notification_channel_repository = Arc::new(FixedChannelRepository { channel });

    let worker = EscalationWorker::new(
        Arc::new(incident_repository.clone()),
        Arc::new(monitor_repository),
        Arc::new(escalation_policy_repository),
        notification_channel_repository,
        notification_service.clone(),
    );

    let mut tasks = Vec::new();
    for _ in 0..20 {
        let worker = worker.clone();
        tasks.push(tokio::spawn(async move {
            worker.run_once().await.unwrap();
        }));
    }
    for task in tasks {
        task.await.unwrap();
    }

    assert_eq!(
        notification_service.call_count(),
        1,
        "expected exactly one escalation notification across 20 concurrent sweeps, got {}",
        notification_service.call_count()
    );

    let reloaded = incident_repository
        .find_by_id(incident.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reloaded.escalation_step_index, 1);
}

/// B28 concurrency test: many concurrent `IncidentManager::open_if_needed`
/// calls for a monitor with an ACTIVE maintenance window must never send
/// a notification, and the resulting (single, deduplicated) incident must
/// be flagged `occurred_during_maintenance`. Proves the suppression
/// wired in `IncidentManager` survives concurrency, not just a
/// single-threaded call.
#[tokio::test]
async fn concurrent_opens_during_maintenance_never_notify() {
    let _guard = TEST_LOCK.lock().await;

    let pool = setup_pool().await;

    let monitor_repository = PostgresMonitorRepository::new(pool.clone());
    let incident_repository = PostgresIncidentRepository::new(pool.clone());
    let maintenance_window_repository = InMemoryMaintenanceWindowRepository::new();

    let user_id = UserId(Uuid::new_v4());
    let tenant_id = insert_user(&pool, user_id).await;

    let mut monitor = build_monitor(user_id, tenant_id);
    monitor.tenant_id = tenant_id;
    monitor_repository.save(&monitor).await.unwrap();

    let now = Utc::now();
    let window = domain::MaintenanceWindow::new(
        domain::shared::ids::MaintenanceWindowId::new(),
        monitor.id,
        tenant_id,
        now - chrono::Duration::minutes(5),
        now + chrono::Duration::minutes(30),
        "scheduled maintenance".to_string(),
    )
    .unwrap();
    maintenance_window_repository.create(&window).await.unwrap();

    let channel = sample_channel(&monitor);
    let notification_service = Arc::new(CountingNotificationService::default());
    let notification_channel_repository = Arc::new(FixedChannelRepository { channel });
    let id_generator = Arc::new(UuidIdGenerator::new());

    let incident_manager = IncidentManager::new_with_maintenance_window(
        Arc::new(incident_repository.clone()),
        id_generator,
        notification_channel_repository,
        notification_service.clone(),
        Arc::new(maintenance_window_repository),
    );

    let mut tasks = Vec::new();
    for _ in 0..20 {
        let incident_manager = incident_manager.clone();
        let monitor = monitor.clone();
        tasks.push(tokio::spawn(async move {
            incident_manager.open_if_needed(&monitor).await.unwrap();
        }));
    }
    for task in tasks {
        task.await.unwrap();
    }

    assert_eq!(
        notification_service.call_count(),
        0,
        "maintenance window must suppress ALL notifications, even under concurrent opens"
    );

    let incidents = incident_repository
        .list_by_monitor(monitor.id)
        .await
        .unwrap();
    let open_incidents: Vec<_> = incidents.iter().filter(|i| i.is_open()).collect();
    assert_eq!(
        open_incidents.len(),
        1,
        "expected exactly one open incident, got {}",
        open_incidents.len()
    );
    assert!(
        open_incidents[0].occurred_during_maintenance,
        "incident opened during an active maintenance window must be flagged"
    );
}
