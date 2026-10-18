//! PART 2 — `POST /api/v1/proxy-checks/preview`.
//!
//! A read-only dry run of a proxy-check submission. This handler:
//!   * requires authentication (so unauthenticated callers cannot burn
//!     server CPU on large CSV/JSON inputs),
//!   * accepts `application/json` with a small wrapper:
//!         { "format": "csv" | "json", "content": "..." }
//!   * dispatches to `parse_csv_with_summary` / `parse_json_with_summary`,
//!     which classify every input unit and never abort the batch on a
//!     single bad line,
//!   * returns a summary struct — total / valid / invalid / duplicates /
//!     rejected / accepted,
//!   * does NOT create a job, does NOT enqueue work, does NOT consume
//!     quota.
//!
//! The wrapper shape (rather than `Content-Type: text/csv`) exists so the
//! existing `post_json` frontend helper can be reused unchanged: the
//! frontend always speaks JSON, and the format signal lives inside the
//! body. This also keeps the endpoint's contract unambiguous — there is
//! exactly one request shape.

use axum::{
    Json,
    extract::{Extension, State},
    http::StatusCode,
};
use serde::Deserialize;
use tracing::info;

use application::parsing::proxy_import::{
    ProxyImportSummary, parse_csv_with_summary, parse_json_with_summary,
};
use domain::shared::types::AuthenticatedUser;

use crate::{
    error::ApiError,
    handlers::proxy_check_types::{
        DuplicateProxyHttp, ProxyEntryPreviewHttpResponse, ProxyPreviewHttpResponse,
        RejectedProxyHttp,
    },
    state::AppState,
};

/// Upper bound on how much JSON body the preview endpoint will accept
/// before dispatching to the parser. Matches `MAX_JSON_BYTES` in the
/// parser, plus a small allowance for the JSON wrapper envelope. Guards
/// against a caller streaming gigabytes into the server.
const MAX_PREVIEW_BODY_BYTES: usize = 1_100_000;

#[derive(Debug, Deserialize)]
pub struct PreviewRequest {
    /// Either `"csv"` or `"json"`. Anything else is a 400.
    pub format: String,
    /// The raw user input, exactly as it appeared in the textarea or
    /// uploaded file.
    pub content: String,
}

pub async fn preview_proxy_check_job(
    State(_state): State<AppState>,
    Extension(_authenticated_user): Extension<AuthenticatedUser>,
    body: String,
) -> Result<(StatusCode, Json<ProxyPreviewHttpResponse>), ApiError> {
    info!("API: preview_proxy_check_job");

    if body.len() > MAX_PREVIEW_BODY_BYTES {
        return Err(ApiError::bad_request(format!(
            "request body size {} exceeds maximum allowed {} bytes",
            body.len(),
            MAX_PREVIEW_BODY_BYTES
        )));
    }

    let request: PreviewRequest = serde_json::from_str(&body)
        .map_err(|err| ApiError::bad_request(format!("invalid preview request body: {err}")))?;

    let summary = match request.format.as_str() {
        "csv" => parse_csv_with_summary(&request.content),
        "json" => parse_json_with_summary(&request.content),
        other => {
            return Err(ApiError::bad_request(format!(
                "unsupported preview format '{other}'; expected 'csv' or 'json'"
            )));
        }
    }
    .map_err(ApiError::bad_request)?;

    Ok((
        StatusCode::OK,
        Json(to_http_response(summary)),
    ))
}

fn to_http_response(summary: ProxyImportSummary) -> ProxyPreviewHttpResponse {
    ProxyPreviewHttpResponse {
        total: summary.total,
        valid: summary.valid_count(),
        invalid: summary.invalid,
        duplicate_count: summary.duplicate_count(),
        rejected_count: summary.rejected_count(),
        accepted: summary
            .accepted
            .into_iter()
            .map(|accepted| ProxyEntryPreviewHttpResponse {
                position: accepted.position,
                host: accepted.input.host,
                port: accepted.input.port,
                scheme: accepted.input.scheme,
                has_credentials: accepted.input.username.is_some()
                    || accepted.input.password.is_some(),
            })
            .collect(),
        rejected: summary
            .rejected
            .into_iter()
            .map(|r| RejectedProxyHttp {
                position: r.position,
                raw: r.raw,
                reason: r.reason,
            })
            .collect(),
        duplicates: summary
            .duplicates
            .into_iter()
            .map(|d| DuplicateProxyHttp {
                position: d.position,
                raw: d.raw,
                first_seen_at: d.first_seen_at,
            })
            .collect(),
    }
}
