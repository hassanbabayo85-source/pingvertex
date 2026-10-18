mod common;

use application::ports::check_repository::{CheckFilters, CheckRepository};
use application::ports::monitor_repository::MonitorRepository;
use chrono::{DateTime, Duration, Utc};
use common::{sample_check, sample_monitor, setup_pool_exclusive};
use domain::{CheckStatus, ErrorType};
use infra::{PostgresCheckRepository, PostgresMonitorRepository};
use serial_test::serial;

#[serial]
#[tokio::test]
async fn save_and_find_by_id_round_trips_check() {
    let (pool, _guard) = setup_pool_exclusive().await;
    let monitor_repository = PostgresMonitorRepository::new(pool.clone());
    let check_repository = PostgresCheckRepository::new(pool.clone());

    let monitor = sample_monitor(&pool, "Google", "https://google.com").await;
    monitor_repository.save(&monitor).await.unwrap();

    let check = sample_check(monitor.id);
    check_repository.save(&check).await.unwrap();

    let found = check_repository
        .find_by_id(check.id, check.checked_at)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(found.id, check.id);
    assert_eq!(found.monitor_id, monitor.id);
    assert_eq!(found.status, CheckStatus::Up);
    assert_eq!(found.status_code, Some(200));
    assert_eq!(found.latency_ms, 123);
    assert_eq!(found.error_type, ErrorType::None);
    assert_eq!(found.error_message, None);
}

#[serial]
#[tokio::test]
async fn list_by_monitor_returns_only_matching_checks() {
    let (pool, _guard) = setup_pool_exclusive().await;
    let monitor_repository = PostgresMonitorRepository::new(pool.clone());
    let check_repository = PostgresCheckRepository::new(pool.clone());

    let monitor_a = sample_monitor(&pool, "Monitor A", "https://a.example.com").await;
    let monitor_b = sample_monitor(&pool, "Monitor B", "https://b.example.com").await;
    monitor_repository.save(&monitor_a).await.unwrap();
    monitor_repository.save(&monitor_b).await.unwrap();

    let check_a = sample_check(monitor_a.id);
    let check_b = sample_check(monitor_b.id);

    check_repository.save(&check_a).await.unwrap();
    check_repository.save(&check_b).await.unwrap();

    let checks = check_repository
        .list_by_monitor(monitor_a.id)
        .await
        .unwrap();

    assert_eq!(checks.len(), 1);
    assert_eq!(checks[0].id, check_a.id);
}

