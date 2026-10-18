//! Behavioral tests for `InMemoryRecurringProxyMonitorRepository`.
//!
//! These tests exercise every contract the worker and API will rely
//! on: tenant isolation, atomic quota, per-tenant dedup of due monitors,
//! lease semantics, checks history. No database is required.

use std::sync::Arc;

use application::ports::monitor_repository::QuotaCheckOutcome;
use application::ports::RecurringProxyMonitorRepository;
use chrono::{Duration, Utc};
use domain::entities::{
    RecurringProxyCheck, RecurringProxyCheckStatus, RecurringProxyMonitor,
};
use domain::enums::ProxyScheme;
use domain::shared::ids::{TenantId, UserId};
use domain::value_objects::{ProxyCredentials, ProxyHost, ProxyResult};
use infra::InMemoryRecurringProxyMonitorRepository;
use uuid::Uuid;

// ── Helpers ─────────────────────────────────────────────────────────

fn monitor(
    tenant: TenantId,
    user: UserId,
    name: &str,
    with_credentials: bool,
) -> RecurringProxyMonitor {
    let creds = if with_credentials {
        Some(ProxyCredentials::new("user", "pass").unwrap())
    } else {
        None
    };
    RecurringProxyMonitor::new(
        Uuid::new_v4(),
        tenant,
        user,
        name,
        ProxyHost::new("8.8.8.8").unwrap(),
        8080,
        ProxyScheme::Http,
        creds,
        300,
    )
    .unwrap()
}

