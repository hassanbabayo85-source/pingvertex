use axum::{
    Json,
    extract::{Extension, Path, State},
    http::StatusCode,
};
use domain::pricing::{FreeTierDefaults, PricingConfig};
use domain::shared::types::{AuthenticatedUser, Permission};
use tracing::info;

use crate::{
    error::ApiError,
    handlers::pricing_types::{
        CreatePricingConfigRequest, PricingAuditLogResponse, PricingConfigResponse,
    },
    state::AppState,
};

fn validate_config_request(req: &CreatePricingConfigRequest) -> Result<(), ApiError> {
    if req.cost_per_check < rust_decimal::Decimal::ZERO {
        return Err(ApiError::bad_request("cost_per_check must be >= 0"));
    }
    if req.cost_per_gb_hot < rust_decimal::Decimal::ZERO {
        return Err(ApiError::bad_request("cost_per_gb_hot must be >= 0"));
    }
    if req.cost_per_gb_warm < rust_decimal::Decimal::ZERO {
        return Err(ApiError::bad_request("cost_per_gb_warm must be >= 0"));
    }
    if req.cost_per_gb_cold < rust_decimal::Decimal::ZERO {
        return Err(ApiError::bad_request("cost_per_gb_cold must be >= 0"));
    }
    if req.backup_multiplier < rust_decimal::Decimal::ZERO {
        return Err(ApiError::bad_request("backup_multiplier must be >= 0"));
    }
    if req.markup_multiplier < rust_decimal::Decimal::ZERO {
        return Err(ApiError::bad_request("markup_multiplier must be >= 0"));
    }
    if req.min_paid_monthly_price < rust_decimal::Decimal::ZERO {
        return Err(ApiError::bad_request("min_paid_monthly_price must be >= 0"));
    }
    if req.cost_per_gb_egress < rust_decimal::Decimal::ZERO {
        return Err(ApiError::bad_request("cost_per_gb_egress must be >= 0"));
    }
    if req.cost_safety_buffer < rust_decimal::Decimal::ZERO {
        return Err(ApiError::bad_request("cost_safety_buffer must be >= 0"));
    }
    if req.min_interval_seconds < 1 {
        return Err(ApiError::bad_request(
            "min_interval_seconds must be at least 1",
        ));
    }
    if req.max_monitors == 0 {
        return Err(ApiError::bad_request("max_monitors must be greater than 0"));
    }
    if req.max_retention_days < 90 {
        return Err(ApiError::bad_request(
            "max_retention_days must be at least 90 days",
        ));
    }
    if req.currency.is_empty() {
        return Err(ApiError::bad_request("currency must not be empty"));
    }
    Ok(())
}

fn require_manage_billing(user: &AuthenticatedUser) -> Result<(), ApiError> {
    if user.can(Permission::ManageBilling) {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "you do not have permission to manage pricing configs",
        ))
    }
}

pub async fn list_pricing_configs(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<PricingConfigResponse>>, ApiError> {
    info!("API: list_pricing_configs");
    require_manage_billing(&user)?;

    let configs = state
        .pricing_config_repository
        .list_versions()
        .await
        .map_err(|e| ApiError::internal_with_source("failed to list pricing configs", e))?;

    Ok(Json(
        configs
            .into_iter()
            .map(PricingConfigResponse::from)
            .collect(),
    ))
}

pub async fn get_pricing_config_by_version(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(version): Path<u32>,
) -> Result<Json<PricingConfigResponse>, ApiError> {
    info!("API: get_pricing_config_by_version (version={})", version);
    require_manage_billing(&user)?;

    let config = state
        .pricing_config_repository
        .find_by_version(version)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to fetch pricing config", e))?
        .ok_or_else(|| ApiError::not_found("pricing config version not found"))?;

    Ok(Json(PricingConfigResponse::from(config)))
}

pub async fn get_active_pricing_config(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<PricingConfigResponse>, ApiError> {
    info!("API: get_active_pricing_config");
    require_manage_billing(&user)?;

    let config = state
        .pricing_config_repository
        .find_active_config()
        .await
        .map_err(|e| ApiError::internal_with_source("failed to fetch active pricing config", e))?
        .ok_or_else(|| ApiError::not_found("no active pricing config found"))?;

    Ok(Json(PricingConfigResponse::from(config)))
}

pub async fn create_pricing_config(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Json(payload): Json<CreatePricingConfigRequest>,
) -> Result<(StatusCode, Json<PricingConfigResponse>), ApiError> {
    info!("API: create_pricing_config");
    require_manage_billing(&authenticated_user)?;
    validate_config_request(&payload)?;

    // Compute next version
    let existing = state
        .pricing_config_repository
        .list_versions()
        .await
        .map_err(|e| ApiError::internal_with_source("failed to list pricing configs", e))?;

    let next_version = existing.iter().map(|c| c.version).max().unwrap_or(0) + 1;

    let config = PricingConfig {
        version: next_version,
        cost_per_check: payload.cost_per_check,
        effective_storage_bytes_per_check: payload.effective_storage_bytes_per_check,
        cost_per_gb_hot: payload.cost_per_gb_hot,
        cost_per_gb_warm: payload.cost_per_gb_warm,
        cost_per_gb_cold: payload.cost_per_gb_cold,
        backup_multiplier: payload.backup_multiplier,
        markup_multiplier: payload.markup_multiplier,
        min_interval_seconds: payload.min_interval_seconds,
        max_monitors: payload.max_monitors,
        max_retention_days: payload.max_retention_days,
        min_paid_monthly_price: payload.min_paid_monthly_price,
        free_tier: FreeTierDefaults {
            monitors: payload.free_tier.monitors,
            interval_seconds: payload.free_tier.interval_seconds,
            retention_days: payload.free_tier.retention_days,
        },
        currency: payload.currency,
        effective_at: payload.effective_at.unwrap_or_else(chrono::Utc::now),
        avg_billable_network_bytes_per_check: payload.avg_billable_network_bytes_per_check,
        cost_per_gb_egress: payload.cost_per_gb_egress,
        cost_safety_buffer: payload.cost_safety_buffer,
        created_by: Some(authenticated_user.user_id()),
    };

    state
        .pricing_config_repository
        .create_version(&config, Some(payload.reason))
        .await
        .map_err(|e| ApiError::internal_with_source("failed to create pricing config", e))?;

    let created = state
        .pricing_config_repository
        .find_by_version(next_version)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to fetch created config", e))?
        .ok_or_else(|| ApiError::internal("created config not found"))?;

    Ok((
        StatusCode::CREATED,
        Json(PricingConfigResponse::from(created)),
    ))
}

pub async fn list_pricing_audit_logs(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<PricingAuditLogResponse>>, ApiError> {
    info!("API: list_pricing_audit_logs");
    require_manage_billing(&user)?;

    let logs = state
        .pricing_config_repository
        .list_audit_logs()
        .await
        .map_err(|e| ApiError::internal_with_source("failed to list audit logs", e))?;

    Ok(Json(
        logs.into_iter()
            .map(PricingAuditLogResponse::from)
            .collect(),
    ))
}
