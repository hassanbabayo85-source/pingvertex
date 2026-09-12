//! GAP 2 acceptance test: `payg_retired_operations` table.
//!
//! Proves the schema the postgres repository depends on actually exists
//! and that `list_active()` filters out the retired operation.
//!
//! - `payg_retired_operations` exists and contains
//!   `proxy_check.monitoring`.
//! - `PostgresPAYGPricingRepository::list_active()` never exposes
//!   `proxy_check.monitoring`, even though the historical pricing row
//!   inserted by migration 0091/0093 is still in
//!   `payg_pricing_configs` (audit/history preserved).
//! - Every operation returned by `list_active()` is canonical according
//!   to `domain::PaygOperation::ALL`.
//! - `PaygOperation::ALL.len() == 14` — the canonical set is unchanged.

mod common;

use application::ports::PAYGPricingRepository;
use domain::PaygOperation;
use infra::PostgresPAYGPricingRepository;

const RETIRED_OPERATION: &str = "proxy_check.monitoring";

#[tokio::test]
#[ignore] // Requires a real PostgreSQL database (set DATABASE_URL)
async fn payg_retired_operations_exists_and_seeded() {
    let pool = common::setup_pool().await;

    // (a) Table exists.
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM information_schema.tables
            WHERE table_schema = current_schema()
              AND table_name = 'payg_retired_operations'
         )",
    )
    .fetch_one(&pool)
    .await
    .expect("schema lookup failed");
    assert!(exists, "payg_retired_operations table must exist");

    // (b) Migration seeded proxy_check.monitoring exactly once.
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM payg_retired_operations WHERE operation = $1")
            .bind(RETIRED_OPERATION)
            .fetch_one(&pool)
            .await
            .expect("count failed");
    assert_eq!(
        count, 1,
        "exactly one retirement row for {RETIRED_OPERATION}"
    );

    // (c) The historical pricing row for the retired operation is still
    //     present (audit/history preserved) — migration must not delete it.
    let price_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM payg_pricing_configs WHERE operation = $1")
            .bind(RETIRED_OPERATION)
            .fetch_one(&pool)
            .await
            .expect("pricing count failed");
    assert!(
        price_rows >= 1,
        "historical pricing rows for {RETIRED_OPERATION} must remain queryable"
    );
}

#[tokio::test]
#[ignore] // Requires a real PostgreSQL database (set DATABASE_URL)
async fn list_active_excludes_retired_operation() {
    let pool = common::setup_pool().await;
    let repo = PostgresPAYGPricingRepository::new(pool.clone());

    let active = repo.list_active().await.expect("list_active failed");

    // (a) Retired operation must never appear.
    let retired_seen = active.iter().any(|p| p.operation == RETIRED_OPERATION);
    assert!(
        !retired_seen,
        "list_active() must not expose retired operation {RETIRED_OPERATION}"
    );

    // (b) Every active operation must be canonical.
    for price in &active {
        assert!(
            PaygOperation::is_canonical(&price.operation),
            "list_active() exposed non-canonical operation {:?}",
            price.operation
        );
    }

    // (c) No operation may appear more than once, even though the
    //     historical seed migrations (0079, 0087) left behind multiple
    //     active versions for the same operation. `DISTINCT ON (operation)`
    //     in the query collapses them.
    let mut seen = std::collections::HashSet::new();
    for price in &active {
        assert!(
            seen.insert(price.operation.clone()),
            "list_active() returned duplicate operation {:?}",
            price.operation
        );
    }

    // (d) The exposed set must be exactly the canonical set — one row per
    //     operation, and every PaygOperation variant present.
    let exposed: std::collections::HashSet<&str> =
        active.iter().map(|p| p.operation.as_str()).collect();
    let canonical: std::collections::HashSet<&str> =
        PaygOperation::ALL.iter().map(|op| op.as_str()).collect();
    assert_eq!(
        exposed, canonical,
        "list_active() must expose exactly the canonical operation set"
    );
}

#[test]
fn canonical_operation_set_is_fourteen() {
    assert_eq!(
        PaygOperation::ALL.len(),
        14,
        "canonical operation set must remain 14 entries"
    );
    assert!(
        !PaygOperation::is_canonical("proxy_check.monitoring"),
        "proxy_check.monitoring must not be canonical"
    );
    assert!(
        PaygOperation::is_canonical("proxy_check.one_time"),
        "proxy_check.one_time must remain canonical (one-shot only)"
    );
}
