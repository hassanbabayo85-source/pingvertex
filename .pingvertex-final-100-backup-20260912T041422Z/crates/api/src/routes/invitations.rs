use axum::{
    Router,
    routing::{delete, get, post},
};

use crate::{
    auth_state::AuthState,
    handlers::{accept_invitation, create_invitation, list_invitations, revoke_invitation},
};

/// Tenant invitation routes, authenticated via `jwt_auth`.
pub fn router() -> Router<AuthState> {
    Router::new()
        .route("/invitations", post(create_invitation))
        .route("/invitations/pending", get(list_invitations))
        .route("/invitations/accept", post(accept_invitation))
        .route("/invitations/{id}", delete(revoke_invitation))
}
