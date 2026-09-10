use application::ports::{AuditLogEntryRepository, AuditLogFilter};
use domain::{TenantId, UserId};
use infra::PostgresAuditLogEntryRepository;
use serial_test::serial;
use uuid::Uuid;

mod common;

#[tokio::test]
#[serial]
async fn audit_log_is_database_append_only() {
    let pool = common::setup_pool().await;
    let user = Uuid::new_v4();
    let tenant = common::insert_user(&pool, UserId(user)).await;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO audit_log(id,actor_user_id,tenant_id,action,target_id,metadata) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(id).bind(user).bind(tenant.0).bind("MonitorCreated").bind("monitor-test")
        .bind(serde_json::json!({"immutable":true})).execute(&pool).await.unwrap();

    let update = sqlx::query("UPDATE audit_log SET target_id='tampered' WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await;
    assert!(
        update.is_err(),
        "audit UPDATE must be rejected by the DB trigger"
    );
    let delete = sqlx::query("DELETE FROM audit_log WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await;
    assert!(
        delete.is_err(),
        "audit DELETE must be rejected by the DB trigger"
    );

    let row: (String,) = sqlx::query_as("SELECT target_id FROM audit_log WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.0, "monitor-test");
}

#[tokio::test]
#[serial]
async fn audit_page_filters_and_tenant_scope_are_bounded() {
    let pool = common::setup_pool().await;
    let user_a = Uuid::new_v4();
    let tenant_a = common::insert_user(&pool, UserId(user_a)).await;
    let user_b = Uuid::new_v4();
    let tenant_b = common::insert_user(&pool, UserId(user_b)).await;

    for (tenant, user, action, target) in [
        (tenant_a.0, user_a, "MonitorCreated", "a-1"),
        (tenant_a.0, user_a, "WalletCredit", "a-2"),
        (tenant_b.0, user_b, "MonitorCreated", "b-1"),
    ] {
        sqlx::query("INSERT INTO audit_log(id,actor_user_id,tenant_id,action,target_id,metadata,created_at) VALUES($1,$2,$3,$4,$5,'{}',now())")
            .bind(Uuid::new_v4()).bind(user).bind(tenant).bind(action).bind(target).execute(&pool).await.unwrap();
    }

    let repo = PostgresAuditLogEntryRepository::new(pool);
    let page = repo
        .list_page(
            &AuditLogFilter {
                tenant_id: Some(TenantId(tenant_a.0)),
                ..Default::default()
            },
            None,
            1,
        )
        .await
        .unwrap();
    assert_eq!(page.entries.len(), 1);
    assert!(page.has_more);
    assert_eq!(page.entries[0].tenant_id, Some(TenantId(tenant_a.0)));

    let filtered = repo
        .list_page(
            &AuditLogFilter {
                tenant_id: Some(TenantId(tenant_a.0)),
                action: Some("WalletCredit".to_owned()),
                ..Default::default()
            },
            None,
            100,
        )
        .await
        .unwrap();
    assert_eq!(filtered.entries.len(), 1);
    assert_eq!(filtered.entries[0].target_id, "a-2");
}