fn sample_result() -> ProxyResult {
    ProxyResult {
        host: "8.8.8.8".to_string(),
        port: 8080,
        scheme: "http".to_string(),
        reachable: true,
        latency_ms: Some(120),
        anonymity: None,
        supports_https: true,
        country_code: Some("US".to_string()),
        error_code: None,
        error_message: None,
        outcome: None,
        failure_kind: None,
        connection_latency_ms: Some(30),
        tls_handshake_latency_ms: Some(50),
        ttfb_ms: Some(100),
        total_time_ms: Some(120),
        http_status_code: Some(200),
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[tokio::test]
async fn save_and_find_round_trips_with_credentials() {
    let repo = InMemoryRecurringProxyMonitorRepository::new();
    let tenant = TenantId::new();
    let user = UserId::new();
    let m = monitor(tenant, user, "test", true);
    let id = m.id;

    repo.save_monitor(&m).await.unwrap();

    let loaded = repo.find_by_id(id, tenant).await.unwrap().unwrap();
    assert_eq!(loaded.name, "test");
    assert_eq!(loaded.interval_seconds, 300);
    assert!(loaded.credentials.is_some());
    assert_eq!(loaded.credentials.as_ref().unwrap().username(), "user");
    assert_eq!(loaded.credentials.as_ref().unwrap().password(), "pass");
}

#[tokio::test]
async fn save_and_find_round_trips_without_credentials() {
    let repo = InMemoryRecurringProxyMonitorRepository::new();
    let tenant = TenantId::new();
    let m = monitor(tenant, UserId::new(), "no-creds", false);
    let id = m.id;

    repo.save_monitor(&m).await.unwrap();
    let loaded = repo.find_by_id(id, tenant).await.unwrap().unwrap();
    assert!(loaded.credentials.is_none());
}

#[tokio::test]
async fn find_by_id_is_tenant_scoped() {
    let repo = InMemoryRecurringProxyMonitorRepository::new();
    let tenant_a = TenantId::new();
    let tenant_b = TenantId::new();
    let m = monitor(tenant_a, UserId::new(), "a", false);
    let id = m.id;
    repo.save_monitor(&m).await.unwrap();

    assert!(repo.find_by_id(id, tenant_a).await.unwrap().is_some());
    assert!(repo.find_by_id(id, tenant_b).await.unwrap().is_none());
}

#[tokio::test]
async fn list_is_tenant_scoped_and_sorted_newest_first() {
    let repo = InMemoryRecurringProxyMonitorRepository::new();
    let tenant_a = TenantId::new();
    let tenant_b = TenantId::new();

    repo.save_monitor(&monitor(tenant_a, UserId::new(), "a1", false))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    repo.save_monitor(&monitor(tenant_a, UserId::new(), "a2", false))
        .await
        .unwrap();
    repo.save_monitor(&monitor(tenant_b, UserId::new(), "b1", false))
        .await
        .unwrap();

    let list_a = repo.list_for_tenant(tenant_a).await.unwrap();
    assert_eq!(list_a.len(), 2);
    assert_eq!(list_a[0].name, "a2");
    assert_eq!(list_a[1].name, "a1");

    let list_b = repo.list_for_tenant(tenant_b).await.unwrap();
    assert_eq!(list_b.len(), 1);
    assert_eq!(list_b[0].name, "b1");
}

#[tokio::test]
async fn quota_is_per_tenant_and_atomic() {
    let repo = Arc::new(InMemoryRecurringProxyMonitorRepository::new());
    let tenant_a = TenantId::new();
    let tenant_b = TenantId::new();

    // Fill tenant_a up to the limit (2).
    for i in 0..2 {
        let m = monitor(tenant_a, UserId::new(), &format!("a{i}"), false);
        let outcome = repo
            .save_monitor_if_within_quota(&m, tenant_a, 2)
            .await
            .unwrap();
        assert_eq!(outcome, QuotaCheckOutcome::Saved);
    }

    // 3rd for tenant_a rejected.
    let over = monitor(tenant_a, UserId::new(), "a-over", false);
    let outcome = repo
        .save_monitor_if_within_quota(&over, tenant_a, 2)
        .await
        .unwrap();
    assert_eq!(outcome, QuotaCheckOutcome::QuotaExceeded);

    // tenant_b still has room.
    let b = monitor(tenant_b, UserId::new(), "b0", false);
    let outcome = repo
        .save_monitor_if_within_quota(&b, tenant_b, 2)
        .await
        .unwrap();
    assert_eq!(outcome, QuotaCheckOutcome::Saved);

    // Concurrent saves from multiple tasks against tenant_b (limit 2, 1
    // used) must all fail except one.
    let mut handles = Vec::new();
    for i in 0..10 {
        let repo = repo.clone();
        handles.push(tokio::spawn(async move {
            let m = monitor(tenant_b, UserId::new(), &format!("b-{i}"), false);
            repo.save_monitor_if_within_quota(&m, tenant_b, 2).await
        }));
    }
    let mut saved = 0;
    let mut rejected = 0;
    for h in handles {
        match h.await.unwrap().unwrap() {
            QuotaCheckOutcome::Saved => saved += 1,
            QuotaCheckOutcome::QuotaExceeded => rejected += 1,
        }
    }
    assert_eq!(saved, 1, "exactly one of 10 concurrent calls should succeed");
    assert_eq!(rejected, 9);
}

#[tokio::test]
async fn delete_removes_monitor_and_its_checks() {
    let repo = InMemoryRecurringProxyMonitorRepository::new();
    let tenant = TenantId::new();
    let m = monitor(tenant, UserId::new(), "to-delete", false);
    let id = m.id;
    repo.save_monitor(&m).await.unwrap();

    let check = RecurringProxyCheck::new_running(Uuid::new_v4(), id, tenant);
    repo.save_check(&check).await.unwrap();

    assert!(repo.delete_for_tenant(id, tenant).await.unwrap());
    assert!(repo.find_by_id(id, tenant).await.unwrap().is_none());
    let (checks, total) = repo.list_checks(id, tenant, 0, 10).await.unwrap();
    assert_eq!(total, 0);
    assert!(checks.is_empty());
}

#[tokio::test]
async fn delete_is_tenant_scoped() {
    let repo = InMemoryRecurringProxyMonitorRepository::new();
    let tenant_a = TenantId::new();
    let tenant_b = TenantId::new();
    let m = monitor(tenant_a, UserId::new(), "a", false);
    let id = m.id;
    repo.save_monitor(&m).await.unwrap();

    assert!(!repo.delete_for_tenant(id, tenant_b).await.unwrap());
    assert!(repo.find_by_id(id, tenant_a).await.unwrap().is_some());
}

#[tokio::test]
async fn claim_due_monitors_only_returns_effective_enabled_due_monitors() {
    let repo = InMemoryRecurringProxyMonitorRepository::new();
    let tenant = TenantId::new();
    let user = UserId::new();

    // Monitor 1: enabled, due (next_run_at in the past).
    let mut m1 = monitor(tenant, user, "due", false);
    m1.next_run_at = Some(Utc::now() - Duration::seconds(10));
    repo.save_monitor(&m1).await.unwrap();

    // Monitor 2: user disabled.
    let mut m2 = monitor(tenant, user, "disabled", false);
    m2.set_user_enabled(false);
    m2.next_run_at = Some(Utc::now() - Duration::seconds(10));
    repo.save_monitor(&m2).await.unwrap();

    // Monitor 3: entitlement suspended.
    let mut m3 = monitor(tenant, user, "suspended", false);
    m3.set_entitlement_suspended(true);
    m3.next_run_at = Some(Utc::now() - Duration::seconds(10));
    repo.save_monitor(&m3).await.unwrap();

    // Monitor 4: not yet due.
    let m4 = monitor(tenant, user, "future", false);
    // next_run_at defaults to now + 300s → not due.
    repo.save_monitor(&m4).await.unwrap();

    let claimed = repo
        .claim_due_monitors(
            Utc::now(),
            "worker-1",
            Utc::now() + Duration::minutes(1),
            10,
        )
        .await
        .unwrap();

    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].monitor.name, "due");
    assert_eq!(claimed[0].worker_id, "worker-1");
}

