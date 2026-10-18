//! Deterministic execution-budget math.
//!
//! These pure functions answer: "if a tenant uses exactly this many
//! resources at exactly these intervals, how many network executions
//! does that produce per 30-day month?"
//!
//! They are deliberately separated from `Entitlements`:
//!   * `Entitlements` says **what a plan allows**;
//!   * `execution_budget` says **what that allowance actually costs**
//!     in executions.
//!
//! TASK 2 requires both, so the quota enforcer can reject updates that
//! would exceed a plan's execution budget even when the resource count
//! itself is under the numeric quota (e.g. shortening intervals).
//!
//! The reference figure: 1 monitor at 300s for 30 days = 8,640 checks.

/// Seconds in a nominal 30-day month.
pub const SECONDS_PER_MONTH: u64 = 30 * 24 * 60 * 60;

/// Number of executions one monitor at `interval_secs` produces in a
/// 30-day month.
pub fn executions_per_month_for_interval(interval_secs: u32) -> u64 {
    if interval_secs == 0 {
        return 0;
    }
    SECONDS_PER_MONTH / u64::from(interval_secs)
}

/// Uptime (HTTP/TCP/DNS) executions per month.
/// = monitors × regions × (seconds_in_month / interval)
pub fn uptime_executions_per_month(monitors: u32, interval_secs: u32, regions: u32) -> u64 {
    u64::from(monitors)
        .saturating_mul(u64::from(regions))
        .saturating_mul(executions_per_month_for_interval(interval_secs))
}

/// Recurring proxy executions per month.
/// = monitors × (seconds_in_month / interval)
pub fn recurring_proxy_executions_per_month(monitors: u32, interval_secs: u32) -> u64 {
    u64::from(monitors).saturating_mul(executions_per_month_for_interval(interval_secs))
}

/// API Flow executions per month.
/// = monitors × steps × (seconds_in_month / interval)
pub fn api_flow_executions_per_month(monitors: u32, steps: u32, interval_secs: u32) -> u64 {
    u64::from(monitors)
        .saturating_mul(u64::from(steps))
        .saturating_mul(executions_per_month_for_interval(interval_secs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_zero_yields_zero_executions() {
        assert_eq!(executions_per_month_for_interval(0), 0);
        assert_eq!(uptime_executions_per_month(1, 0, 1), 0);
    }

    #[test]
    fn one_monitor_at_300s_is_exactly_8640() {
        // 30 days × 24 h × 60 min × 60 s / 300 = 8640
        assert_eq!(uptime_executions_per_month(1, 300, 1), 8_640);
        assert_eq!(recurring_proxy_executions_per_month(1, 300), 8_640);
    }

    #[test]
    fn five_monitors_at_300s_is_43200() {
        assert_eq!(recurring_proxy_executions_per_month(5, 300), 43_200);
    }

    #[test]
    fn ten_monitors_at_300s_is_86400() {
        assert_eq!(recurring_proxy_executions_per_month(10, 300), 86_400);
    }

    #[test]
    fn uptime_regions_multiply() {
        // 1 monitor × 3 regions × 8640 = 25920
        assert_eq!(uptime_executions_per_month(1, 300, 3), 25_920);
    }

    #[test]
    fn api_flow_steps_multiply() {
        // 10 flows × 10 steps × (8640 at 300s) = 864000
        assert_eq!(api_flow_executions_per_month(10, 10, 300), 864_000);
    }

    #[test]
    fn at_60s_interval_one_monitor_is_43200() {
        // 30 days / 60s = 43200
        assert_eq!(uptime_executions_per_month(1, 60, 1), 43_200);
    }

    #[test]
    fn overflow_saturates() {
        // Defensive: absurd inputs must not panic.
        let _ = uptime_executions_per_month(u32::MAX, 1, u32::MAX);
    }
}
