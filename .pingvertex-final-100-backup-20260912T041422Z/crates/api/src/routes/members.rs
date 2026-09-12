use axum::{
    Router,
    routing::{delete, get, put},
};

use crate::{
    auth_state::AuthState,
    handlers::{add_member, change_member_role, list_members, remove_member},
};

/// Tenant membership management routes, authenticated via `jwt_auth` and
/// authorized in each use case (`ManageUsers` permission + role rules).
pub fn router() -> Router<AuthState> {
    Router::new()
        .route("/members", get(list_members).post(add_member))
        .route("/members/{user_id}", delete(remove_member))
        .route("/members/{user_id}/role", put(change_member_role))
}
