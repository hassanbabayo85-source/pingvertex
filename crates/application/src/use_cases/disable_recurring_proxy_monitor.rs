//! Convenience wrapper: `disable` is `enable(false)`. Kept as its own
//! module so route handlers can import the correct intent by name
//! instead of remembering a boolean flag.

pub use super::enable_recurring_proxy_monitor::{SetEnabledError, SetEnabledUseCase};

pub type DisableRecurringProxyMonitorUseCase<R> = SetEnabledUseCase<R>;
pub type DisableRecurringProxyMonitorError = SetEnabledError;
