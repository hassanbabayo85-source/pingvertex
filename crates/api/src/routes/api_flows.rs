use axum::{
    Router,
    routing::{post, put},
};

use crate::{
    handlers::{create_api_flow, delete_api_flow, get_api_flows, update_api_flow},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/monitors/{id}/flows",
            post(create_api_flow).get(get_api_flows),
        )
        .route(
            "/monitors/{id}/flows/{flow_id}",
            put(update_api_flow).delete(delete_api_flow),
        )
}
