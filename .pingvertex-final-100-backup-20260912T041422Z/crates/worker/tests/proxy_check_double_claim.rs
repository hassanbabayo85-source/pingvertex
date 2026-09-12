use std::sync::Arc;

use application::ports::ProxyCheckJobRepository;
use domain::ProxyCheckJob;
use domain::shared::ids::{ProxyCheckJobId, TenantId, UserId};
use infra::{AesGcmProxyCredentialCipher, PostgresProxyCheckJobRepository, create_pool};
use sqlx::{PgPool, query};

async fn setup_pool_exclusive() -> PgPool {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
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

async fn insert_user_and_tenant(pool: &PgPool) -> (UserId, TenantId) {
    let user_id = UserId::new();

    query(
        r#"
        INSERT INTO users (id, email, password_hash, full_name, created_at, updated_at)
        VALUES ($1, $2, 'not-a-real-hash', 'Test User', NOW(), NOW())
        "#,
    )
    .bind(user_id.0)
    .bind(format!("{}@example.test", user_id.0))
    .execute(pool)
    .await
    .unwrap();

    let tenant_id = TenantId::new();
    query(
        r#"
        INSERT INTO tenants (id, name, owner_user_id, created_at)
        VALUES ($1, $2, $3, NOW())
        "#,
    )
    .bind(tenant_id.0)
    .bind(format!("Tenant {}", user_id.0))
    .bind(user_id.0)
    .execute(pool)
    .await
    .unwrap();

    query(
        r#"
        INSERT INTO tenant_memberships (tenant_id, user_id, role)
        VALUES ($1, $2, 'owner')
        "#,
    )
    .bind(tenant_id.0)
    .bind(user_id.0)
    .execute(pool)
    .await
    .unwrap();

    (user_id, tenant_id)
}

#[tokio::test]
async fn concurrent_proxy_job_claims_only_one_wins() {
    let pool = setup_pool_exclusive().await;
    let (user_id, tenant_id) = insert_user_and_tenant(&pool).await;

    let repo = PostgresProxyCheckJobRepository::new(
        pool.clone(),
        Arc::new(AesGcmProxyCredentialCipher::new([1u8; 32])),
    );

    let job_id = ProxyCheckJobId::new();
    let job = ProxyCheckJob::new(job_id, user_id, tenant_id, 0);
    repo.save_job(&job, &[]).await.unwrap();

    let repo_a = repo.clone();
    let repo_b = repo.clone();

    let (a, b) = tokio::join!(
        repo_a.try_mark_running(job_id, "worker-a"),
        repo_b.try_mark_running(job_id, "worker-b")
    );

    let a = a.unwrap();
    let b = b.unwrap();
    let winners = [a, b].into_iter().filter(|won| *won).count();

    assert_eq!(winners, 1, "exactly one concurrent claim must win");

    let row: (String,) = sqlx::query_as("SELECT status FROM proxy_check_jobs WHERE id = $1")
        .bind(job_id.0)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(row.0, "running");
}
