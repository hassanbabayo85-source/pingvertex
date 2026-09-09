use axum::{
    Json,
    extract::{Extension, Path, State},
    http::StatusCode,
};
use domain::pricing::{FreeTierDefaults, PricingConfig};
use domain::shared::types::AuthenticatedUser;
use domain::value_objects::RegionCode;
use domain::{AccountType, MonitorKind, PaygOperation};
use std::str::FromStr;
use tracing::info;

use crate::{
    error::ApiError,
    handlers::pricing_types::{
        CreatePricingConfigRequest, PricingAuditLogResponse, PricingConfigResponse,
    },
    state::AppState,
};

#[derive(Debug, serde::Deserialize)]
pub struct CreatePaygPriceRequest {
    pub operation: String,
    pub unit_name: String,
    pub unit_price: i64,
    pub currency: String,
    pub effective_from: Option<chrono::DateTime<chrono::Utc>>,
    pub account_type: Option<String>,
    pub region: Option<String>,
    pub monitor_kind: Option<String>,
    pub min_quantity: Option<u64>,
    pub max_quantity: Option<u64>,
    pub reason: String,
}

#[derive(Debug, serde::Serialize)]
pub struct PaygPriceResponse {
    pub id: String,
    pub version: i64,
    pub operation: String,
    pub unit_name: String,
    pub unit_price: i64,
    pub currency: String,
    pub effective_from: String,
}

fn validate_payg_price_request(req: &CreatePaygPriceRequest) -> Result<PaygOperation, ApiError> {
    let operation = PaygOperation::from_str(req.operation.trim())
        .map_err(|_| ApiError::bad_request("unsupported PAYG operation"))?;
    if req.unit_price <= 0 {
        return Err(ApiError::bad_request(
            "unit_price must be greater than zero",
        ));
    }
    if req.unit_name.trim().is_empty() || req.unit_name.len() > 64 {
        return Err(ApiError::bad_request(
            "unit_name must be between 1 and 64 characters",
        ));
    }
    if req.currency != "USD" {
        return Err(ApiError::bad_request(
            "PAYG currently supports USD wallet pricing only",
        ));
    }
    if req.reason.trim().is_empty() || req.reason.len() > 500 {
        return Err(ApiError::bad_request(
            "reason must be between 1 and 500 characters",
        ));
    }
    Ok(operation)
}

pub async fn create_payg_price(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(payload): Json<CreatePaygPriceRequest>,
) -> Result<(StatusCode, Json<PaygPriceResponse>), ApiError> {
    require_platform_admin(&user)?;
    let operation = validate_payg_price_request(&payload)?;
    let account_type = payload
        .account_type
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(AccountType::from_str)
        .transpose()
        .map_err(|_| ApiError::bad_request("account_type must be individual or company"))?;
    let region = payload
        .region
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(RegionCode::new)
        .transpose()
        .map_err(|_| ApiError::bad_request("unsupported pricing region"))?;
    let monitor_kind = payload
        .monitor_kind
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| match v.to_ascii_lowercase().as_str() {
            "http" => Ok(MonitorKind::Http),
            "tcp" => Ok(MonitorKind::Tcp),
            "dns" => Ok(MonitorKind::Dns),
            "heartbeat" => Ok(MonitorKind::Heartbeat),
            "flow" => Ok(MonitorKind::Flow),
            _ => Err(ApiError::bad_request("unsupported pricing monitor kind")),
        })
        .transpose()?;
    if payload
        .min_quantity
        .zip(payload.max_quantity)
        .is_some_and(|(min, max)| min > max)
    {
        return Err(ApiError::bad_request(
            "min_quantity must be less than or equal to max_quantity",
        ));
    }

    let pricing = state
        .payg_pricing_repository
        .create_version_with_context(
            operation.as_str(),
            payload.unit_name.trim(),
            payload.unit_price,
            "USD",
            payload.effective_from.unwrap_or_else(chrono::Utc::now),
            &application::ports::PricingRuleContext {
                account_type,
                region,
                monitor_kind,
                min_quantity: payload.min_quantity,
                max_quantity: payload.max_quantity,
            },
        )
        .await
        .map_err(|e| ApiError::internal_with_source("failed to create PAYG price version", e))?;

    state
        .financial_audit_log_repository
        .create(&domain::FinancialAuditLogEntry::new(
            user.user_id().0.to_string(),
            Some(user.tenant_context().tenant_id().0),
            "payg_price_created",
            "payg_pricing",
            Some(pricing.id.to_string()),
            serde_json::json!({
                "version": pricing.version,
                "operation": pricing.operation,
                "unit_name": pricing.unit_name,
                "unit_price": pricing.unit_price,
                "currency": pricing.currency,
                "effective_from": pricing.effective_from,
                "reason": payload.reason.trim(),
            }),
        ))
        .await
        .map_err(|e| ApiError::internal_with_source("failed to audit PAYG price change", e))?;

    Ok((
        StatusCode::CREATED,
        Json(PaygPriceResponse {
            id: pricing.id.to_string(),
            version: pricing.version,
            operation: pricing.operation,
            unit_name: pricing.unit_name,
            unit_price: pricing.unit_price,
            currency: pricing.currency,
            effective_from: pricing.effective_from.to_rfc3339(),
        }),
    ))
}

fn validate_config_request(req: &CreatePricingConfigRequest) -> Result<(), ApiError> {
    if req.free_tier.monitors != domain::FREE_MONITOR_LIMIT
        || req.free_tier.interval_seconds != domain::FREE_MIN_INTERVAL_SECONDS
    {
        return Err(ApiError::bad_request(
            "Free entitlement is fixed at 10 monitors with a 5-minute minimum interval",
        ));
    }
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
    if req.sms_markup_multiplier < rust_decimal::Decimal::ZERO {
        return Err(ApiError::bad_request("sms_markup_multiplier must be >= 0"));
    }
    if req.enterprise_sla_monthly_price < rust_decimal::Decimal::ZERO {
        return Err(ApiError::bad_request(
            "enterprise_sla_monthly_price must be >= 0",
        ));
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

fn require_platform_admin(user: &AuthenticatedUser) -> Result<(), ApiError> {
    if user.is_platform_admin() {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "platform administrator authorization is required",
        ))
    }
}

pub async fn list_pricing_configs(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<PricingConfigResponse>>, ApiError> {
    info!("API: list_pricing_configs");
    require_platform_admin(&user)?;

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
    require_platform_admin(&user)?;

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
    require_platform_admin(&user)?;

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
    require_platform_admin(&authenticated_user)?;
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
        sms_markup_multiplier: payload.sms_markup_multiplier,
        enterprise_sla_monthly_price: payload.enterprise_sla_monthly_price,
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
    require_platform_admin(&user)?;

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
