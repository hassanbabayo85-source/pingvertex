//! Task 16 acceptance test: `tenant_audit` pagination correctness.
//!
//! The tenant audit endpoint merges two independent sources
//! (`audit_log` + `financial_audit_log`) into a single cursor-paginated
//! response. This test seeds both sources with interleaved timestamps
//! and walks the entire endpoint, asserting that:
//!
//!   1. Every entry appears exactly once (no skips, no duplicates).
//!   2. The global ordering is `(created_at DESC, id DESC)`.
//!   3. Both sources contribute their full set.
//!
//! Uses in-memory repositories so the test does not need a live
//! Postgres. The cursor semantics being tested (ordering + tie-break)
//! are identical to the Postgres implementation's
//! `WHERE created_at < c OR (created_at = c AND id < c.id)` filter.

#![allow(dead_code)]

mod common;

use api::handlers::audit::{AuditQuery, tenant_audit};
use api::state::AppState;
use application::ports::{AuditLogEntryRepository, FinancialAuditLogRepository};
use axum::Json;
use axum::extract::{Extension, Query, State};
use chrono::{Duration, Utc};
use domain::shared::ids::{AuditLogEntryId, TenantId, UserId};
use domain::shared::types::{AuthenticatedUser, Role, TenantContext};
use domain::{AuditAction, AuditLogEntry, FinancialAuditLogEntry};
use infra::repositories::in_memory_financial_audit_log_repository::InMemoryFinancialAuditLogRepository;
use infra::{
    InMemoryApiFlowRepository, InMemoryAuditLogEntryRepository, InMemoryMonitorRepository,
    InMemoryNotificationChannelRepository, InMemoryProxyCheckJobRepository,
    NoopNotificationService, UuidIdGenerator,
};
use std::sync::Arc;
use uuid::Uuid;

