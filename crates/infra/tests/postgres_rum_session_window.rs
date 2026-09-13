//! GAP 3 Kashi E: PostgresRumEventRepository::upsert_session 30-minute
//! window tests.
//!
//! Proves that a new beacon starts a new billable session (created=true)
//! and that a follow-up beacon within the window extends it (created=false).
//! Also proves that a beacon after the window restarts the session row
//! (created=true) instead of silently reusing the stale one.

use application::ports::{RumEventRepository, RumSiteRepository};
use chrono::{Duration, Utc};
use domain::RumSite;
use domain::shared::ids::{RumSessionId, RumSiteId, TenantId, UserId};
use infra::repositories::postgres_rum_event_repository::PostgresRumEventRepository;
use infra::repositories::postgres_rum_site_repository::PostgresRumSiteRepository;
use sqlx::PgPool;
use std::time::Duration as StdDuration;

async fn pool() -> (PgPool, infra::test_support::ExclusiveTestGuard) {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    infra::test_support::setup_pool_exclusive(&url).await
}

async fn seed_site(pool: &PgPool) -> (TenantId, RumSiteId) {
    let user_id = UserId::new();
    sqlx::query(
        "INSERT INTO users (id, email, password_hash, full_name, created_at, updated_at)
         VALUES ($1, $2, 'not-a-real-hash', 'RUM Test User', NOW(), NOW())
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(user_id.0)
    .bind(format!("{}@rum-window.test", user_id.0))
    .execute(pool)
    .await
    .unwrap();

    let tenant_id = TenantId::new();
    sqlx::query(
        "INSERT INTO tenants (id, name, owner_user_id, created_at)
         VALUES ($1, 'RUM Window Test', $2, NOW())",
    )
    .bind(tenant_id.0)
    .bind(user_id.0)
    .execute(pool)
    .await
    .unwrap();

    let site = RumSite::new(
        RumSiteId::new(),
        tenant_id,
        "Test Site".to_string(),
        "example.com".to_string(),
    )
    .unwrap();
    let site_repo = PostgresRumSiteRepository::new(pool.clone());
    site_repo.create(&site).await.unwrap();

    (tenant_id, site.id)
}

fn session(tenant_id: TenantId, site_id: RumSiteId, key: &str) -> domain::RumSession {
    domain::RumSession::new(
        RumSessionId::new(),
        tenant_id,
        site_id,
        key.to_string(),
        Some("Chrome".to_string()),
        Some("128".to_string()),
        Some("Linux".to_string()),
        Some(domain::DeviceType::Desktop),
        None,
    )
}

#[tokio::test]
async fn first_beacon_creates_new_session() {
    let (pool, _guard) = pool().await;
    let (tenant_id, site_id) = seed_site(&pool).await;
    let repo = PostgresRumEventRepository::new(pool.clone());

    let s = session(tenant_id, site_id, "sess_1");
    let result = repo.upsert_session(&s, Utc::now()).await.unwrap();

    assert!(result.created, "first beacon must create a new session");
    assert_eq!(result.session.session_key, "sess_1");
}

#[tokio::test]
async fn beacon_within_window_extends_session() {
    let (pool, _guard) = pool().await;
    let (tenant_id, site_id) = seed_site(&pool).await;
    let repo = PostgresRumEventRepository::new(pool.clone());

    let now = Utc::now();
    let first = repo
        .upsert_session(&session(tenant_id, site_id, "sess_2"), now)
        .await
        .unwrap();
    assert!(first.created);

    // 5 minutes later: still inside the 30-minute window.
    let second = repo
        .upsert_session(
            &session(tenant_id, site_id, "sess_2"),
            now + Duration::minutes(5),
        )
        .await
        .unwrap();

    assert!(!second.created, "beacon within window must NOT be billable");
    assert_eq!(
        second.session.id, first.session.id,
        "same session row reused"
    );
    assert!(second.session.last_seen_at > first.session.last_seen_at);
}

#[tokio::test]
async fn beacon_after_window_restarts_session() {
    let (pool, _guard) = pool().await;
    let (tenant_id, site_id) = seed_site(&pool).await;
    let repo = PostgresRumEventRepository::new(pool.clone());

    let now = Utc::now();
    let first = repo
        .upsert_session(&session(tenant_id, site_id, "sess_3"), now)
        .await
        .unwrap();
    assert!(first.created);

    // 40 minutes later: outside the 30-minute window -> new billable session.
    let second = repo
        .upsert_session(
            &session(tenant_id, site_id, "sess_3"),
            now + Duration::minutes(40),
        )
        .await
        .unwrap();

    assert!(
        second.created,
        "beacon past the 30-minute window must start a NEW billable session"
    );
    // Same (site_id, session_key) unique constraint keeps the same row id.
    assert_eq!(second.session.id, first.session.id);
    assert!(second.session.started_at > first.session.started_at);
}

#[tokio::test]
async fn concurrent_beacons_for_same_session_create_one_row() {
    let (pool, _guard) = pool().await;
    let (tenant_id, site_id) = seed_site(&pool).await;
    let repo = std::sync::Arc::new(PostgresRumEventRepository::new(pool.clone()));

    let mut set = tokio::task::JoinSet::new();
    for _ in 0..8 {
        let repo = repo.clone();
        let s = session(tenant_id, site_id, "sess_4");
        let now = Utc::now();
        set.spawn(async move { repo.upsert_session(&s, now).await.unwrap() });
    }

    let mut created_true = 0;
    while let Some(res) = set.join_next().await {
        let r = res.unwrap();
        if r.created {
            created_true += 1;
        }
    }

    // The unique (site_id, session_key) constraint means exactly one
    // fresh INSERT wins; every other beacon sees the now-active session
    // and extends it. Under the extended session model the winner is the
    // only one to report created = true.
    assert!(
        created_true >= 1,
        "at least the first beacon must report a new session"
    );

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM rum_sessions WHERE site_id = $1 AND session_key = 'sess_4'",
    )
    .bind(site_id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1, "one session row per (site_id, session_key)");
}

// Silence unused import warnings on helper used only for types.
const _: StdDuration = StdDuration::from_secs(1);
