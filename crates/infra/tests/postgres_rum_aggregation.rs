//! GAP 3 Kashi E: PostgresRumRetentionRepository aggregation tests.
//!
//! Proves the aggregate-before-delete contract:
//!   1. Raw rows older than 7 days are rolled into rum_daily_aggregates.
//!   2. The raw rows are then deleted.
//!   3. Running the retention twice is idempotent.
//!   4. Sites are isolated — running site A does not touch site B.

use application::ports::{RumRetentionRepository, RumSiteRepository};
use chrono::{Duration, Utc};
use domain::RumSite;
use domain::shared::ids::{RumSessionId, RumSiteId, TenantId, UserId};
use infra::repositories::postgres_rum_retention_repository::PostgresRumRetentionRepository;
use infra::repositories::postgres_rum_site_repository::PostgresRumSiteRepository;
use sqlx::PgPool;
use uuid::Uuid;

async fn pool() -> (PgPool, infra::test_support::ExclusiveTestGuard) {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    infra::test_support::setup_pool_exclusive(&url).await
}

async fn seed_site(pool: &PgPool, name: &str) -> (TenantId, RumSiteId) {
    let user_id = UserId::new();
    sqlx::query(
        "INSERT INTO users (id, email, password_hash, full_name, created_at, updated_at)
         VALUES ($1, $2, 'not-a-real-hash', 'RUM Agg Test', NOW(), NOW())
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(user_id.0)
    .bind(format!("{}@rum-agg.test", user_id.0))
    .execute(pool)
    .await
    .unwrap();

    let tenant_id = TenantId::new();
    sqlx::query(
        "INSERT INTO tenants (id, name, owner_user_id, created_at)
         VALUES ($1, $2, $3, NOW())",
    )
    .bind(tenant_id.0)
    .bind(format!("RUM Agg {name}"))
    .bind(user_id.0)
    .execute(pool)
    .await
    .unwrap();

    let site = RumSite::new(
        RumSiteId::new(),
        tenant_id,
        name.to_string(),
        format!("{name}.example.com"),
    )
    .unwrap();
    PostgresRumSiteRepository::new(pool.clone())
        .create(&site)
        .await
        .unwrap();

    (tenant_id, site.id)
}

async fn insert_old_session(
    pool: &PgPool,
    tenant_id: TenantId,
    site_id: RumSiteId,
    key: &str,
    when: chrono::DateTime<Utc>,
) -> RumSessionId {
    let id = RumSessionId::new();
    sqlx::query(
        "INSERT INTO rum_sessions
             (id, tenant_id, site_id, session_key, started_at, last_seen_at)
         VALUES ($1, $2, $3, $4, $5, $5)",
    )
    .bind(id.0)
    .bind(tenant_id.0)
    .bind(site_id.0)
    .bind(key)
    .bind(when)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn insert_old_page_view(
    pool: &PgPool,
    tenant_id: TenantId,
    site_id: RumSiteId,
    session_id: RumSessionId,
    path: &str,
    load_time_ms: i32,
    when: chrono::DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO rum_page_views
             (id, tenant_id, site_id, session_id, path, load_time_ms, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id.0)
    .bind(site_id.0)
    .bind(session_id.0)
    .bind(path)
    .bind(load_time_ms)
    .bind(when)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_old_js_error(
    pool: &PgPool,
    tenant_id: TenantId,
    site_id: RumSiteId,
    session_id: RumSessionId,
    when: chrono::DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO rum_js_errors
             (id, tenant_id, site_id, session_id, message, created_at)
         VALUES ($1, $2, $3, $4, 'boom', $5)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id.0)
    .bind(site_id.0)
    .bind(session_id.0)
    .bind(when)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn aggregates_before_deleting() {
    let (pool, _guard) = pool().await;
    let (tenant_id, site_id) = seed_site(&pool, "agg1").await;
    let repo = PostgresRumRetentionRepository::new(pool.clone());

    let when = Utc::now() - Duration::days(10);
    let sess = insert_old_session(&pool, tenant_id, site_id, "old1", when).await;
    insert_old_page_view(&pool, tenant_id, site_id, sess, "/home", 100, when).await;
    insert_old_page_view(&pool, tenant_id, site_id, sess, "/about", 200, when).await;
    insert_old_js_error(&pool, tenant_id, site_id, sess, when).await;

    let outcome = repo
        .run_retention_for_site(tenant_id, site_id, 7, 100)
        .await
        .unwrap();

    assert!(outcome.days_aggregated >= 1);
    assert_eq!(outcome.page_views_deleted, 2);
    assert_eq!(outcome.js_errors_deleted, 1);
    assert_eq!(outcome.sessions_deleted, 1);

    // Aggregate row exists with the right counts.
    let (pv, js, s): (i64, i64, i64) = sqlx::query_as(
        "SELECT page_view_count, js_error_count, total_sessions
         FROM rum_daily_aggregates
         WHERE site_id = $1 AND day = $2",
    )
    .bind(site_id.0)
    .bind(when.date_naive())
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(pv, 2);
    assert_eq!(js, 1);
    assert_eq!(s, 1);

    // Raw rows gone.
    let pv_left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rum_page_views WHERE site_id = $1")
        .bind(site_id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(pv_left, 0);
}

#[tokio::test]
async fn retention_is_idempotent() {
    let (pool, _guard) = pool().await;
    let (tenant_id, site_id) = seed_site(&pool, "agg2").await;
    let repo = PostgresRumRetentionRepository::new(pool.clone());

    let when = Utc::now() - Duration::days(10);
    let sess = insert_old_session(&pool, tenant_id, site_id, "old2", when).await;
    insert_old_page_view(&pool, tenant_id, site_id, sess, "/x", 50, when).await;

    let _ = repo
        .run_retention_for_site(tenant_id, site_id, 7, 100)
        .await
        .unwrap();
    let second = repo
        .run_retention_for_site(tenant_id, site_id, 7, 100)
        .await
        .unwrap();

    assert_eq!(second.page_views_deleted, 0, "no rows left to delete");
    assert_eq!(second.sessions_deleted, 0);

    // Counts did not double.
    let pv: i64 = sqlx::query_scalar(
        "SELECT page_view_count FROM rum_daily_aggregates WHERE site_id = $1 AND day = $2",
    )
    .bind(site_id.0)
    .bind(when.date_naive())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(pv, 1);
}

#[tokio::test]
async fn retention_does_not_touch_other_sites() {
    let (pool, _guard) = pool().await;
    let (tenant_a, site_a) = seed_site(&pool, "agga").await;
    let (tenant_b, site_b) = seed_site(&pool, "aggb").await;
    let repo = PostgresRumRetentionRepository::new(pool.clone());

    let when = Utc::now() - Duration::days(10);
    let sess_a = insert_old_session(&pool, tenant_a, site_a, "a", when).await;
    insert_old_page_view(&pool, tenant_a, site_a, sess_a, "/a", 10, when).await;
    let sess_b = insert_old_session(&pool, tenant_b, site_b, "b", when).await;
    insert_old_page_view(&pool, tenant_b, site_b, sess_b, "/b", 20, when).await;

    repo.run_retention_for_site(tenant_a, site_a, 7, 100)
        .await
        .unwrap();

    // Site A is empty; site B is untouched.
    let a_left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rum_page_views WHERE site_id = $1")
        .bind(site_a.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    let b_left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rum_page_views WHERE site_id = $1")
        .bind(site_b.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(a_left, 0);
    assert_eq!(b_left, 1);
}
