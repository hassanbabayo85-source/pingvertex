//! SSRF protection now lives in `shared::ssrf` so both this worker crate
//! and `crates/infra`'s webhook notification service can reuse the exact
//! same private-IP / DNS-rebinding checks instead of duplicating them.
//!
//! Re-exported unchanged under the original path so existing call sites
//! (`crate::ssrf::resolve_and_validate_destination`, `crate::ssrf::is_private_ip`,
//! etc.) and the existing test suite keep working with zero changes. Every
//! function, type, and test that used to live in this file now lives in
//! `crates/shared/src/ssrf.rs` verbatim — nothing was removed or weakened.
pub use shared::ssrf::*;
