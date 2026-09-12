use axum::{
    Json,
    extract::{Extension, Multipart, Query, State},
};
use domain::shared::types::{AuthenticatedUser, Permission};
use serde::Deserialize;
use tracing::info;

use application::{
    dto::BulkMonitorImportPreview,
    parsing::bulk_monitor_import::parse_csv,
    use_cases::{BulkMonitorImportPreviewError, PreviewBulkMonitorImportUseCase},
};

use crate::{error::ApiError, state::AppState};

const MAX_CSV_BYTES: usize = 1_000_000;
const DEFAULT_FORECAST_DAYS: u32 = 30;

#[derive(Debug, Deserialize)]
pub struct BulkMonitorImportPreviewQuery {
    #[serde(default = "default_forecast_days")]
    pub forecast_days: u32,
}

fn default_forecast_days() -> u32 {
    DEFAULT_FORECAST_DAYS
}

/// Read-only CSV preview. It intentionally performs no monitor creation,
/// wallet reservation/capture, usage metering, or audit write.
pub async fn preview_bulk_monitor_import(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Query(query): Query<BulkMonitorImportPreviewQuery>,
    mut multipart: Multipart,
) -> Result<Json<BulkMonitorImportPreview>, ApiError> {
    // Fail before reading/parsing a potentially large body for actors that
    // are authenticated but not allowed to create monitors.
    authenticated_user
        .require(Permission::CreateMonitor)
        .map_err(|_| ApiError::forbidden("you do not have permission to create monitors"))?;

    info!(
        tenant_id = %authenticated_user.tenant_context().tenant_id().0,
        "API: preview_bulk_monitor_import"
    );

    let field = multipart
        .next_field()
        .await
        .map_err(|_| ApiError::bad_request("invalid multipart CSV upload"))?
        .ok_or_else(|| ApiError::bad_request("CSV file is required"))?;

    if field.name() != Some("file") {
        return Err(ApiError::bad_request(
            "CSV upload field must be named \"file\"",
        ));
    }

    let bytes = field
        .bytes()
        .await
        .map_err(|_| ApiError::bad_request("invalid or too large CSV upload"))?;

    if bytes.len() > MAX_CSV_BYTES {
        return Err(ApiError::bad_request(format!(
            "CSV file exceeds the {MAX_CSV_BYTES}-byte preview limit"
        )));
    }

    if multipart
        .next_field()
        .await
        .map_err(|_| ApiError::bad_request("invalid multipart CSV upload"))?
        .is_some()
    {
        return Err(ApiError::bad_request(
            "exactly one multipart field named \"file\" is allowed",
        ));
    }

    let csv = std::str::from_utf8(&bytes)
        .map_err(|_| ApiError::bad_request("CSV file must be valid UTF-8"))?;
    let (rows, parse_errors) = parse_csv(csv).map_err(ApiError::bad_request)?;

    let tenant_id = authenticated_user.tenant_context().tenant_id();
    let tenant = state
        .platform_tenant_repository
        .find_by_id(tenant_id)
        .await
        .map_err(|error| ApiError::internal_with_source("failed to load tenant status", error))?
        .ok_or_else(|| ApiError::not_found("tenant not found"))?;
    if tenant.status != domain::TenantStatus::Active {
        return Err(ApiError::forbidden("tenant is suspended"));
    }
    let snapshot = state
        .monitor_repository
        .bulk_import_snapshot_for_tenant(tenant_id)
        .await
        .map_err(|error| {
            ApiError::internal_with_source("failed to load existing monitor snapshot", error)
        })?;

    // Read-only lookup is deliberate: a preview must not create a wallet row.
    let payg_activated = state
        .wallet_repository
        .find(tenant_id.0)
        .await
        .map_err(|error| ApiError::internal_with_source("failed to read PAYG eligibility", error))?
        .is_some_and(|wallet| wallet.payg_activated_at.is_some());

    let preview = PreviewBulkMonitorImportUseCase::new(
        state.payg_pricing_repository.clone(),
        state.monitor_interval_policy_repository.clone(),
    )
    .with_subscription_repository(state.subscription_repository.clone())
    .with_pricing_config_repository(state.pricing_config_repository.clone())
    .execute(
        authenticated_user,
        rows,
        parse_errors,
        snapshot,
        payg_activated,
        query.forecast_days,
    )
    .await
    .map_err(|error| match error {
        BulkMonitorImportPreviewError::Forbidden => {
            ApiError::forbidden("you do not have permission to create monitors")
        }
        BulkMonitorImportPreviewError::Invalid(message) => ApiError::bad_request(message),
        BulkMonitorImportPreviewError::Overflow => {
            ApiError::bad_request("bulk import estimate is too large")
        }
        BulkMonitorImportPreviewError::Repository(source) => {
            ApiError::internal_with_source("failed to calculate bulk import preview", source)
        }
    })?;

    let mut preview = preview;
    preview.pricing_snapshot_hash =
        super::bulk_monitor_import_confirm::pricing_snapshot_hash(&preview.pricing_versions)?;
    Ok(Json(preview))
}