#[serial]
#[tokio::test]
async fn delete_by_monitor_removes_all_checks_for_monitor() {
    let (pool, _guard) = setup_pool_exclusive().await;
    let monitor_repository = PostgresMonitorRepository::new(pool.clone());
    let check_repository = PostgresCheckRepository::new(pool.clone());

    let monitor_a = sample_monitor(&pool, "Monitor A", "https://a.example.com").await;
    let monitor_b = sample_monitor(&pool, "Monitor B", "https://b.example.com").await;
    monitor_repository.save(&monitor_a).await.unwrap();
    monitor_repository.save(&monitor_b).await.unwrap();

    let check_a1 = sample_check(monitor_a.id);
    let check_a2 = sample_check(monitor_a.id);
    let check_b = sample_check(monitor_b.id);

    check_repository.save(&check_a1).await.unwrap();
    check_repository.save(&check_a2).await.unwrap();
    check_repository.save(&check_b).await.unwrap();

    let deleted = check_repository
        .delete_by_monitor(monitor_a.id)
        .await
        .unwrap();

    assert_eq!(deleted, 2);
    assert!(
        check_repository
            .list_by_monitor(monitor_a.id)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        check_repository
            .list_by_monitor(monitor_b.id)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[serial]
#[tokio::test]
async fn list_checks_filtered_pagination_and_filters_are_correct() {
    let (pool, _guard) = setup_pool_exclusive().await;
    let monitor_repository = PostgresMonitorRepository::new(pool.clone());
    let check_repository = PostgresCheckRepository::new(pool.clone());

    let monitor_a = sample_monitor(&pool, "Filter A", "https://a.example.com").await;
    let mut monitor_b = sample_monitor(&pool, "Filter B", "https://b.example.com").await;

    // Put monitor B in the same tenant as monitor A so that both are
    // visible in tenant-scoped history queries. `sample_monitor` normally
    // creates a fresh tenant for each monitor.
    monitor_b.tenant_id = monitor_a.tenant_id;

    monitor_repository.save(&monitor_a).await.unwrap();
    monitor_repository.save(&monitor_b).await.unwrap();
    let tenant_id = monitor_a.tenant_id;

    let base = Utc::now() - Duration::days(10);
    for i in 0..6 {
        let mut check = sample_check(monitor_a.id);
        check.checked_at = base + Duration::minutes(i as i64);
        check.status = if i % 2 == 0 {
            CheckStatus::Up
        } else {
            CheckStatus::Down
        };
        check_repository.save(&check).await.unwrap();
    }

    let mut check_b = sample_check(monitor_b.id);
    check_b.checked_at = base + Duration::minutes(30);
    check_repository.save(&check_b).await.unwrap();

    let page1 = check_repository
        .list_checks_filtered(2, 0, tenant_id, CheckFilters::default())
        .await
        .unwrap();
    assert_eq!(page1.len(), 2);
    assert_eq!(
        page1[0].checked_at.timestamp_micros(),
        (base + Duration::minutes(30)).timestamp_micros()
    );
    assert_eq!(
        page1[1].checked_at.timestamp_micros(),
        (base + Duration::minutes(5)).timestamp_micros()
    );

    let up_checks = check_repository
        .list_checks_filtered(
            100,
            0,
            tenant_id,
            CheckFilters {
                status: Some(CheckStatus::Up),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(up_checks.len(), 4); // minutes 0,2,4 + monitor_b check
    assert!(up_checks.iter().all(|c| c.status == CheckStatus::Up));

    let monitor_a_checks = check_repository
        .list_checks_filtered(
            100,
            0,
            tenant_id,
            CheckFilters {
                monitor_id: Some(monitor_a.id),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(monitor_a_checks.len(), 6);
    assert!(
        monitor_a_checks
            .iter()
            .all(|c| c.monitor_id == monitor_a.id)
    );

    let range_checks = check_repository
        .list_checks_filtered(
            100,
            0,
            tenant_id,
            CheckFilters {
                from: Some(base + Duration::minutes(2)),
                to: Some(base + Duration::minutes(4)),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(range_checks.len(), 3); // minutes 2,3,4
}

#[serial]
#[tokio::test]
async fn list_checks_filtered_query_plan_uses_index() {
    let (pool, _guard) = setup_pool_exclusive().await;
    let monitor_repository = PostgresMonitorRepository::new(pool.clone());
    let check_repository = PostgresCheckRepository::new(pool.clone());

    let monitor = sample_monitor(&pool, "Plan Monitor", "https://plan.example.com").await;
    monitor_repository.save(&monitor).await.unwrap();
    let tenant_id = monitor.tenant_id;

    let base = Utc::now();
    for i in 0..200 {
        let mut check = sample_check(monitor.id);
        check.checked_at = base - Duration::seconds(i as i64);
        check_repository.save(&check).await.unwrap();
    }

    sqlx::query("ANALYZE checks").execute(&pool).await.unwrap();

    let sql = r#"
        EXPLAIN (FORMAT TEXT)
        SELECT c.id, c.monitor_id, c.region, c.status, c.http_status, c.response_time_ms,
               c.error_type, c.error_message, c.checked_at, c.cert_expires_at,
               c.cert_days_remaining, c.country_code
        FROM checks c
        JOIN monitors m ON c.monitor_id = m.id
        WHERE m.tenant_id = $1
          AND ($2::uuid IS NULL OR c.monitor_id = $2)
          AND ($3::smallint IS NULL OR c.status = $3)
          AND ($4::timestamptz IS NULL OR c.checked_at >= $4)
          AND ($5::timestamptz IS NULL OR c.checked_at <= $5)
        ORDER BY c.checked_at DESC
        LIMIT $6 OFFSET $7
    "#;

    let plan_rows: Vec<String> = sqlx::query_scalar::<_, String>(sql)
        .bind(tenant_id.0)
        .bind(Some(monitor.id.0))
        .bind(None::<i16>)
        .bind(None::<DateTime<Utc>>)
        .bind(None::<DateTime<Utc>>)
        .bind(10i64)
        .bind(0i64)
        .fetch_all(&pool)
        .await
        .unwrap();

    let plan = plan_rows.join("\n");
    assert!(
        !plan.contains("Seq Scan on checks"),
        "plan unexpectedly used a sequential scan on checks:\n{plan}"
    );
    assert!(
        plan.contains("Index") || plan.contains("Bitmap Index"),
        "plan did not reference an index on checks:\n{plan}"
    );
}

#[serial]
#[tokio::test]
async fn save_batch_persists_all_checks_in_one_call() {
    let (pool, _guard) = setup_pool_exclusive().await;
    let monitor_repository = PostgresMonitorRepository::new(pool.clone());
    let check_repository = PostgresCheckRepository::new(pool.clone());

    let monitor = sample_monitor(&pool, "Batch Monitor", "https://batch.example.com").await;
    monitor_repository.save(&monitor).await.unwrap();

    let checks: Vec<_> = (0..5).map(|_| sample_check(monitor.id)).collect();
    check_repository.save_batch(&checks).await.unwrap();

    for check in &checks {
        let found = check_repository
            .find_by_id(check.id, check.checked_at)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.id, check.id);
    }
}

/// `save_batch` must chunk internally so a caller-supplied slice
/// larger than PostgreSQL's 65,535-bind-parameter ceiling (~5,461 rows
/// at 12 params/row) still succeeds instead of failing the entire
/// batch. 1,200 rows crosses the repository's internal 500-row chunk
/// boundary at least twice, proving multiple chunks are all applied.
#[serial]
#[tokio::test]
async fn save_batch_persists_more_rows_than_a_single_chunk() {
    let (pool, _guard) = setup_pool_exclusive().await;
    let monitor_repository = PostgresMonitorRepository::new(pool.clone());
    let check_repository = PostgresCheckRepository::new(pool.clone());

    let monitor = sample_monitor(
        &pool,
        "Large Batch Monitor",
        "https://large-batch.example.com",
    )
    .await;
    monitor_repository.save(&monitor).await.unwrap();

    const N: usize = 1_200;
    let checks: Vec<_> = (0..N).map(|_| sample_check(monitor.id)).collect();
    check_repository.save_batch(&checks).await.unwrap();

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM checks WHERE monitor_id = $1")
        .bind(monitor.id.0)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(count, N as i64);
}

/// PERFORMANCE RULE — before/after benchmark.
///
/// Not run by default (`#[ignore]`) since it writes/measures 500 rows
/// and is timing-sensitive, not a correctness check. Run explicitly:
///
///   cargo test --release -p infra --test postgres_check_repository \
///       -- --ignored --nocapture benchmark_save_batch_vs_individual_saves
///
/// Prints wall-clock time for N individual `save()` calls (N DB
/// round-trips, the naive approach) versus one `save_batch()`
/// call (chunked multi-row INSERTs, the batched approach) writing the
/// same N checks. Record the printed numbers in your PR — do not
/// report this as "improved" without them.
#[serial]
#[tokio::test]
#[ignore]
async fn benchmark_save_batch_vs_individual_saves() {
    use std::time::Instant;

    const N: usize = 500;

    let (pool, _guard) = setup_pool_exclusive().await;
    let monitor_repository = PostgresMonitorRepository::new(pool.clone());
    let check_repository = PostgresCheckRepository::new(pool.clone());

    let monitor_individual = sample_monitor(
        &pool,
        "Benchmark Individual",
        "https://bench-individual.example.com",
    )
    .await;
    let monitor_batch =
        sample_monitor(&pool, "Benchmark Batch", "https://bench-batch.example.com").await;
    monitor_repository.save(&monitor_individual).await.unwrap();
    monitor_repository.save(&monitor_batch).await.unwrap();

    let individual_checks: Vec<_> = (0..N)
        .map(|_| sample_check(monitor_individual.id))
        .collect();
    let batch_checks: Vec<_> = (0..N).map(|_| sample_check(monitor_batch.id)).collect();

    let start_individual = Instant::now();
    for check in &individual_checks {
        check_repository.save(check).await.unwrap();
    }
    let individual_elapsed = start_individual.elapsed();

    let start_batch = Instant::now();
    check_repository.save_batch(&batch_checks).await.unwrap();
    let batch_elapsed = start_batch.elapsed();

    println!("=== save_batch benchmark (N = {N}) ===");
    println!(
        "Individual save() x{N} (naive, {N} DB round-trips): {:?}",
        individual_elapsed
    );
    println!(
        "save_batch() (chunked multi-row INSERT, {} round-trip(s)): {:?}",
        N.div_ceil(500),
        batch_elapsed
    );
    println!(
        "Speedup: {:.1}x",
        individual_elapsed.as_secs_f64() / batch_elapsed.as_secs_f64().max(0.000_001)
    );
}
