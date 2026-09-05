use crate::state::AppState;
use axum::{Router, routing::get};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/wallet", get(crate::handlers::wallet::get_wallet))
        .route(
            "/wallet/transactions",
            get(crate::handlers::wallet::list_wallet_transactions),
        )
}
