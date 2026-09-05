use axum::{
    Json,
    extract::{Extension, State},
};
use serde::Serialize;
use uuid::Uuid;

use crate::{error::ApiError, state::AppState};
use domain::shared::types::AuthenticatedUser;

#[derive(Debug, Serialize)]
pub struct WalletResponse {
    pub wallet_id: String,
    pub currency: String,
    pub balance_units: i64,
}

#[derive(Debug, Serialize)]
pub struct WalletTransactionResponse {
    pub id: String,
    pub transaction_type: String,
    pub amount_units: i64,
    pub balance_after_units: i64,
    pub reference_type: Option<String>,
    pub reference_id: Option<String>,
    pub description: Option<String>,
    pub created_at: String,
}

pub async fn get_wallet(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<WalletResponse>, ApiError> {
    let tenant_id = user.tenant_context().tenant_id().0;
    let wallet = state
        .wallet_repository
        .get_or_create(tenant_id, "USD")
        .await
        .map_err(|_| ApiError::internal("wallet unavailable"))?;
    Ok(Json(WalletResponse {
        wallet_id: wallet.id.to_string(),
        currency: wallet.currency,
        balance_units: wallet.balance_units,
    }))
}

pub async fn list_wallet_transactions(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<WalletTransactionResponse>>, ApiError> {
    let tenant_id = user.tenant_context().tenant_id().0;
    let rows = state
        .wallet_repository
        .list_transactions(tenant_id, 100)
        .await
        .map_err(|_| ApiError::internal("wallet unavailable"))?;
    Ok(Json(
        rows.into_iter()
            .map(|t| WalletTransactionResponse {
                id: t.id.to_string(),
                transaction_type: t.transaction_type.as_str().to_owned(),
                amount_units: t.amount_units,
                balance_after_units: t.balance_after_units,
                reference_type: t.reference_type,
                reference_id: t.reference_id,
                description: t.description,
                created_at: t.created_at.to_rfc3339(),
            })
            .collect(),
    ))
}

// Keep uuid imported at module boundary for consistent ID parsing policy in
// future reservation handlers; no client-supplied tenant/user IDs are accepted.
#[allow(dead_code)]
fn _tenant_id_type(_: Uuid) {}
