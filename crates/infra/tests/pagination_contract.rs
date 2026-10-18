//! Pagination contract regression tests.
//!
//! These tests prove the invariant:
//!
//!   No authenticated tenant can force the API to load an unbounded
//!   amount of database data in one request.
//!
//! Specifically they prove:
//!
//!   1. A request for an oversized `limit` is clamped to MAX_PAGE_SIZE.
//!   2. A page never returns more rows than the requested limit.
//!   3. When more rows exist, `next_cursor` is `Some`; on the last page
//!      it is `None`.
//!   4. Walking pages with the returned cursor visits every row exactly
//!      once, with no duplicates and no skips.
//!   5. Tenant isolation: a page for tenant A never returns rows
//!      belonging to tenant B, even when both tenants have rows.
//!   6. Deterministic ordering under ties: rows created in the same
//!      instant are sorted by id DESC.
//!
//! Every test uses a fresh tenant, so they never race each other or
//! existing tests.
//!
//! Graceful skip: if DATABASE_URL is unset or Postgres unreachable,
//! each test returns Ok without asserting anything.

use std::collections::HashSet;

use application::pagination::{Cursor, MAX_PAGE_SIZE, PageRequest};
use application::ports::ApiKeyRepository;
use chrono::Utc;
use domain::entities::ApiKey;
use domain::enums::ApiKeyScope;
use domain::shared::ids::{ApiKeyId, TenantId, UserId};
use infra::PostgresApiKeyRepository;
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn load_env_test() {
    for candidate in &["../../.env.test", ".env.test", "../.env.test"] {
        if std::path::Path::new(candidate).exists() {
            dotenvy::from_filename(candidate).ok();
            return;
        }
    }
}

async fn pool_or_skip() -> Option<(PgPool, infra::test_support::ExclusiveTestGuard)> {
    load_env_test();
    let url = std::env::var("DATABASE_URL").ok()?;
    Some(infra::test_support::setup_pool_exclusive(&url).await)
}

/// Creates a fresh user + tenant pair, returning the tenant id.
async fn fresh_tenant(pool: &PgPool) -> TenantId {
    let user_id = UserId::new();
    sqlx::query(
        "INSERT INTO users (id, email, password_hash, full_name, created_at, updated_at)
         VALUES ($1, $2, 'not-a-real-hash', 'Pagination Test', NOW(), NOW())
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(user_id.0)
    .bind(format!("{}@pagination.test", user_id.0))
    .execute(pool)
    .await
    .expect("insert test user");

    let tenant_id = TenantId::new();
    sqlx::query(
        "INSERT INTO tenants (id, name, owner_user_id, account_type, status, created_at)
         VALUES ($1, $2, $3, 'individual', 'active', NOW())
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(tenant_id.0)
    .bind(format!("pagination-test-{}", tenant_id.0))
    .bind(user_id.0)
    .execute(pool)
    .await
    .expect("insert test tenant");

    tenant_id
}

/// Inserts `n` api_keys for `tenant_id`, with sequentially older
/// `created_at` values so the ordering is deterministic.
async fn seed_api_keys(pool: &PgPool, tenant_id: TenantId, n: usize) -> Vec<ApiKeyId> {
    let user_id = UserId::new();
    sqlx::query(
        "INSERT INTO users (id, email, password_hash, full_name, created_at, updated_at)
         VALUES ($1, $2, 'not-a-real-hash', 'API Key Owner', NOW(), NOW())
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(user_id.0)
    .bind(format!("{}@pagination-key.test", user_id.0))
    .execute(pool)
    .await
    .expect("insert api key owner");

    let mut ids = Vec::with_capacity(n);
    let base = Utc::now();
    for i in 0..n {
        let id = ApiKeyId::new();
        let created_at = base - chrono::Duration::seconds(i as i64);
        sqlx::query(
            "INSERT INTO api_keys (id, tenant_id, user_id, name, key_prefix, hashed_key, scopes, created_at, expires_at, last_used_at)
             VALUES ($1, $2, $3, $4, $5, 'hashed', ARRAY['monitor:read']::text[], $6, NULL, NULL)",
        )
        .bind(id.0)
        .bind(tenant_id.0)
        .bind(user_id.0)
        .bind(format!("key-{i:03}"))
        .bind(format!("pv_{}", &id.0.simple().to_string()[..12]))
        .bind(created_at)
        .execute(pool)
        .await
        .expect("insert api key");
        ids.push(id);
    }
    ids
}

fn assert_no_overflow(page_len: usize, requested_limit: u32, what: &str) {
    assert!(
        page_len <= requested_limit as usize,
        "SECURITY: {what} returned {page_len} rows for limit={requested_limit}"
    );
}

// ===========================================================================
// 1) Oversized limit is clamped
// ===========================================================================

#[tokio::test]
async fn oversized_limit_is_clamped_to_max_page_size() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };
    let tenant = fresh_tenant(&pool).await;
    seed_api_keys(&pool, tenant, 200).await;

    // Client asks for a million. `PageRequest::new` clamps to MAX_PAGE_SIZE.
    let request = PageRequest::new(Some(1_000_000), None);
    assert_eq!(request.limit, MAX_PAGE_SIZE);
    assert_eq!(request.fetch_size() as u32, MAX_PAGE_SIZE + 1);

    let repo = PostgresApiKeyRepository::new(pool);
    let page = repo
        .list_by_tenant_paginated(tenant, request)
        .await
        .unwrap();

    assert_no_overflow(page.items.len(), MAX_PAGE_SIZE, "oversized page");
    assert!(
        page.next_cursor.is_some(),
        "with 200 rows and a max page of {MAX_PAGE_SIZE} there must be a next page"
    );
}

// ===========================================================================
// 2) Page size is bounded by the requested limit
// ===========================================================================

#[tokio::test]
async fn page_never_exceeds_requested_limit() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };
    let tenant = fresh_tenant(&pool).await;
    seed_api_keys(&pool, tenant, 25).await;

    let repo = PostgresApiKeyRepository::new(pool);
    for limit in [1u32, 5, 10, 25] {
        let request = PageRequest::new(Some(limit), None);
        let page = repo
            .list_by_tenant_paginated(tenant, request)
            .await
            .unwrap();
        assert_no_overflow(page.items.len(), limit, "bounded page");
    }
}

