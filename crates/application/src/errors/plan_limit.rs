//! Structured hard-quota rejection.
//!
//! TASK 2 requires every quota-exceeded path to return a consistent,
//! machine-readable error carrying:
//!   * the quota that was exceeded,
//!   * the tenant's current usage,
//!   * the plan's allowed maximum,
//!   * a suggested corrective action.
//!
//! Unlike `PaymentRequired`, this is **not** a "pay more" error — it is
//! a hard subscription limit. The customer cannot pay their way past it
//! without changing plans. That distinction is why we return a distinct
//! error type and a distinct code (`plan_limit_reached`), not
//! `PAYG_INSUFFICIENT_BALANCE` or similar.
//!
//! # Why there is no explicit `From<PlanLimitError> for Box<dyn Error>`
//!
//! `PlanLimitError: std::error::Error + Send + Sync + 'static`, so the
//! standard library's blanket impl
//!
//! ```ignore
//! impl<E: Error + Send + Sync + 'static> From<E> for Box<dyn Error + Send + Sync>
//! ```
//!
//! already covers the conversion. Writing it by hand collides with that
//! blanket impl (E0119). The `?` operator works without any additional
//! impl because of the blanket impl.

/// A hard subscription quota that the current operation would exceed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanLimitError {
    /// Machine-readable quota identifier, e.g. `"max_uptime_monitors"`.
    /// Always one of the `domain::entities::plan_entitlements::*`
    /// constants.
    pub quota: &'static str,
    /// The tenant's usage count *before* the rejected operation.
    pub current: u64,
    /// The plan's allowed maximum.
    pub allowed: u64,
    /// Suggested corrective action, e.g. `"upgrade_plan"` or
    /// `"delete_unused_monitors"`.
    pub action: &'static str,
}

impl PlanLimitError {
    pub fn new(
        quota: &'static str,
        current: u64,
        allowed: u64,
        action: &'static str,
    ) -> Self {
        Self {
            quota,
            current,
            allowed,
            action,
        }
    }

    /// Stable machine-readable error code. Matches the format mandated
    /// by TASK 2 so a client can branch on it without parsing prose.
    pub fn code(&self) -> &'static str {
        "plan_limit_reached"
    }
}

impl std::fmt::Display for PlanLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "plan limit reached: quota '{}' allows at most {} (current {}). Suggested action: {}",
            self.quota, self.allowed, self.current, self.action
        )
    }
}

impl std::error::Error for PlanLimitError {}

/// Convenience helper that boxes a `PlanLimitError` into the boxed
/// error type most `RepoResult<T>` call sites use.
pub fn plan_limit_err(
    quota: &'static str,
    current: u64,
    allowed: u64,
    action: &'static str,
) -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(PlanLimitError::new(quota, current, allowed, action))
}

/// Returns `true` iff `err` (or anything it wraps via `source()`) is a
/// `PlanLimitError`. Used by callers that need to branch on the
/// structured fields without a full pattern-match across the boxed
/// dynamic error.
pub fn is_plan_limit_error(err: &(dyn std::error::Error + 'static)) -> bool {
    if err.downcast_ref::<PlanLimitError>().is_some() {
        return true;
    }
    let mut source = err.source();
    while let Some(s) = source {
        if s.downcast_ref::<PlanLimitError>().is_some() {
            return true;
        }
        source = s.source();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::RepoResult;

    #[test]
    fn code_is_plan_limit_reached() {
        let e = PlanLimitError::new("max_uptime_monitors", 20, 20, "upgrade_plan");
        assert_eq!(e.code(), "plan_limit_reached");
    }

    #[test]
    fn display_includes_quota_current_allowed_action() {
        let e = PlanLimitError::new("max_uptime_monitors", 20, 20, "upgrade_plan");
        let s = e.to_string();
        assert!(s.contains("max_uptime_monitors"), "got: {s}");
        assert!(s.contains("20"), "got: {s}");
        assert!(s.contains("upgrade_plan"), "got: {s}");
    }

    #[test]
    fn boxed_error_can_be_downcast_back() {
        let boxed: Box<dyn std::error::Error + Send + Sync> =
            Box::new(PlanLimitError::new("max_uptime_monitors", 5, 5, "upgrade_plan"));
        let recovered = boxed.downcast_ref::<PlanLimitError>().unwrap();
        assert_eq!(recovered.quota, "max_uptime_monitors");
    }

    #[test]
    fn plan_limit_err_helper_returns_boxed_plan_limit() {
        let boxed = plan_limit_err("max_rum_sites", 0, 0, "upgrade_plan");
        assert!(is_plan_limit_error(boxed.as_ref()));
    }

    #[test]
    fn blanket_from_impl_via_question_mark_works() {
        // Verifies the standard-library blanket impl is enough: no
        // manual From<PlanLimitError> impl is needed for `?` to work.
        fn try_op() -> RepoResult<()> {
            let e = PlanLimitError::new("max_uptime_monitors", 1, 1, "upgrade_plan");
            Err(e)?
        }
        assert!(try_op().is_err());
    }

    #[test]
    fn is_plan_limit_error_walks_source_chain() {
        #[derive(Debug)]
        struct Wrapper(PlanLimitError);
        impl std::fmt::Display for Wrapper {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "wrapper: {}", self.0)
            }
        }
        impl std::error::Error for Wrapper {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }

        let w = Wrapper(PlanLimitError::new("max_uptime_monitors", 1, 1, "upgrade_plan"));
        assert!(is_plan_limit_error(&w));
    }
}
