use axum::{
    extract::{Extension, Multipart, Path, State},
    Json,
};
use tracing::info;

use application::{
    commands::UploadStatusPageLogo,
    use_cases::{UploadStatusPageLogoError, UploadStatusPageLogoUseCase},
};
use domain::shared::ids::UserId;

use crate::{
    error::ApiError, handlers::types::StatusPageResponse, state::AppState,
    validation::parse_status_page_id,
};

pub async fn upload_status_page_logo(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
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

    let use_case = UploadStatusPageLogoUseCase::new(
        state.status_page_repository.clone(),
        state.logo_storage.clone(),
    );

    let status_page = use_case
        .execute(UploadStatusPageLogo {
            id,
            user_id,
            data: data.to_vec(),
        })
        .await
        .map_err(|err| match err {
            // The client's fault (wrong file type, too large, empty,
            // corrupt) — say so plainly instead of a generic 500. Uses
            // DomainError's own `#[error("...")]` message, which is
            // already written to be a safe, specific, user-facing string
            // (e.g. "logo image exceeds the maximum size of 500KB") —
            // unlike a raw storage/DB error, there's nothing internal to
            // hide here.
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
