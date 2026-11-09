//! Acceptance test: `tenant_audit` cursor-pagination correctness.
//!
//! The tenant audit endpoint returns a cursor-paginated view of
//! `audit_log`. The PAYG-era version of this test also verified that
//! entries from a separate `financial_audit_log` source were merged in,
//! but that source was removed with the billing surface in phases 1
//! and 3. This test verifies the remaining invariant on the single
//! `audit_log` source: walking every page visits every entry exactly
//! once, in `(created_at DESC, id DESC)` order, with no duplicates and
//! no skips.
//!
//! Uses in-memory repositories so the test does not need a live
//! Postgres. The cursor semantics being tested (ordering + tie-break)
//! are identical to the Postgres implementation's
//! `WHERE created_at < c OR (created_at = c AND id < c.id)` filter.

#![allow(dead_code)]

mod common;

use api::handlers::audit::{AuditQuery, tenant_audit};
use api::state::AppState;
use application::ports::AuditLogEntryRepository;
use axum::Json;
use axum::extract::{Extension, Query, State};
use chrono::{Duration, Utc};
use domain::shared::ids::{AuditLogEntryId, TenantId, UserId};
use domain::shared::types::{AuthenticatedUser, Role, TenantContext};
use domain::{AuditAction, AuditLogEntry};
use infra::{
    InMemoryApiFlowRepository, InMemoryAuditLogEntryRepository, InMemoryMonitorRepository,
    InMemoryNotificationChannelRepository, InMemoryProxyCheckJobRepository,
    NoopNotificationService, UuidIdGenerator,
};
use std::sync::Arc;
use uuid::Uuid;

fn build_state(audit_repo: Arc<InMemoryAuditLogEntryRepository>) -> AppState {
    AppState::new(
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
        audit_repo,
    )
}

#[tokio::test]
async fn tenant_audit_pagination_walks_all_entries_exactly_once() {
    let user_id = UserId::new();
    let tenant_id = TenantId::new();
    let base = Utc::now();

    let audit_repo: Arc<InMemoryAuditLogEntryRepository> =
        Arc::new(InMemoryAuditLogEntryRepository::new());

    let mut expected_targets: Vec<String> = Vec::new();

    // Seed 12 generic audit entries at t+0, +10, +20, ...
    for i in 0..12i64 {
        let target = format!("generic-{i:02}");
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
        AuditLogEntryRepository::create(audit_repo.as_ref(), &entry)
            .await
            .expect("seed generic audit entry");
    }

    let state = build_state(audit_repo.clone());
    let auth = AuthenticatedUser::new(user_id, Role::Owner, TenantContext::new(tenant_id));

    // Walk pages of 3. The handler clamps limit to [1, MAX_LIMIT] and
    // requests `limit + 1` internally to detect has_more.
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

    // 3. All entries came from the generic audit source (financial is
    //    gone).
    assert!(
        collected.iter().all(|e| e.source == "audit"),
        "every entry must have source=audit after billing removal"
    );
}

#[tokio::test]
async fn tenant_audit_actor_filter_applies() {
    let user_id = UserId::new();
    let other_user_id = UserId::new();
    let tenant_id = TenantId::new();
    let base = Utc::now();

    let audit_repo: Arc<InMemoryAuditLogEntryRepository> =
        Arc::new(InMemoryAuditLogEntryRepository::new());

    // Three entries for the primary user.
    for i in 0..3i64 {
        let entry = AuditLogEntry {
            id: AuditLogEntryId::new(),
            actor_user_id: user_id,
            tenant_id: Some(tenant_id),
            action: AuditAction::MonitorCreated,
            target_id: format!("mine-{i}"),
            metadata: serde_json::json!({}),
            created_at: base + Duration::seconds(i),
        };
        AuditLogEntryRepository::create(audit_repo.as_ref(), &entry)
            .await
            .unwrap();
    }

    // Two entries for another user in the same tenant.
    for i in 0..2i64 {
        let entry = AuditLogEntry {
            id: AuditLogEntryId::new(),
            actor_user_id: other_user_id,
            tenant_id: Some(tenant_id),
            action: AuditAction::MonitorCreated,
            target_id: format!("other-{i}"),
            metadata: serde_json::json!({}),
            created_at: base + Duration::seconds(i),
        };
        AuditLogEntryRepository::create(audit_repo.as_ref(), &entry)
            .await
            .unwrap();
    }

    let state = build_state(audit_repo.clone());
    let auth = AuthenticatedUser::new(user_id, Role::Owner, TenantContext::new(tenant_id));

    let Json(page) = tenant_audit(
        State(state),
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

    assert_eq!(
        page.items.len(),
        3,
        "only the primary user's entries must appear"
    );
    assert!(
        page.items
            .iter()
            .all(|e| e.actor_user_id == user_id.0.to_string()),
        "actor filter must exclude the other user's entries"
    );
}
