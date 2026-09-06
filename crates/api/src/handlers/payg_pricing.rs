use axum::{extract::{Extension,State},Json};
use domain::shared::types::{AuthenticatedUser,Permission};
use serde::Serialize;
use crate::{error::ApiError,state::AppState};
#[derive(Debug,Serialize)]
pub struct PAYGPricingResponse{pub operation:String,pub unit_name:String,pub unit_price:i64,pub currency:String,pub version:i64,pub effective_from:chrono::DateTime<chrono::Utc>}
pub async fn payg_pricing(State(s):State<AppState>,Extension(u):Extension<AuthenticatedUser>)->Result<Json<Vec<PAYGPricingResponse>>,ApiError>{
 u.require(Permission::ViewBilling).map_err(|_|ApiError::forbidden("billing access is not permitted"))?;
 let rows=s.payg_pricing_repository.list_active().await.map_err(|e|ApiError::internal_with_source("failed to load PAYG pricing",e))?;
 Ok(Json(rows.into_iter().map(|p|PAYGPricingResponse{operation:p.operation,unit_name:p.unit_name,unit_price:p.unit_price,currency:p.currency,version:p.version,effective_from:p.effective_from}).collect()))
}