// ===========================================================================
// 3) Walking pages visits every row exactly once, no duplicates, no skips
// ===========================================================================

#[tokio::test]
async fn walking_pages_visits_every_row_exactly_once() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };
    let tenant = fresh_tenant(&pool).await;
    let inserted_ids = seed_api_keys(&pool, tenant, 37).await;
    let inserted_set: HashSet<Uuid> = inserted_ids.iter().map(|id| id.0).collect();

    let repo = PostgresApiKeyRepository::new(pool);

    let mut seen = HashSet::new();
    let mut page_request = PageRequest::new(Some(10), None);
    let mut iterations = 0;

    loop {
        iterations += 1;
        assert!(iterations <= 100, "pagination loop did not terminate");

        let page = repo
            .list_by_tenant_paginated(tenant, page_request)
            .await
            .unwrap();
        assert_no_overflow(page.items.len(), page_request.limit, "walked page");

        for item in &page.items {
            let id = item.id.0;
            assert!(
                seen.insert(id),
                "duplicate row {id} returned across pages (cursor not stable)"
            );
        }

        match page.next_cursor {
            Some(cursor_str) => {
                let cursor = Cursor::decode(&cursor_str).expect("cursor must decode");
                page_request = PageRequest {
                    limit: page_request.limit,
                    cursor: Some(cursor),
                };
            }
            None => break,
        }
    }

    assert_eq!(
        seen, inserted_set,
        "walking all pages must yield exactly the inserted rows"
    );
}

// ===========================================================================
// 4) Tenant isolation
// ===========================================================================

#[tokio::test]
async fn paginated_list_is_tenant_scoped() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };
    let tenant_a = fresh_tenant(&pool).await;
    let tenant_b = fresh_tenant(&pool).await;

    let a_ids: HashSet<Uuid> = seed_api_keys(&pool, tenant_a, 5)
        .await
        .into_iter()
        .map(|id| id.0)
        .collect();
    let b_ids: HashSet<Uuid> = seed_api_keys(&pool, tenant_b, 7)
        .await
        .into_iter()
        .map(|id| id.0)
        .collect();

    let repo = PostgresApiKeyRepository::new(pool);
    let request = PageRequest::new(Some(100), None);

    let page_a = repo
        .list_by_tenant_paginated(tenant_a, request)
        .await
        .unwrap();
    let page_b = repo
        .list_by_tenant_paginated(tenant_b, request)
        .await
        .unwrap();

    let a_returned: HashSet<Uuid> = page_a.items.iter().map(|k| k.id.0).collect();
    let b_returned: HashSet<Uuid> = page_b.items.iter().map(|k| k.id.0).collect();

    assert_eq!(a_returned, a_ids, "tenant A must see only its own rows");
    assert_eq!(b_returned, b_ids, "tenant B must see only its own rows");
    assert!(
        a_returned.is_disjoint(&b_returned),
        "tenant A and B results must never overlap"
    );
}

