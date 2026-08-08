use axum::{routing::post, Router};

use crate::handlers::{login::login, register::register};

pub fn router() -> Router<crate::auth_state::AuthState> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
}
