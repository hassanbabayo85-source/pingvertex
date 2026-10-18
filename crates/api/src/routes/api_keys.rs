use axum::{
    Router,
    middleware::from_fn,
    routing::{delete, get, post, put},
};

use domain::enums::ApiKeyScope;

use crate::{
    handlers::{create_api_key, delete_api_key, list_api_keys},
    middleware::api_key_auth::require_api_key_scope,
    state::AppState,
};

/// Management routes for API keys themselves. Authenticated the normal
/// way (session/JWT via `jwt_auth`, wired in `router.rs`) — minting or
/// revoking a key is a tenant-management action, not something an API
/// key can do to itself.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api-keys", post(create_api_key).get(list_api_keys))
        .route("/api-keys/{id}", delete(delete_api_key))
}

/// The public API surface authenticated via `X-API-Key` instead of a
/// session. Kept on its own `Router` so `router.rs` can layer
/// `api_key_auth` (authentication only) once for the whole group.
///
/// Unlike the original single-endpoint shape of this router (just
/// `/external/monitors`, gated on a single blanket `MonitorRead` scope
/// applied in `router.rs`), each resource below requires a DIFFERENT
/// scope — e.g. `IncidentRead` to list incidents vs `IncidentWrite` to
/// acknowledge one. So scope enforcement now lives HERE, one
/// `require_api_key_scope` layer per resource sub-router, all merged
/// together. `api_key_auth` (layered in `router.rs`, outside this
/// router) MUST run first so `ApiKeyScopes` is already in request
/// extensions by the time any of these `route_layer`s run.
pub fn external_router() -> Router<AppState> {
    use crate::handlers::{
        acknowledge_incident_via_api_key, list_checks_via_api_key, list_incidents_via_api_key,
        list_monitors_via_api_key, list_status_pages_via_api_key, update_status_page_via_api_key,
    };

    let monitors = Router::new()
        .route("/external/monitors", get(list_monitors_via_api_key))
        .route_layer(from_fn(move |req, next| {
            require_api_key_scope(ApiKeyScope::MonitorRead, req, next)
        }));

    // No dedicated `check:*` scope exists on `ApiKeyScope` — see the doc
    // comment on `handlers::external_checks::list_checks_via_api_key`.
    let checks = Router::new()
        .route("/external/checks", get(list_checks_via_api_key))
        .route_layer(from_fn(move |req, next| {
            require_api_key_scope(ApiKeyScope::MonitorRead, req, next)
        }));

    let incidents_read = Router::new()
        .route("/external/incidents/{id}", get(list_incidents_via_api_key))
        .route_layer(from_fn(move |req, next| {
            require_api_key_scope(ApiKeyScope::IncidentRead, req, next)
        }));

    let incidents_write = Router::new()
        .route(
            "/external/incidents/{incident_id}/acknowledge",
            post(acknowledge_incident_via_api_key),
        )
        .route_layer(from_fn(move |req, next| {
            require_api_key_scope(ApiKeyScope::IncidentWrite, req, next)
        }));

    let status_pages_read = Router::new()
        .route("/external/status-pages", get(list_status_pages_via_api_key))
        .route_layer(from_fn(move |req, next| {
            require_api_key_scope(ApiKeyScope::StatusPageRead, req, next)
        }));

    let status_pages_write = Router::new()
        .route(
            "/external/status-pages/{id}",
            put(update_status_page_via_api_key),
        )
        .route_layer(from_fn(move |req, next| {
            require_api_key_scope(ApiKeyScope::StatusPageWrite, req, next)
        }));

    monitors
        .merge(checks)
        .merge(incidents_read)
        .merge(incidents_write)
        .merge(status_pages_read)
        .merge(status_pages_write)
}
