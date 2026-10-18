//! Application-layer error types that are shared across use cases.
//!
//! These are *not* per-use-case enums (like `CreateMonitorError`);
//! those stay inline. This module hosts errors that surface from
//! cross-cutting services (today: `PlanLimitError` from the
//! entitlement resolver).

pub mod plan_limit;
pub use plan_limit::PlanLimitError;
