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
//! error type and a distinct code (`resource_limit_reached`), not
//! `PAYG_INSUFFICIENT_BALANCE` or similar.
//!
//! # Why there is no explicit `From<ResourceLimitError> for Box<dyn Error>`
//!
//! `ResourceLimitError: std::error::Error + Send + Sync + 'static`, so the
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
pub struct ResourceLimitError {
    /// Machine-readable quota identifier, e.g. `"max_uptime_monitors"`.
    /// Always one of the `domain::entities::plan_entitlements::*`
    /// constants.
    pub quota: &'static str,
    /// The tenant's usage count *before* the rejected operation.
    pub current: u64,
    /// The plan's allowed maximum.
    pub allowed: u64,
    /// Suggested corrective action, e.g. `"increase_resource_limit"` or
    /// `"delete_unused_monitors"`.
    pub action: &'static str,
}

impl ResourceLimitError {
    pub fn new(quota: &'static str, current: u64, allowed: u64, action: &'static str) -> Self {
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
        "resource_limit_reached"
    }
}

impl std::fmt::Display for ResourceLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "plan limit reached: quota '{}' allows at most {} (current {}). Suggested action: {}",
            self.quota, self.allowed, self.current, self.action
        )
    }
}

impl std::error::Error for ResourceLimitError {}

/// Convenience helper that boxes a `ResourceLimitError` into the boxed
/// error type most `RepoResult<T>` call sites use.
pub fn resource_limit_err(
    quota: &'static str,
    current: u64,
    allowed: u64,
    action: &'static str,
) -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(ResourceLimitError::new(quota, current, allowed, action))
}

/// Returns `true` iff `err` (or anything it wraps via `source()`) is a
/// `ResourceLimitError`. Used by callers that need to branch on the
/// structured fields without a full pattern-match across the boxed
/// dynamic error.
pub fn is_resource_limit_error(err: &(dyn std::error::Error + 'static)) -> bool {
    if err.downcast_ref::<ResourceLimitError>().is_some() {
        return true;
    }
    let mut source = err.source();
    while let Some(s) = source {
        if s.downcast_ref::<ResourceLimitError>().is_some() {
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
    fn code_is_resource_limit_reached() {
        let e = ResourceLimitError::new("max_uptime_monitors", 20, 20, "increase_resource_limit");
        assert_eq!(e.code(), "resource_limit_reached");
    }

    #[test]
    fn display_includes_quota_current_allowed_action() {
        let e = ResourceLimitError::new("max_uptime_monitors", 20, 20, "increase_resource_limit");
        let s = e.to_string();
        assert!(s.contains("max_uptime_monitors"), "got: {s}");
        assert!(s.contains("20"), "got: {s}");
        assert!(s.contains("increase_resource_limit"), "got: {s}");
    }

    #[test]
    fn boxed_error_can_be_downcast_back() {
        let boxed: Box<dyn std::error::Error + Send + Sync> = Box::new(ResourceLimitError::new(
            "max_uptime_monitors",
            5,
            5,
            "increase_resource_limit",
        ));
        let recovered = boxed.downcast_ref::<ResourceLimitError>().unwrap();
        assert_eq!(recovered.quota, "max_uptime_monitors");
    }

    #[test]
    fn plan_limit_err_helper_returns_boxed_plan_limit() {
        let boxed = resource_limit_err("max_rum_sites", 0, 0, "increase_resource_limit");
        assert!(is_resource_limit_error(boxed.as_ref()));
    }

    #[test]
    fn blanket_from_impl_via_question_mark_works() {
        // Verifies the standard-library blanket impl is enough: no
        // manual From<ResourceLimitError> impl is needed for `?` to work.
        fn try_op() -> RepoResult<()> {
            let e = ResourceLimitError::new("max_uptime_monitors", 1, 1, "increase_resource_limit");
            Err(e)?
        }
        assert!(try_op().is_err());
    }

    #[test]
    fn is_plan_limit_error_walks_source_chain() {
        #[derive(Debug)]
        struct Wrapper(ResourceLimitError);
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

        let w = Wrapper(ResourceLimitError::new(
            "max_uptime_monitors",
            1,
            1,
            "increase_resource_limit",
        ));
        assert!(is_resource_limit_error(&w));
    }
}
