//! Account-level read helpers for the Security page.
//!
//! NOTE for future Paths / the orchestrator: as of this pass, the backend
//! has no dedicated "account profile" endpoints (no `GET/PUT /account`).
//! Every account fact the frontend can currently read — email, username,
//! `email_verified`, role — comes from `GET /auth/me` (wrapped by
//! `auth::api::fetch_current_user`). This module exists as the place to
//! grow real account endpoints into later (per Path 7's file ownership),
//! and for now re-exports the session fetch under an account-flavored name
//! so `pages/security.rs` doesn't reach into `auth::api` directly for
//! profile data. Do not add fields here that the backend doesn't return —
//! see the confirmed backend gap below.
//!
pub use crate::auth::context::User as AccountProfile;

/// Fetches the current user's profile via the existing session endpoint.
/// See the module-level doc comment for why this is a thin re-export
/// rather than a dedicated `/account` call.
pub async fn fetch_account_profile() -> Result<AccountProfile, String> {
    crate::auth::api::fetch_current_user().await
}
