use axum::{
    Json,
    extract::{Extension, Multipart, Path, State},
    http::HeaderMap,
};
use tracing::info;

use application::{
    commands::UploadStatusPageLogo,
    use_cases::{UploadStatusPageLogoError, UploadStatusPageLogoUseCase},
};
use domain::shared::types::AuthenticatedUser;

use crate::{
    error::ApiError, handlers::types::StatusPageResponse, state::AppState,
    validation::parse_status_page_id,
};

pub async fn upload_status_page_logo(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    headers: HeaderMap,
    Path(id): Path<String>,
    mut multipart: Multipart,
) -> Result<Json<StatusPageResponse>, ApiError> {
    info!("API: upload_status_page_logo");

    let id = parse_status_page_id(&id)?;

    let field = multipart
        .next_field()
        .await
        .map_err(|_| ApiError::bad_request("invalid multipart upload"))?
        .ok_or_else(|| ApiError::bad_request("logo file is required"))?;

    if field.name() != Some("logo") {
        return Err(ApiError::bad_request("upload field must be named \"logo\""));
    }

    let data = field
        .bytes()
        .await
        .map_err(|_| ApiError::bad_request("invalid or too large logo upload"))?;

    // GAP 3: logo upload is free — no PAYG metering.
    let use_case = UploadStatusPageLogoUseCase::new(
        state.status_page_repository.clone(),
        state.logo_storage.clone(),
        state.audit_log_entry_repository.clone(),
    );

    let request_id = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 128
                && value.is_ascii()
                && value.bytes().all(|b| b.is_ascii_graphic())
        })
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("status-page-logo:{}:{}", id.0, uuid::Uuid::new_v4()));

    let status_page = use_case
        .execute_with_request_id(
            UploadStatusPageLogo {
                id,
                data: data.to_vec(),
            },
            authenticated_user,
            request_id,
        )
        .await
        .map_err(|err| match err {
            UploadStatusPageLogoError::Forbidden => {
                ApiError::forbidden("you do not have permission to update status pages")
            }
            UploadStatusPageLogoError::InvalidLogo(domain_err) => {
                ApiError::bad_request(domain_err.to_string())
            }
            UploadStatusPageLogoError::Other(inner) => {
                ApiError::internal_with_source("failed to upload logo", inner)
            }
        })?
        .ok_or_else(|| ApiError::not_found("status page not found"))?;

    Ok(Json(StatusPageResponse::from(status_page)))
}
