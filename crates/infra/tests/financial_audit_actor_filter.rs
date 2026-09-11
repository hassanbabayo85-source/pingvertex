//! Task 17 regression: the platform admin audit endpoint can filter
//! financial audit entries by `actor`.
//!
//! `financial_audit_log.actor` is a free-form text column. User-initiated
//! entries store the acting user's UUID as a string; system entries use
//! namespaced strings like `"billing:webhook"` and `"system:payg"`. This
//! test proves that passing a user UUID to `list_page_admin` selects
//! exactly the entries that were written with that UUID as their actor,
//! and no others.
//!
//! Before Task 17, `FinancialAuditLogFilter` had no `actor` field at all,
//! so the admin audit endpoint could not narrow the financial stream by
//! actor even though the generic stream was already filterable that way.

use application::ports::{
    FinancialAuditLogFilter, FinancialAuditLogRepository,
};
use domain::FinancialAuditLogEntry;
use infra::repositories::in_memory_financial_audit_log_repository::InMemoryFinancialAuditLogRepository;
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn list_page_admin_filters_financial_audit_by_actor() {
    let repo = InMemoryFinancialAuditLogRepository::new();
    let tenant_id = Uuid::new_v4();
    let user_a = Uuid::new_v4();
    let user_b = Uuid::new_v4();

    // Two entries by user A, one by user B, one system-generated.
    for (actor, action, resource_id) in [
        (user_a.to_string(), "wallet_credited", "tx-a-1"),
        (user_a.to_string(), "wallet_debited", "tx-a-2"),
        (user_b.to_string(), "wallet_credited", "tx-b-1"),
        ("billing:webhook".to_string(), "payment_created", "pay-1"),
    ] {
        FinancialAuditLogRepository::create(
            &repo,
            &FinancialAuditLogEntry::new(
                actor,
                Some(tenant_id),
                action,
                "wallet_transaction",
                Some(resource_id.to_string()),
                json!({}),
            ),
        )
        .await
        .unwrap();
    }

    // Filter by user A's UUID (as string, matching the audit writer).
    let page = repo
        .list_page_admin(
            Some(tenant_id),
            &FinancialAuditLogFilter {
                actor: Some(user_a.to_string()),
                ..Default::default()
            },
            None,
            100,
        )
        .await
        .unwrap();

    assert_eq!(
        page.entries.len(),
        2,
        "exactly the two entries written by user A must be returned"
    );
    assert!(
        page.entries
            .iter()
            .all(|e| e.actor == user_a.to_string()),
        "every returned entry must have user A as its actor"
    );

    // Filter by user B's UUID.
    let page_b = repo
        .list_page_admin(
            Some(tenant_id),
            &FinancialAuditLogFilter {
                actor: Some(user_b.to_string()),
                ..Default::default()
            },
            None,
            100,
        )
        .await
        .unwrap();
    assert_eq!(page_b.entries.len(), 1);
    assert_eq!(page_b.entries[0].actor, user_b.to_string());

    // Filter by system actor string.
    let page_sys = repo
        .list_page_admin(
            Some(tenant_id),
            &FinancialAuditLogFilter {
                actor: Some("billing:webhook".to_string()),
                ..Default::default()
            },
            None,
            100,
        )
        .await
        .unwrap();
    assert_eq!(page_sys.entries.len(), 1);
    assert_eq!(page_sys.entries[0].actor, "billing:webhook");

    // No actor filter → all four entries.
    let page_all = repo
        .list_page_admin(Some(tenant_id), &FinancialAuditLogFilter::default(), None, 100)
        .await
        .unwrap();
    assert_eq!(page_all.entries.len(), 4);
}
