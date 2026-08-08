use axum::http::StatusCode;

/// Health check endpoint.
pub async fn health() -> StatusCode {
    StatusCode::OK
}