#[tokio::test]
async fn claim_respects_limit_and_orders_by_next_run_at() {
    let repo = InMemoryRecurringProxyMonitorRepository::new();
    let tenant = TenantId::new();
    let user = UserId::new();
    let now = Utc::now();

    let mut ids = Vec::new();
    for i in 0..5 {
        let mut m = monitor(tenant, user, &format!("m{i}"), false);
        m.next_run_at = Some(now - Duration::seconds((5 - i) as i64));
        ids.push(m.id);
        repo.save_monitor(&m).await.unwrap();
    }

    // Request 2 → must get the 2 earliest.
    let claimed = repo
        .claim_due_monitors(now, "w", now + Duration::minutes(1), 2)
        .await
        .unwrap();
    assert_eq!(claimed.len(), 2);
    // ids[0] is oldest (m0), ids[1] is m1 → earliest first.
    let claimed_ids: Vec<Uuid> = claimed.iter().map(|c| c.monitor.id).collect();
    assert_eq!(claimed_ids, vec![ids[0], ids[1]]);
}

#[tokio::test]
async fn concurrent_claims_do_not_return_same_monitor_twice() {
    let repo = Arc::new(InMemoryRecurringProxyMonitorRepository::new());
    let tenant = TenantId::new();
    let user = UserId::new();
    let now = Utc::now();

    let mut m = monitor(tenant, user, "only", false);
    m.next_run_at = Some(now - Duration::seconds(1));
    repo.save_monitor(&m).await.unwrap();

    let mut handles = Vec::new();
    for i in 0..5 {
        let repo = repo.clone();
        handles.push(tokio::spawn(async move {
            repo.claim_due_monitors(now, &format!("worker-{i}"), now + Duration::minutes(1), 10)
                .await
        }));
    }

    let mut total_claimed = 0;
    for h in handles {
        total_claimed += h.await.unwrap().unwrap().len();
    }
    assert_eq!(total_claimed, 1, "monitor must be claimed exactly once");
}

#[tokio::test]
async fn reschedule_advances_next_run_and_releases_lease() {
    let repo = InMemoryRecurringProxyMonitorRepository::new();
    let tenant = TenantId::new();
    let mut m = monitor(tenant, UserId::new(), "resched", false);
    m.next_run_at = Some(Utc::now() - Duration::seconds(1));
    let id = m.id;
    repo.save_monitor(&m).await.unwrap();

    let claimed = repo
        .claim_due_monitors(
            Utc::now(),
            "worker-1",
            Utc::now() + Duration::minutes(1),
            1,
        )
        .await
        .unwrap();
    assert_eq!(claimed.len(), 1);

    let now = Utc::now();
    let next = now + Duration::seconds(300);
    let ok = repo
        .reschedule(id, tenant, "worker-1", now, next)
        .await
        .unwrap();
    assert!(ok);

    let loaded = repo.find_by_id(id, tenant).await.unwrap().unwrap();
    assert_eq!(loaded.next_run_at, Some(next));
    assert_eq!(loaded.last_run_at, Some(now));

    // A different worker cannot reschedule after that.
    let other = repo
        .reschedule(id, tenant, "worker-2", now, next)
        .await
        .unwrap();
    assert!(!other);
}

