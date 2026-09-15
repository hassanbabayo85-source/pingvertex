use crate::{error::ApiError, state::AppState};
use axum::{
    Json,
    extract::{Extension, Path, State},
};
use domain::shared::types::{AuthenticatedUser, Permission};
use uuid::Uuid;

pub async fn get_invoice(
    State(s): State<AppState>,
    Extension(u): Extension<AuthenticatedUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::handlers::invoices::InvoiceResponse>, ApiError> {
    u.require(Permission::ViewBilling)
        .map_err(|_| ApiError::forbidden("billing access is not permitted"))?;
    let tenant = u.tenant_context().tenant_id().0;
    let invoice = s
        .invoice_repository
        .find_by_id_for_tenant(id, tenant)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load invoice", e))?
        .ok_or_else(|| ApiError::not_found("invoice not found"))?;
    Ok(Json(invoice.into()))
}
