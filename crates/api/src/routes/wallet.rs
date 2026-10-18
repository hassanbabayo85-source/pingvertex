use crate::state::AppState;
use axum::{
    Router,
    routing::{get, post},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/wallet", get(crate::handlers::wallet::get_wallet))
        .route(
            "/wallet/transactions",
            get(crate::handlers::wallet::list_wallet_transactions),
        )
        .route(
            "/wallet/notifications",
            get(crate::handlers::wallet::notifications),
        )
        .route("/wallet/top-up", post(crate::handlers::payg_retired::retired))
        .route(
            "/wallet/top-up/{provider_checkout_id}/verify",
            post(crate::handlers::payg_retired::retired),
        )
        .route(
            "/wallet/settings",
            get(crate::handlers::wallet::get_settings)
                .put(crate::handlers::payg_retired::retired),
        )
        .route(
            "/wallet/refund",
            post(crate::handlers::payg_retired::retired),
        )
        .route("/wallet/usage", get(crate::handlers::payg_retired::retired))
}
