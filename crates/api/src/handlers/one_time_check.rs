use crate::{error::ApiError, state::AppState, validation::parse_monitor_id};
use axum::{
    Json,
    extract::{Extension, Path, State},
    http::HeaderMap,
};
use domain::shared::types::{AuthenticatedUser, Permission};
use serde::Serialize;
use uuid::Uuid;
#[derive(Debug, Serialize)]
pub struct OneTimeCheckResponse {
    pub id: String,
    pub request_id: String,
    pub status: String,
}
fn validated_idempotency_key(headers: &HeaderMap) -> Result<Option<String>, ApiError> {
    let Some(value) = headers.get("idempotency-key") else {
        return Ok(None);
    };
    let value = value
        .to_str()
        .map_err(|_| ApiError::bad_request("invalid Idempotency-Key header"))?
        .trim();
    if value.is_empty()
        || value.len() > 200
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b':'))
    {
        return Err(ApiError::bad_request(
            "Idempotency-Key must be 1-200 URL-safe characters",
        ));
    }
    Ok(Some(value.to_owned()))
}

pub async fn create_one_time_check(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<OneTimeCheckResponse>, ApiError> {
    user.require(Permission::CreateMonitor)
        .map_err(|e| ApiError::forbidden(e.to_string()))?;
    let monitor_id = parse_monitor_id(&id)?;
    let tenant_id = user.tenant_context().tenant_id();
    let monitor = state
        .monitor_repository
        .find_by_id(monitor_id)
        .await
        .map_err(|_| ApiError::internal("monitor lookup failed"))?
        .ok_or_else(|| ApiError::not_found("monitor not found"))?;
    if monitor.tenant_id != tenant_id {
        return Err(ApiError::not_found("monitor not found"));
    }
    if monitor.kind == domain::MonitorKind::Heartbeat {
        return Err(ApiError::bad_request(
            "heartbeat monitors are passive and cannot run a one-time active check",
        ));
    }

    let request_key =
        validated_idempotency_key(&headers)?.map(|key| format!("one-time-check:{key}"));

    if let Some(existing_key) = request_key.as_deref()
        && let Some(existing) = state
            .scheduling_repository
            .get_one_time_check_by_request_id(tenant_id, monitor_id, existing_key)
            .await
            .map_err(|_| ApiError::internal("one-time check lookup failed"))?
    {
        return Ok(Json(OneTimeCheckResponse {
            id: existing.id.to_string(),
            request_id: existing.request_id,
            status: existing.status,
        }));
    }

    state
        .wallet_repository
        .get_or_create(tenant_id.0, "USD")
        .await
        .map_err(|_| ApiError::internal("wallet lookup failed"))?;
    let operation = domain::monitor_operation(monitor.kind, true);
    let pricing = state
        .payg_pricing_repository
        .find_active(operation.as_str())
        .await
        .map_err(|_| ApiError::internal("PAYG pricing unavailable"))?
        .ok_or_else(|| ApiError::internal("PAYG pricing unavailable"))?;
    if pricing.currency != "USD" || pricing.unit_price <= 0 {
        return Err(ApiError::internal("PAYG pricing configuration is invalid"));
    }
    let balance = state
        .wallet_repository
        .get_balance_summary(tenant_id.0, "USD")
        .await
        .map_err(|_| ApiError::internal("wallet lookup failed"))?;
    if balance.available_units < pricing.unit_price {
        return Err(ApiError::payment_required(
            "available PAYG wallet credit is insufficient for this check",
            "PAYG_INSUFFICIENT_BALANCE",
        ));
    }

    let request_id = request_key.unwrap_or_else(|| format!("one-time-check:{}", Uuid::new_v4()));
    let r = state
        .scheduling_repository
        .enqueue_one_time_check(tenant_id, monitor_id, user.user_id(), &request_id)
        .await
        .map_err(|err| {
            let message = err.to_string();
            if message.contains("idempotency key is already used") {
                ApiError::conflict("Idempotency-Key is already associated with another monitor")
            } else {
                ApiError::internal_with_source("failed to queue one-time check", err)
            }
        })?;
    Ok(Json(OneTimeCheckResponse {
        id: r.id.to_string(),
        request_id: r.request_id,
        status: r.status,
    }))
}
pub async fn get_one_time_check(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path((monitor_id, request_id)): Path<(String, String)>,
) -> Result<Json<OneTimeCheckResponse>, ApiError> {
    user.require(Permission::ViewMonitor)
        .map_err(|e| ApiError::forbidden(e.to_string()))?;
    let monitor_id = parse_monitor_id(&monitor_id)?;
    let r = state
        .scheduling_repository
        .get_one_time_check_by_request_id(
            user.tenant_context().tenant_id(),
            monitor_id,
            &request_id,
        )
        .await
        .map_err(|_| ApiError::internal("one-time check lookup failed"))?
        .ok_or_else(|| ApiError::not_found("one-time check not found"))?;
    Ok(Json(OneTimeCheckResponse {
        id: r.id.to_string(),
        request_id: r.request_id,
        status: r.status,
    }))
}