#[tokio::test]
async fn release_lease_without_advancing_schedule_makes_monitor_reclaimable() {
    let repo = InMemoryRecurringProxyMonitorRepository::new();
    let tenant = TenantId::new();
    let mut m = monitor(tenant, UserId::new(), "release", false);
    m.next_run_at = Some(Utc::now() - Duration::seconds(1));
    let id = m.id;
    repo.save_monitor(&m).await.unwrap();

    repo.claim_due_monitors(
        Utc::now(),
        "worker-1",
        Utc::now() + Duration::minutes(1),
        1,
    )
    .await
    .unwrap();

    assert!(repo
        .release_lease(id, tenant, "worker-1")
        .await
        .unwrap());

    // Still due → can be claimed again.
    let reclaimed = repo
        .claim_due_monitors(
            Utc::now(),
            "worker-2",
            Utc::now() + Duration::minutes(1),
            1,
        )
        .await
        .unwrap();
    assert_eq!(reclaimed.len(), 1);
    assert_eq!(reclaimed[0].monitor.id, id);
    assert_eq!(reclaimed[0].worker_id, "worker-2");
}

#[tokio::test]
async fn checks_round_trip_and_are_tenant_scoped() {
    let repo = InMemoryRecurringProxyMonitorRepository::new();
    let tenant = TenantId::new();
    let other_tenant = TenantId::new();
    let m = monitor(tenant, UserId::new(), "checks", false);
    let id = m.id;
    repo.save_monitor(&m).await.unwrap();

    let check = RecurringProxyCheck::new_running(Uuid::new_v4(), id, tenant);
    let check_id = check.id;
    repo.save_check(&check).await.unwrap();

    // Latest for tenant → found.
    let latest = repo.find_latest_check(id, tenant).await.unwrap();
    assert!(latest.is_some());
    assert_eq!(latest.unwrap().status, RecurringProxyCheckStatus::Running);

    // Latest for a different tenant → not found.
    let not_found = repo.find_latest_check(id, other_tenant).await.unwrap();
    assert!(not_found.is_none());

    // Pagination works.
    let (page, total) = repo.list_checks(id, tenant, 0, 10).await.unwrap();
    assert_eq!(total, 1);
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].id, check_id);
}

#[tokio::test]
async fn finish_check_updates_only_terminal_fields() {
    let repo = InMemoryRecurringProxyMonitorRepository::new();
    let tenant = TenantId::new();
    let m = monitor(tenant, UserId::new(), "finish", false);
    let id = m.id;
    repo.save_monitor(&m).await.unwrap();

    let mut check = RecurringProxyCheck::new_running(Uuid::new_v4(), id, tenant);
    let started = check.started_at;
    repo.save_check(&check).await.unwrap();

    check.finish_succeeded(sample_result());
    assert!(repo.finish_check(&check).await.unwrap());

    let loaded = repo.find_latest_check(id, tenant).await.unwrap().unwrap();
    assert_eq!(loaded.status, RecurringProxyCheckStatus::Succeeded);
    assert_eq!(loaded.started_at, started);
    assert!(loaded.result.is_some());
    assert!(loaded.finished_at.is_some());
}

#[tokio::test]
async fn finish_check_on_missing_row_returns_false() {
    let repo = InMemoryRecurringProxyMonitorRepository::new();
    let tenant = TenantId::new();
    let check = RecurringProxyCheck::new_running(Uuid::new_v4(), Uuid::new_v4(), tenant);
    assert!(!repo.finish_check(&check).await.unwrap());
}
