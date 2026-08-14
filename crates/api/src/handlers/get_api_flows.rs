use axum::{
    extract::{Extension, Path, State},
    Json,
};
use tracing::info;

use domain::shared::ids::UserId;

use crate::{
    error::ApiError, handlers::flow_types::ApiFlowHttpResponse, state::AppState,
    validation::parse_monitor_id,
};

pub async fn get_api_flows(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<String>,
) -> Result<Json<Vec<ApiFlowHttpResponse>>, ApiError> {
    info!("API: get_api_flows");

    let monitor_id = parse_monitor_id(&id)?;

    // Authorization check: the monitor must belong to the authenticated user.
    state
        .monitor_repository
        .find_by_id_for_user(monitor_id, user_id)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to fetch monitor", err))?
        .ok_or_else(|| ApiError::not_found("monitor not found"))?;

    let flow = state
        .api_flow_repository
        .find_by_monitor_id(monitor_id)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to fetch api flow", err))?;

    Ok(Json(
        flow.into_iter().map(ApiFlowHttpResponse::from).collect(),
    ))
}
