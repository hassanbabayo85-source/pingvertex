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
        .route("/wallet/top-up", post(crate::handlers::wallet::top_up))
        .route(
            "/wallet/top-up/:provider_checkout_id/verify",
            post(crate::handlers::wallet::verify_wallet_top_up),
        )
        .route(
            "/wallet/settings",
            get(crate::handlers::wallet::get_settings)
                .put(crate::handlers::wallet::update_settings),
        )
        .route(
            "/wallet/refund",
            post(crate::handlers::wallet::request_refund),
        )
        .route("/wallet/usage", get(crate::handlers::wallet::usage_report))
}
