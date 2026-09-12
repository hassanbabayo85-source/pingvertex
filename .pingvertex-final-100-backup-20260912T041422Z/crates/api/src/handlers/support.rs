use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
};

use application::use_cases::{
    CreateSupportTicketError, CreateSupportTicketUseCase, GetSupportTicketUseCase,
    ListMySupportTicketsUseCase, ReplyToSupportTicketUseCase, UpdateSupportTicketStatusUseCase,
};
use domain::enums::{SupportCategory, SupportTicketStatus};
use domain::shared::types::AuthenticatedUser;

use crate::{
    error::ApiError,
    handlers::support_types::{
        CreateSupportTicketRequest, ReplyToSupportTicketRequest, SupportMessageResponse,
        SupportTicketResponse, UpdateSupportTicketStatusRequest,
    },
    state::AppState,
};

fn parse_support_category(value: &str) -> Result<SupportCategory, ApiError> {
    value
        .parse()
        .map_err(|_| ApiError::bad_request("invalid support category"))
}

fn parse_support_ticket_status(value: &str) -> Result<SupportTicketStatus, ApiError> {
    value
        .parse()
        .map_err(|_| ApiError::bad_request("invalid support ticket status"))
}

/// POST /api/v1/support/tickets
pub async fn create_support_ticket(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Json(payload): Json<CreateSupportTicketRequest>,
) -> Result<(StatusCode, Json<SupportTicketResponse>), ApiError> {
    let category = parse_support_category(&payload.category)?;

    let use_case = CreateSupportTicketUseCase::new(state.support_ticket_repository.clone());
    let ticket = use_case
        .execute(authenticated_user, payload.subject, category)
        .await
        .map_err(|err| match err {
            CreateSupportTicketError::Forbidden => ApiError::forbidden(err.to_string()),
            CreateSupportTicketError::Validation(msg) => ApiError::bad_request(msg),
            CreateSupportTicketError::Other(err) => {
                ApiError::internal_with_source("failed to create support ticket", err)
            }
        })?;

    Ok((
        StatusCode::CREATED,
        Json(SupportTicketResponse::from(ticket)),
    ))
}

/// GET /api/v1/support/tickets
pub async fn list_my_support_tickets(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<SupportTicketResponse>>, ApiError> {
    let use_case = ListMySupportTicketsUseCase::new(state.support_ticket_repository.clone());
    let tickets = use_case
        .execute(authenticated_user)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to list support tickets", err))?;
    Ok(Json(
        tickets
            .into_iter()
            .map(SupportTicketResponse::from)
            .collect(),
    ))
}

/// GET /api/v1/support/tickets/:id
pub async fn get_support_ticket(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<(SupportTicketResponse, Vec<SupportMessageResponse>)>, ApiError> {
    let ticket_id = parse_support_ticket_id(&id)?;
    let use_case = GetSupportTicketUseCase::new(state.support_ticket_repository.clone());
    let result = use_case
        .execute(ticket_id, authenticated_user)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to fetch support ticket", err))?
        .ok_or_else(|| ApiError::not_found("support ticket not found"))?;

    let (ticket, messages) = result;
    Ok(Json((
        SupportTicketResponse::from(ticket),
        messages
            .into_iter()
            .map(SupportMessageResponse::from)
            .collect(),
    )))
}

/// POST /api/v1/support/tickets/:id/messages
pub async fn reply_to_support_ticket(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(payload): Json<ReplyToSupportTicketRequest>,
) -> Result<(StatusCode, Json<SupportMessageResponse>), ApiError> {
    let ticket_id = parse_support_ticket_id(&id)?;
    let use_case = ReplyToSupportTicketUseCase::new(state.support_ticket_repository.clone());
    let message = use_case
        .execute(ticket_id, authenticated_user, payload.message)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to reply to support ticket", err))?
        .ok_or_else(|| ApiError::not_found("support ticket not found"))?;

    Ok((
        StatusCode::CREATED,
        Json(SupportMessageResponse::from(message)),
    ))
}

/// PATCH /api/v1/support/tickets/:id/status
pub async fn update_support_ticket_status(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateSupportTicketStatusRequest>,
) -> Result<StatusCode, ApiError> {
    let ticket_id = parse_support_ticket_id(&id)?;
    let target_status = parse_support_ticket_status(&payload.status)?;

    let use_case = UpdateSupportTicketStatusUseCase::new(state.support_ticket_repository.clone());
    use_case
        .execute(ticket_id, target_status, authenticated_user)
        .await
        .map_err(|err| {
            ApiError::internal_with_source("failed to update support ticket status", err)
        })?
        .ok_or_else(|| ApiError::not_found("support ticket not found"))?;

    Ok(StatusCode::NO_CONTENT)
}

fn parse_support_ticket_id(value: &str) -> Result<domain::shared::ids::SupportTicketId, ApiError> {
    let uuid = uuid::Uuid::parse_str(value)
        .map_err(|_| ApiError::bad_request("invalid support ticket id"))?;
    Ok(domain::shared::ids::SupportTicketId(uuid))
}