// ===========================================================================
// 5) Deterministic ordering under ties (same created_at)
// ===========================================================================

#[tokio::test]
async fn ordering_is_deterministic_under_created_at_ties() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };
    let tenant = fresh_tenant(&pool).await;

    // Insert 5 keys with the EXACT same created_at. The cursor must still
    // be stable because of the id DESC tiebreaker.
    let user_id = UserId::new();
    sqlx::query(
        "INSERT INTO users (id, email, password_hash, full_name, created_at, updated_at)
         VALUES ($1, $2, 'x', 'Tie', NOW(), NOW()) ON CONFLICT (id) DO NOTHING",
    )
    .bind(user_id.0)
    .bind(format!("{}@tie.test", user_id.0))
    .execute(&pool)
    .await
    .unwrap();

    let same_instant = Utc::now();
    for i in 0..5 {
        sqlx::query(
            "INSERT INTO api_keys (id, tenant_id, user_id, name, key_prefix, hashed_key, scopes, created_at, expires_at, last_used_at)
             VALUES ($1, $2, $3, $4, $5, 'h', ARRAY['monitor:read']::text[], $6, NULL, NULL)",
        )
        .bind(ApiKeyId::new().0)
        .bind(tenant.0)
        .bind(user_id.0)
        .bind(format!("tie-{i}"))
        .bind(format!("pv_tie_{i:08}"))
        .bind(same_instant)
        .execute(&pool)
        .await
        .unwrap();
    }

    let repo = PostgresApiKeyRepository::new(pool);

    // Fetch all 5 in one page; id DESC ordering means they come back in
    // a stable, deterministic order.
    let request = PageRequest::new(Some(5), None);
    let page = repo
        .list_by_tenant_paginated(tenant, request)
        .await
        .unwrap();
    assert_eq!(page.items.len(), 5);

    let ids_in_order: Vec<Uuid> = page.items.iter().map(|k| k.id.0).collect();
    let mut sorted_desc = ids_in_order.clone();
    sorted_desc.sort_by(|a, b| b.cmp(a));
    assert_eq!(
        ids_in_order, sorted_desc,
        "rows tied on created_at must come back sorted by id DESC"
    );
}

// ===========================================================================
// 6) Malformed cursor is ignored, not rejected
// ===========================================================================

#[tokio::test]
async fn malformed_cursor_is_ignored_and_starts_from_first_page() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };
    let tenant = fresh_tenant(&pool).await;
    seed_api_keys(&pool, tenant, 3).await;

    // `PageRequest::new` treats a malformed cursor as `None`, so this is
    // equivalent to page 1. No distinguishable error is returned — an
    // attacker cannot probe the cursor encoding.
    let request = PageRequest::new(Some(10), Some("not-a-real-cursor"));
    assert!(request.cursor.is_none());

    let repo = PostgresApiKeyRepository::new(pool);
    let page = repo
        .list_by_tenant_paginated(tenant, request)
        .await
        .unwrap();
    assert_eq!(page.items.len(), 3);
    assert!(page.next_cursor.is_none());
}

// ===========================================================================
// 7) The `ApiKey` entity type is still usable via the returned struct
// ===========================================================================

#[tokio::test]
async fn paginated_api_key_is_the_full_domain_entity() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };
    let tenant = fresh_tenant(&pool).await;
    seed_api_keys(&pool, tenant, 1).await;

    let repo = PostgresApiKeyRepository::new(pool);
    let page = repo
        .list_by_tenant_paginated(tenant, PageRequest::first())
        .await
        .unwrap();

    let key: &ApiKey = page.items.first().expect("one row");
    assert_eq!(key.tenant_id, tenant);
    assert!(key.scopes.contains(&ApiKeyScope::MonitorRead));
}
