use std::{collections::HashMap, fmt::Write as _};

use application::{
    dto::{
        BulkMonitorImportConfirmResponse, CreateBulkMonitorImportJob,
        CreateBulkMonitorImportJobOutcome, NewBulkMonitorImportRow,
    },
    parsing::bulk_monitor_import::parse_csv,
    use_cases::{BulkMonitorImportPreviewError, PreviewBulkMonitorImportUseCase},
};
use axum::{
    Json,
    extract::{Extension, Multipart, Path, Query, State},
    http::HeaderMap,
};
use domain::shared::types::{AuthenticatedUser, Permission};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use tracing::info;
use uuid::Uuid;

use crate::{error::ApiError, state::AppState};

const MAX_CSV_BYTES: usize = 1_000_000;
const DEFAULT_FORECAST_DAYS: u32 = 30;
const DEFAULT_BATCH_SIZE: i32 = 500;
const MIN_BATCH_SIZE: i32 = 50;
const MAX_BATCH_SIZE: i32 = 1000;

fn configured_batch_size() -> i32 {
    std::env::var("BULK_IMPORT_BATCH_SIZE")
        .ok()
        .and_then(|v| v.trim().parse::<i32>().ok())
        .map(|v| v.clamp(MIN_BATCH_SIZE, MAX_BATCH_SIZE))
        .unwrap_or(DEFAULT_BATCH_SIZE)
}

#[derive(Debug, Deserialize)]
pub struct BulkMonitorImportConfirmQuery {
    #[serde(default = "default_forecast_days")]
    pub forecast_days: u32,
}

fn default_forecast_days() -> u32 {
    DEFAULT_FORECAST_DAYS
}

fn validated_idempotency_key(headers: &HeaderMap) -> Result<String, ApiError> {
    let value = headers
        .get("idempotency-key")
        .ok_or_else(|| ApiError::bad_request("Idempotency-Key header is required"))?
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
    Ok(value.to_owned())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

pub async fn confirm_bulk_monitor_import(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Query(query): Query<BulkMonitorImportConfirmQuery>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<BulkMonitorImportConfirmResponse>, ApiError> {
    authenticated_user
        .require(Permission::CreateMonitor)
        .map_err(|_| ApiError::forbidden("you do not have permission to create monitors"))?;

    let repository = state
        .bulk_monitor_import_repository
        .as_ref()
        .ok_or_else(|| ApiError::internal("bulk monitor import is not configured"))?;
    let idempotency_key = validated_idempotency_key(&headers)?;

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
            "CSV file exceeds the {MAX_CSV_BYTES}-byte confirm limit"
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
    let snapshot = state
        .monitor_repository
        .bulk_import_snapshot_for_tenant(tenant_id)
        .await
        .map_err(|error| {
            ApiError::internal_with_source("failed to load existing monitor snapshot", error)
        })?;
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
    .execute(
        authenticated_user,
        rows.clone(),
        parse_errors.clone(),
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
            ApiError::internal_with_source("failed to validate bulk import confirmation", source)
        }
    })?;

    if preview.valid_rows == 0 {
        return Err(ApiError::bad_request(
            "bulk import has no valid rows to create; fix preview errors before confirming",
        ));
    }

    let errors_by_row = preview
        .errors
        .iter()
        .map(|error| (error.row, error.errors.clone()))
        .collect::<HashMap<_, _>>();

    let mut durable_rows = Vec::with_capacity(preview.total_rows);
    for row in rows {
        let errors = errors_by_row.get(&row.source_row);
        durable_rows.push(NewBulkMonitorImportRow {
            row_number: i32::try_from(row.source_row)
                .map_err(|_| ApiError::bad_request("CSV row number is too large"))?,
            payload: serde_json::to_value(&row)
                .map_err(|_| ApiError::internal("failed to serialize bulk import row"))?,
            validation_status: if errors.is_some() {
                "INVALID".to_owned()
            } else {
                "VALID".to_owned()
            },
            validation_errors: errors.map(|errors| json!(errors)),
            planned_monitor_id: errors.is_none().then(Uuid::new_v4),
        });
    }
    for error in parse_errors {
        durable_rows.push(NewBulkMonitorImportRow {
            row_number: i32::try_from(error.row)
                .map_err(|_| ApiError::bad_request("CSV row number is too large"))?,
            payload: json!({}),
            validation_status: "INVALID".to_owned(),
            validation_errors: Some(json!(error.errors)),
            planned_monitor_id: None,
        });
    }
    durable_rows.sort_by_key(|row| row.row_number);

    let input = CreateBulkMonitorImportJob {
        id: Uuid::new_v4(),
        tenant_id: tenant_id.0,
        actor_user_id: authenticated_user.user_id().0,
        idempotency_key,
        source_sha256: sha256_hex(&bytes),
        total_rows: i32::try_from(preview.total_rows)
            .map_err(|_| ApiError::bad_request("too many CSV rows"))?,
        valid_rows: i32::try_from(preview.valid_rows)
            .map_err(|_| ApiError::bad_request("too many valid CSV rows"))?,
        invalid_rows: i32::try_from(preview.invalid_rows)
            .map_err(|_| ApiError::bad_request("too many invalid CSV rows"))?,
        forecast_days: i32::try_from(preview.forecast_days)
            .map_err(|_| ApiError::bad_request("forecast_days is too large"))?,
        estimated_checks: i64::try_from(preview.estimated_execution_quantity)
            .map_err(|_| ApiError::bad_request("bulk import estimate is too large"))?,
        estimated_charge_units: preview.estimated_payg_cost_units,
        currency: preview.currency.clone(),
        batch_size: configured_batch_size(),
        pricing_snapshot: serde_json::to_value(&preview.pricing_versions)
            .map_err(|_| ApiError::internal("failed to serialize pricing snapshot"))?,
        rows: durable_rows,
    };

    let (job, replayed) = match repository
        .create_confirmed_job(&input)
        .await
        .map_err(|error| {
            ApiError::internal_with_source("failed to persist bulk import job", error)
        })? {
        CreateBulkMonitorImportJobOutcome::Created(job) => (job, false),
        CreateBulkMonitorImportJobOutcome::Existing(job) => (job, true),
        CreateBulkMonitorImportJobOutcome::IdempotencyConflict => {
            return Err(ApiError::conflict(
                "Idempotency-Key is already associated with a different CSV payload",
            ));
        }
    };

    info!(
        tenant_id = %tenant_id.0,
        job_id = %job.id,
        replayed,
        valid_rows = job.valid_rows,
        invalid_rows = job.invalid_rows,
        "API: confirm_bulk_monitor_import"
    );

    Ok(Json(BulkMonitorImportConfirmResponse { job, replayed }))
}

pub async fn get_bulk_monitor_import_job(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(job_id): Path<Uuid>,
) -> Result<Json<application::dto::BulkMonitorImportJob>, ApiError> {
    authenticated_user
        .require(Permission::ViewMonitor)
        .map_err(|_| ApiError::forbidden("you do not have permission to view monitors"))?;
    let repository = state
        .bulk_monitor_import_repository
        .as_ref()
        .ok_or_else(|| ApiError::internal("bulk monitor import is not configured"))?;

    let tenant_id = authenticated_user.tenant_context().tenant_id().0;
    let job = repository
        .find_job_for_tenant(job_id, tenant_id)
        .await
        .map_err(|error| ApiError::internal_with_source("failed to load bulk import job", error))?
        .ok_or_else(|| ApiError::not_found("bulk import job not found"))?;

    Ok(Json(job))
}
