use application::ports::DashboardRepository;
use domain::shared::ids::{CheckId, IncidentId, MonitorId, TenantId, UserId};
use domain::{
    Check, CheckInterval, CheckStatus, ErrorType, ExpectedStatus, HttpMethod, Incident,
    MonitorBuilder, MonitorName, RegionCode, Timeout, Url,
};
use infra::PostgresDashboardRepository;
use sqlx::PgPool;
use uuid::Uuid;

mod common;

async fn insert_user_and_tenant(pool: &PgPool, user_id: UserId) -> TenantId {
    let email = format!("{}@example.test", user_id.0);
    let tenant_id = TenantId::new();
    sqlx::query("INSERT INTO users (id, email, password_hash, full_name, created_at, updated_at) VALUES ($1,$2,$3,$4,NOW(),NOW())")
        .bind(user_id.0).bind(email).bind("not-a-real-hash").bind("Test User")
        .execute(pool).await.unwrap();
    sqlx::query(
        "INSERT INTO tenants (id, name, owner_user_id, created_at) VALUES ($1,$2,$3,NOW())",
    )
    .bind(tenant_id.0)
    .bind(format!("Tenant {}", user_id.0))
    .bind(user_id.0)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO tenant_memberships (tenant_id, user_id, role, invited_at, joined_at) VALUES ($1,$2,'Owner',NOW(),NOW())")
        .bind(tenant_id.0).bind(user_id.0)
        .execute(pool).await.unwrap();
    tenant_id
}

async fn insert_monitor_with_data(pool: &PgPool, tenant_id: TenantId, user_id: UserId, name: &str) {
    let monitor = MonitorBuilder::new(MonitorId(Uuid::new_v4()), user_id, tenant_id)
        .name(MonitorName::new(name).unwrap())
        .url(Url::new("https://example.com").unwrap())
        .method(HttpMethod::Get)
        .expected_status(ExpectedStatus::new(200).unwrap())
        .interval(CheckInterval::new(60).unwrap())
        .timeout(Timeout::new(10).unwrap())
        .build();

    sqlx::query(
        r#"
        INSERT INTO monitors (id, user_id, tenant_id, regions, name, url, method, expected_status,
            interval_seconds, timeout_seconds, enabled, is_public, ssl_check_enabled, ssl_expiry_warning_days,
            kind, created_at, updated_at)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)
        "#,
    )
    .bind(monitor.id.0)
    .bind(monitor.user_id.0)
    .bind(monitor.tenant_id.0)
    .bind(Vec::<String>::new())
    .bind(monitor.name.as_str())
    .bind(monitor.url.clone())
    .bind("GET")
    .bind(200i16)
    .bind(60i32)
    .bind(10i32)
    .bind(true)
    .bind(false)
    .bind(true)
    .bind(14i32)
    .bind("http")
    .bind(monitor.created_at)
    .bind(monitor.updated_at)
    .execute(pool)
    .await
    .unwrap();

    let check = Check::new(
        CheckId(Uuid::new_v4()),
        monitor.id,
        RegionCode::default_region(),
        CheckStatus::Up,
        Some(200),
        100,
        ErrorType::None,
        None,
    );
    sqlx::query("INSERT INTO checks (id, monitor_id, region, status, http_status, response_time_ms, error_type, checked_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(check.id.0).bind(check.monitor_id.0).bind(check.region.to_string())
        .bind(1i16).bind(check.status_code.map(i32::from)).bind(check.latency_ms as i64)
        .bind("None").bind(check.checked_at)
        .execute(pool).await.unwrap();

    let incident = Incident::new(IncidentId(Uuid::new_v4()), monitor.id);
    sqlx::query("INSERT INTO incidents (id, monitor_id, started_at, resolved_at, last_seen_at, created_at, updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7)")
        .bind(incident.id.0).bind(incident.monitor_id.0).bind(incident.started_at)
        .bind(incident.resolved_at).bind(incident.last_seen_at).bind(incident.created_at).bind(incident.updated_at)
        .execute(pool).await.unwrap();
}

#[tokio::test]
#[serial_test::serial]
async fn get_overview_handles_1000_monitors_without_n_plus_one_queries() {
    let pool = common::setup_pool().await;
    let repo = PostgresDashboardRepository::new(pool.clone());

    let user_id = UserId(Uuid::new_v4());
    let tenant_id = insert_user_and_tenant(&pool, user_id).await;

    let start = std::time::Instant::now();
    for i in 0..1000 {
        let name = format!("monitor-{i}");
        insert_monitor_with_data(&pool, tenant_id, user_id, &name).await;
    }
    let insertion_elapsed = start.elapsed();
    println!("Inserted 1000 monitors in {:?}", insertion_elapsed);

    let query_start = std::time::Instant::now();
    let overview = repo
        .get_overview(tenant_id)
        .await
        .expect("get_overview should succeed");
    let query_elapsed = query_start.elapsed();
    println!("Fetched overview in {:?}", query_elapsed);

    assert_eq!(overview.monitor_summary.total, 1000);
    assert_eq!(overview.monitor_summary.up, 1000);
    assert_eq!(overview.monitor_summary.down, 0);
    assert_eq!(overview.monitor_summary.disabled, 0);
    assert_eq!(overview.monitors.len(), 1000);
    assert_eq!(overview.incident_summary.open_count, 1000);
    assert_eq!(overview.incident_summary.recent.len(), 5);

    assert!(
        query_elapsed.as_secs_f64() < 2.0,
        "dashboard overview query took too long: {:?} — likely N+1",
        query_elapsed
    );
}

#[tokio::test]
#[serial_test::serial]
async fn get_overview_respects_tenant_isolation() {
    let pool = common::setup_pool().await;
    let repo = PostgresDashboardRepository::new(pool.clone());

    let user_a = UserId(Uuid::new_v4());
    let tenant_a = insert_user_and_tenant(&pool, user_a).await;
    let user_b = UserId(Uuid::new_v4());
    let tenant_b = insert_user_and_tenant(&pool, user_b).await;

    insert_monitor_with_data(&pool, tenant_a, user_a, "tenant-a-monitor").await;

    let overview_a = repo.get_overview(tenant_a).await.unwrap();
    assert_eq!(overview_a.monitor_summary.total, 1);
    assert_eq!(overview_a.monitors.len(), 1);

    let overview_b = repo.get_overview(tenant_b).await.unwrap();
    assert_eq!(overview_b.monitor_summary.total, 0);
    assert_eq!(overview_b.monitors.len(), 0);
    assert_eq!(overview_b.incident_summary.open_count, 0);
}