#[tokio::test]
async fn tenant_audit_pagination_walks_all_entries_exactly_once() {
    let user_id = UserId::new();
    let other_user_id = UserId::new();
    let tenant_id = TenantId::new();
    let base = Utc::now();

    let generic_repo: Arc<InMemoryAuditLogEntryRepository> =
        Arc::new(InMemoryAuditLogEntryRepository::new());
    let financial_repo: Arc<InMemoryFinancialAuditLogRepository> =
        Arc::new(InMemoryFinancialAuditLogRepository::new());

    let mut expected_targets: Vec<String> = Vec::new();

    // Generic stream: t+0, +10, +20, +30, +40
    for i in 0..5i64 {
        let target = format!("generic-{i}");
        let entry = AuditLogEntry {
            id: AuditLogEntryId::new(),
            actor_user_id: user_id,
            tenant_id: Some(tenant_id),
            action: AuditAction::MonitorCreated,
            target_id: target.clone(),
            metadata: serde_json::json!({"source": "generic", "i": i}),
            created_at: base + Duration::seconds(i * 10),
        };
        expected_targets.push(target);
        AuditLogEntryRepository::create(generic_repo.as_ref(), &entry)
            .await
            .expect("seed generic audit entry");
    }

    // Financial stream: t+5, +15, +25, +35, +45 (interleaved with generic)
    for i in 0..5i64 {
        let resource_id = format!("financial-{i}");
        let entry = FinancialAuditLogEntry {
            id: Uuid::new_v4(),
            actor: user_id.0.to_string(),
            tenant_id: Some(tenant_id.0),
            action: "wallet_credited".to_string(),
            resource: "wallet_transaction".to_string(),
            resource_id: Some(resource_id.clone()),
            metadata: serde_json::json!({"source": "financial", "i": i}),
            created_at: base + Duration::seconds(i * 10 + 5),
        };
        expected_targets.push(resource_id);
        FinancialAuditLogRepository::create(financial_repo.as_ref(), &entry)
            .await
            .expect("seed financial audit entry");
    }

    let other_entry = FinancialAuditLogEntry {
        id: Uuid::new_v4(),
        actor: other_user_id.0.to_string(),
        tenant_id: Some(tenant_id.0),
        action: "wallet_credited".to_string(),
        resource: "wallet_transaction".to_string(),
        resource_id: Some("financial-other-actor".to_owned()),
        metadata: serde_json::json!({"source": "financial", "actor": "other"}),
        created_at: base + Duration::seconds(55),
    };
    FinancialAuditLogRepository::create(financial_repo.as_ref(), &other_entry)
        .await
        .expect("seed other-actor financial audit entry");
    expected_targets.push("financial-other-actor".to_owned());

    let state = AppState::new(
        InMemoryMonitorRepository::new(),
        common::NoopCheckRepository,
        common::NoopDiagnosticReportRepository,
        common::NoopIncidentRepository,
        InMemoryNotificationChannelRepository::new(InMemoryMonitorRepository::new()),
        NoopNotificationService::new(),
        InMemoryApiFlowRepository::new(),
        InMemoryProxyCheckJobRepository::new(),
        UuidIdGenerator::new(),
        Arc::new(common::NoopSecretCipher),
        generic_repo.clone(),
    )
    .with_financial_audit_log_repository(financial_repo.clone());

    let auth = AuthenticatedUser::new(user_id, Role::Owner, TenantContext::new(tenant_id));

    let Json(filtered_page) = tenant_audit(
        State(state.clone()),
        Extension(auth),
        Query(AuditQuery {
            cursor: None,
            limit: Some(100),
            action: None,
            actor: Some(user_id.0),
            tenant: None,
            target: None,
            from: None,
            to: None,
        }),
    )
    .await
    .expect("actor-filtered tenant audit must succeed");
    assert!(
        filtered_page
            .items
            .iter()
            .all(|item| item.actor_user_id == user_id.0.to_string()),
        "actor filter must apply to both generic and financial audit streams"
    );
    assert!(
        filtered_page
            .items
            .iter()
            .any(|item| item.source == "financial"),
        "matching financial audit entries must remain visible"
    );
    assert!(
        filtered_page
            .items
            .iter()
            .all(|item| item.target_id != "financial-other-actor"),
        "financial entries by another actor must be excluded"
    );

    const PAGE_SIZE: u32 = 3;
    let mut collected = Vec::new();
    let mut cursor: Option<String> = None;
    let mut page_count = 0usize;

    loop {
        page_count += 1;
        assert!(
            page_count <= 10,
            "pagination must terminate within 10 pages"
        );

        let query = AuditQuery {
            cursor: cursor.clone(),
            limit: Some(PAGE_SIZE),
            action: None,
            actor: None,
            tenant: None,
            target: None,
            from: None,
            to: None,
        };
        let Json(page) = tenant_audit(State(state.clone()), Extension(auth), Query(query))
            .await
            .expect("tenant_audit must succeed");

        for item in page.items {
            collected.push(item);
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }

    // 1. Every expected target appeared exactly once, no extras.
    assert_eq!(
        collected.len(),
        expected_targets.len(),
        "collected {} entries over {} pages, expected {}",
        collected.len(),
        page_count,
        expected_targets.len()
    );

    let mut collected_targets: Vec<String> =
        collected.iter().map(|e| e.target_id.clone()).collect();
    collected_targets.sort();
    let mut expected_sorted = expected_targets.clone();
    expected_sorted.sort();
    assert_eq!(
        collected_targets, expected_sorted,
        "collected target_ids must match expected exactly (no duplicates, no skips)"
    );

    // 2. Global ordering (created_at DESC, id DESC).
    for w in collected.windows(2) {
        let prev = &w[0];
        let next = &w[1];
        assert!(
            prev.created_at >= next.created_at,
            "order violated: {} ({}) then {} ({})",
            prev.id,
            prev.created_at,
            next.id,
            next.created_at
        );
        if prev.created_at == next.created_at {
            let prev_uuid = Uuid::parse_str(&prev.id).unwrap();
            let next_uuid = Uuid::parse_str(&next.id).unwrap();
            assert!(
                prev_uuid >= next_uuid,
                "tie-break violated: {} vs {}",
                prev.id,
                next.id
            );
        }
    }

    // 3. Both sources contributed their full 5 entries.
    let generic_count = collected.iter().filter(|e| e.source == "audit").count();
    let financial_count = collected.iter().filter(|e| e.source == "financial").count();
    assert_eq!(generic_count, 5);
    assert_eq!(financial_count, 6);
}
