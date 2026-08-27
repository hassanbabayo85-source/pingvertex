use std::env;

use domain::shared::ids::{CheckId, IncidentId, MonitorId, TenantId, UserId};
use domain::{
    Check, CheckInterval, CheckStatus, ErrorType, ExpectedStatus, HttpMethod, Incident, Monitor,
    MonitorBuilder, MonitorName, RegionCode, Timeout, Url,
};
use infra::create_pool;
use sqlx::{PgPool, query};
use uuid::Uuid;

#[allow(dead_code)]
pub async fn setup_pool() -> PgPool {
    let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/uptime_monitor".to_string()
    });

    let pool = create_pool(&database_url).await.unwrap();

    println!("CARGO_MANIFEST_DIR = {}", env!("CARGO_MANIFEST_DIR"));

    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();

    clear_tables(&pool).await;

    pool
}

#[allow(dead_code)]
async fn reset_database(pool: &PgPool) {
    query("DROP SCHEMA IF EXISTS public CASCADE")
        .execute(pool)
        .await
        .unwrap();

    query("CREATE SCHEMA IF NOT EXISTS public")
        .execute(pool)
        .await
        .unwrap();
}

#[allow(dead_code)]
pub async fn clear_tables(pool: &PgPool) {
    query(
        "TRUNCATE TABLE audit_log, tenant_memberships, tenants, status_pages, notification_channels, incidents, checks, monitors, users RESTART IDENTITY CASCADE",
    )
    .execute(pool)
    .await
    .unwrap();
}

pub async fn sample_monitor(pool: &PgPool, name: &str, url: &str) -> Monitor {
    let mut monitor = MonitorBuilder::new(
        MonitorId(Uuid::new_v4()),
        UserId(Uuid::new_v4()),
        TenantId::new(),
    )
    .name(MonitorName::new(name).unwrap())
    .url(Url::new(url).unwrap())
    .method(HttpMethod::Get)
    .expected_status(ExpectedStatus::new(200).unwrap())
    .interval(CheckInterval::new(60).unwrap())
    .timeout(Timeout::new(10).unwrap())
    .build();

    let tenant_id = insert_user(pool, monitor.user_id).await;
    monitor.tenant_id = tenant_id;
    monitor
}

/// Inserts a minimal, valid row into `users` for `user_id`.
///
/// Needed since `migrations/0010_create_notification_channels.sql` added
/// `notification_channels.user_id REFERENCES users(id) ON DELETE CASCADE`
/// — unlike `monitors.user_id`, which has no such FK — so any test that
/// creates a notification channel must first make sure the owning user
/// actually exists in `users`, or the insert fails with a foreign-key
/// violation.
///
/// `email` is derived from `user_id` so it's unique per call even when a
/// test creates more than one user (e.g. an "owner" and an "attacker").
#[allow(dead_code)]
pub async fn insert_tenant_for_user(pool: &PgPool, user_id: UserId) -> TenantId {
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

#[allow(dead_code)]
pub async fn insert_user(pool: &PgPool, user_id: UserId) -> TenantId {
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

    insert_tenant_for_user(pool, user_id).await
}

/// Convenience: builds a monitor AND inserts the matching `users` row for
/// its `user_id`, so callers don't have to remember to call `insert_user`
/// separately every time.
#[allow(dead_code)]
pub async fn sample_monitor_with_user(pool: &PgPool, name: &str, url: &str) -> Monitor {
    sample_monitor(pool, name, url).await
}

#[allow(dead_code)]
pub fn sample_check(monitor_id: MonitorId) -> Check {
    Check::new(
        CheckId(Uuid::new_v4()),
        monitor_id,
        RegionCode::default_region(),
        CheckStatus::Up,
        Some(200),
        123,
        ErrorType::None,
        None,
    )
}

#[allow(dead_code)]
pub fn sample_incident(monitor_id: MonitorId) -> Incident {
    Incident::new(IncidentId(Uuid::new_v4()), monitor_id)
}

#[allow(dead_code)]
pub fn sample_status_page(
    pool_user_id: UserId,
    name: &str,
    subdomain: &str,
) -> domain::entities::StatusPage {
    domain::entities::StatusPage::create(domain::entities::CreateStatusPage {
        id: domain::shared::ids::StatusPageId::new(),
        user_id: pool_user_id,
        tenant_id: TenantId::new(),
        name: name.to_string(),
        subdomain: domain::value_objects::Subdomain::new(subdomain).unwrap(),
        monitor_ids: vec![],
    })
    .unwrap()
}

/// Convenience: builds a status page AND inserts the matching `users` row
/// for its `user_id`, mirroring `sample_monitor_with_user` above — needed
/// since `status_pages.user_id REFERENCES users(id) ON DELETE CASCADE`.
#[allow(dead_code)]
pub async fn sample_status_page_with_user(
    pool: &PgPool,
    name: &str,
    subdomain: &str,
) -> domain::entities::StatusPage {
    let user_id = UserId(Uuid::new_v4());
    insert_user(pool, user_id).await;
    let tenant_id = insert_tenant_for_user(pool, user_id).await;
    let mut page = sample_status_page(user_id, name, subdomain);
    page.tenant_id = tenant_id;
    page
}
