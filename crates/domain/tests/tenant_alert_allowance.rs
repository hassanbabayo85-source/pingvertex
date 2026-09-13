//! GAP 3 Kashi D: unit tests for the fair-use SMS allowance formula.
//!
//! Baseline from the blueprint: 1,000 enabled monitors = 250 SMS per
//! monthly cycle. The formula is `ceil(enabled * 250 / 1000)`.

use domain::entities::TenantAlertAllowance;

#[test]
fn zero_monitors_yields_zero_sms() {
    assert_eq!(TenantAlertAllowance::compute_sms_limit(0), 0);
}

#[test]
fn single_monitor_yields_one_sms() {
    // ceil(1 * 250 / 1000) = ceil(0.25) = 1
    assert_eq!(TenantAlertAllowance::compute_sms_limit(1), 1);
}

#[test]
fn ten_monitors_yields_three_sms() {
    // ceil(10 * 250 / 1000) = ceil(2.5) = 3
    assert_eq!(TenantAlertAllowance::compute_sms_limit(10), 3);
}

#[test]
fn one_thousand_monitors_yields_baseline() {
    assert_eq!(TenantAlertAllowance::compute_sms_limit(1_000), 250);
}

#[test]
fn one_thousand_and_one_monitors_yields_one_extra() {
    // ceil(1001 * 250 / 1000) = ceil(250.25) = 251
    assert_eq!(TenantAlertAllowance::compute_sms_limit(1_001), 251);
}

#[test]
fn four_monitors_yields_one_sms() {
    // ceil(4 * 250 / 1000) = ceil(1.0) = 1
    assert_eq!(TenantAlertAllowance::compute_sms_limit(4), 1);
}

#[test]
fn has_sms_remaining_true_below_limit() {
    let mut a = allowance(3, 0);
    assert!(a.has_sms_remaining());
    a.sms_used = 2;
    assert!(a.has_sms_remaining());
}

#[test]
fn has_sms_remaining_false_at_limit() {
    let a = allowance(3, 3);
    assert!(!a.has_sms_remaining());
    assert_eq!(a.sms_remaining(), 0);
}

#[test]
fn sms_remaining_is_saturating() {
    // Even if sms_used somehow exceeds sms_limit, remaining clamps to 0.
    let a = allowance(3, 10);
    assert_eq!(a.sms_remaining(), 0);
}

fn allowance(limit: i32, used: i32) -> TenantAlertAllowance {
    let now = chrono::Utc::now();
    TenantAlertAllowance {
        tenant_id: domain::shared::ids::TenantId::new(),
        cycle_started_at: now,
        cycle_ends_at: now + chrono::Duration::days(30),
        sms_limit: limit,
        sms_used: used,
        voice_limit: 0,
        voice_used: 0,
        phone_numbers_limit: 0,
        phone_numbers_used: 0,
        created_at: now,
        updated_at: now,
    }
}
