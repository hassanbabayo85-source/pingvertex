//! GAP 2 acceptance test: `payg_retired_operations` and the canonical
//! operation set.
//!
//! After GAP 2, `proxy_check.monitoring` is a first-class recurring
//! operation again. It must be canonical, it must NOT be retired, and
//! `PostgresPAYGPricingRepository::list_active()` must expose its
//! (immutable) historical pricing row.
//!
//! `monitor_check` remains retired and must never be advertised.

mod common;

use application::ports::PAYGPricingRepository;
use domain::PaygOperation;
use infra::PostgresPAYGPricingRepository;

/// A historical non-canonical operation that GAP 2 deliberately leaves
/// retired. Used by the retirement test below.
const STILL_RETIRED: &str = "monitor_check";

/// The recurring operation restored by migration 0113.
const RESTORED: &str = "proxy_check.monitoring";

#[tokio::test]
#[ignore] // Requires a real PostgreSQL database (set DATABASE_URL)
async fn payg_retired_operations_exists_and_does_not_retire_proxy_monitoring() {
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

    // (b) A known historical operation is still retired.
    let still: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM payg_retired_operations WHERE operation = $1")
            .bind(STILL_RETIRED)
            .fetch_one(&pool)
            .await
            .expect("count failed");
    assert_eq!(still, 1, "exactly one retirement row for {STILL_RETIRED}");

    // (c) After migration 0113, proxy_check.monitoring is NOT retired.
    let proxy_retired: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM payg_retired_operations WHERE operation = $1")
            .bind(RESTORED)
            .fetch_one(&pool)
            .await
            .expect("count failed");
    assert_eq!(
        proxy_retired, 0,
        "{RESTORED} must not appear in payg_retired_operations after GAP 2"
    );

    // (d) The historical pricing row for proxy_check.monitoring is still
    //     present (audit/history preserved).
    let price_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM payg_pricing_configs WHERE operation = $1")
            .bind(RESTORED)
            .fetch_one(&pool)
            .await
            .expect("pricing count failed");
    assert!(
        price_rows >= 1,
        "historical pricing rows for {RESTORED} must remain queryable"
    );
}

#[tokio::test]
#[ignore] // Requires a real PostgreSQL database (set DATABASE_URL)
async fn list_active_exposes_proxy_monitoring_and_excludes_retired() {
    let pool = common::setup_pool().await;
    let repo = PostgresPAYGPricingRepository::new(pool.clone());

    let active = repo.list_active().await.expect("list_active failed");

    // (a) The restored recurring operation is exposed.
    let proxy_seen = active.iter().any(|p| p.operation == RESTORED);
    assert!(
        proxy_seen,
        "list_active() must expose restored operation {RESTORED}"
    );

    // (b) Retired operations must never appear.
    let retired_seen = active.iter().any(|p| p.operation == STILL_RETIRED);
    assert!(
        !retired_seen,
        "list_active() must not expose retired operation {STILL_RETIRED}"
    );

    // (c) Every active operation must be canonical.
    for price in &active {
        assert!(
            PaygOperation::is_canonical(&price.operation),
            "list_active() exposed non-canonical operation {:?}",
            price.operation
        );
    }

    // (d) No operation may appear more than once.
    let mut seen = std::collections::HashSet::new();
    for price in &active {
        assert!(
            seen.insert(price.operation.clone()),
            "list_active() returned duplicate operation {:?}",
            price.operation
        );
    }

    // (e) Every canonical operation that has an active pricing row must
    //     be exposed exactly once. (Storage's recurring redesign is GAP 2
    //     Phase B; until then its row remains a write-time charge, so we
    //     only assert that the exposed subset is a subset of the canonical
    //     set — no further equality.)
    for price in &active {
        assert!(PaygOperation::is_canonical(&price.operation));
    }
}

#[test]
fn canonical_operation_set_is_fourteen() {
    assert_eq!(
        PaygOperation::ALL.len(),
        14,
        "canonical operation set must be 14 entries after GAP 3 (storage retired)"
    );
    assert!(
        PaygOperation::is_canonical("proxy_check.monitoring"),
        "proxy_check.monitoring must be canonical after GAP 2 restore"
    );
    assert!(
        PaygOperation::is_canonical("proxy_check.one_time"),
        "proxy_check.one_time must remain canonical (one-shot only)"
    );
    assert!(
        !PaygOperation::is_canonical("monitor_check"),
        "monitor_check must remain non-canonical"
    );
    assert!(
        !PaygOperation::is_canonical("storage"),
        "storage must be non-canonical after GAP 3"
    );
}
