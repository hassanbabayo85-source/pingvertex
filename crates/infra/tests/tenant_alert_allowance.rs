//! GAP 3 Kashi D: repository acceptance tests for `TenantAlertAllowance`.
//!
//! Covered:
//!   1. `current_cycle` creates a zero-state row for a new tenant.
//!   2. `current_cycle` lazy-resets a stale cycle and recomputes `sms_limit`.
//!   3. `try_consume_sms` succeeds until the limit, then fails.
//!   4. `try_consume_sms` fails when no row exists.
//!   5. `find` returns None before any cycle is created.

use application::ports::TenantAlertAllowanceRepository;
use chrono::{Datelike, Utc};
use domain::shared::ids::TenantId;
use infra::InMemoryTenantAlertAllowanceRepository;

#[tokio::test]
async fn current_cycle_creates_row_with_computed_limit() {
    let repo = InMemoryTenantAlertAllowanceRepository::new();
    let tenant = TenantId::new();
    let now = Utc::now();

    let a = repo.current_cycle(tenant, 1_000, now).await.unwrap();
    assert_eq!(a.sms_limit, 250);
    assert_eq!(a.sms_used, 0);
    assert_eq!(a.cycle_started_at.day(), 1);
}

#[tokio::test]
async fn current_cycle_is_idempotent_within_same_month() {
    let repo = InMemoryTenantAlertAllowanceRepository::new();
    let tenant = TenantId::new();
    let now = Utc::now();

    let a1 = repo.current_cycle(tenant, 1_000, now).await.unwrap();
    // Second call same month with a different monitor count must NOT
    // change the frozen limit.
    let a2 = repo.current_cycle(tenant, 5_000, now).await.unwrap();
    assert_eq!(a1.sms_limit, a2.sms_limit);
    assert_eq!(a2.sms_limit, 250);
}

#[tokio::test]
async fn current_cycle_lazy_resets_stale_cycle() {
    let repo = InMemoryTenantAlertAllowanceRepository::new();
    let tenant = TenantId::new();

    // Simulate a tenant whose stored cycle is 60 days old.
    let stale = Utc::now() - chrono::Duration::days(60);
    let a_old = repo.current_cycle(tenant, 1_000, stale).await.unwrap();
    assert_eq!(a_old.sms_limit, 250);

    // Consume some SMS in the old cycle.
    assert!(repo.try_consume_sms(tenant, stale).await.unwrap());
    assert!(repo.try_consume_sms(tenant, stale).await.unwrap());
    let mid = repo.find(tenant).await.unwrap().unwrap();
    assert_eq!(mid.sms_used, 2);

    // Now a request arrives in the new cycle.
    let fresh = Utc::now();
    let a_new = repo.current_cycle(tenant, 500, fresh).await.unwrap();

    // Counters reset, and `sms_limit` is recomputed from the new monitor count.
    assert_eq!(a_new.sms_used, 0);
    assert_eq!(a_new.sms_limit, 125); // ceil(500 * 250 / 1000) = 125
    assert!(a_new.cycle_started_at > mid.cycle_started_at);
}

#[tokio::test]
async fn try_consume_sms_succeeds_until_limit_then_fails() {
    let repo = InMemoryTenantAlertAllowanceRepository::new();
    let tenant = TenantId::new();
    let now = Utc::now();

    // 40 enabled monitors → ceil(40 * 250 / 1000) = 10 SMS.
    repo.current_cycle(tenant, 40, now).await.unwrap();

    for i in 0..10 {
        assert!(
            repo.try_consume_sms(tenant, now).await.unwrap(),
            "consume #{} should succeed",
            i + 1
        );
    }
    assert!(
        !repo.try_consume_sms(tenant, now).await.unwrap(),
        "the 11th consume must fail"
    );

    let a = repo.find(tenant).await.unwrap().unwrap();
    assert_eq!(a.sms_used, 10);
}

#[tokio::test]
async fn try_consume_sms_fails_when_no_row_exists() {
    let repo = InMemoryTenantAlertAllowanceRepository::new();
    let tenant = TenantId::new();
    assert!(!repo.try_consume_sms(tenant, Utc::now()).await.unwrap());
}

#[tokio::test]
async fn find_returns_none_before_first_cycle() {
    let repo = InMemoryTenantAlertAllowanceRepository::new();
    assert!(repo.find(TenantId::new()).await.unwrap().is_none());
}

#[tokio::test]
async fn try_consume_sms_fails_outside_cycle_window() {
    let repo = InMemoryTenantAlertAllowanceRepository::new();
    let tenant = TenantId::new();
    let now = Utc::now();
    repo.current_cycle(tenant, 40, now).await.unwrap();

    // Far future: outside the current cycle window.
    let future = now + chrono::Duration::days(60);
    assert!(!repo.try_consume_sms(tenant, future).await.unwrap());
}
