use crate::{error::ApiError, state::AppState};
use axum::{
    Json,
    extract::{Extension, State},
};
use domain::Invoice;
use domain::shared::types::AuthenticatedUser;
use serde::Serialize;
#[derive(Debug, Serialize)]
pub struct InvoiceResponse {
    pub id: uuid::Uuid,
    pub tenant_id: uuid::Uuid,
    pub invoice_number: String,
    pub currency: String,
    pub subtotal: String,
    pub tax: String,
    pub total: String,
    pub status: String,
    pub period_start: chrono::DateTime<chrono::Utc>,
    pub period_end: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub paid_at: Option<chrono::DateTime<chrono::Utc>>,
}
impl From<Invoice> for InvoiceResponse {
    fn from(i: Invoice) -> Self {
        Self {
            id: i.id,
            tenant_id: i.tenant_id,
            invoice_number: i.invoice_number,
            currency: i.currency,
            subtotal: i.subtotal.to_string(),
            tax: i.tax.to_string(),
            total: i.total.to_string(),
            status: i.status.as_str().into(),
            period_start: i.period_start,
            period_end: i.period_end,
            created_at: i.created_at,
            paid_at: i.paid_at,
        }
    }
}
pub async fn list_invoices(
    State(s): State<AppState>,
    Extension(u): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<InvoiceResponse>>, ApiError> {
    let rows = s
        .invoice_repository
        .list_by_tenant(u.tenant_context().tenant_id().0, 50)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load invoices", e))?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}
