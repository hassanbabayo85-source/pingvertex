use crate::{error::ApiError, state::AppState};
use axum::{
    Json,
    extract::{Extension, Path, State},
    http::StatusCode,
};
use domain::shared::types::AuthenticatedUser;
use domain::{TenantStatus, UserStatus};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
fn admin(u: &AuthenticatedUser) -> Result<(), ApiError> {
    if u.is_platform_admin() {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "platform administrator authorization is required",
        ))
    }
}
#[derive(Serialize)]
pub struct CompanyResponse {
    pub id: String,
    pub name: String,
    pub owner_user_id: String,
    pub account_type: String,
    pub status: String,
    pub wallet_balance_units: i64,
}
#[derive(Serialize)]
pub struct PlatformUserResponse {
    pub id: String,
    pub username: String,
    pub email: String,
    pub role: String,
    pub status: String,
}
#[derive(Deserialize)]
pub struct StatusRequest {
    pub status: String,
}
#[derive(Deserialize)]
pub struct WalletAdjustmentRequest {
    pub delta_units: i64,
    pub idempotency_key: String,
    pub description: Option<String>,
}
pub async fn list_companies(
    State(s): State<AppState>,
    Extension(u): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<CompanyResponse>>, ApiError> {
    admin(&u)?;
    let ts = s
        .platform_tenant_repository
        .list_all()
        .await
        .map_err(|e| ApiError::internal_with_source("failed to list companies", e))?;
    let mut out = Vec::with_capacity(ts.len());
    for t in ts {
        let b = s
            .wallet_repository
            .find(t.id.0)
            .await
            .map_err(|e| ApiError::internal_with_source("failed to load company wallet", e))?
            .map(|w| w.balance_units)
            .unwrap_or(0);
        out.push(CompanyResponse {
            id: t.id.0.to_string(),
            name: t.name,
            owner_user_id: t.owner_user_id.0.to_string(),
            account_type: t.account_type.as_str().into(),
            status: t.status.as_str().into(),
            wallet_balance_units: b,
        })
    }
    Ok(Json(out))
}
pub async fn list_users(
    State(s): State<AppState>,
    Extension(u): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<PlatformUserResponse>>, ApiError> {
    admin(&u)?;
    let us = s
        .platform_user_repository
        .list_all()
        .await
        .map_err(|e| ApiError::internal_with_source("failed to list users", e))?;
    Ok(Json(
        us.into_iter()
            .map(|x| PlatformUserResponse {
                id: x.id().0.to_string(),
                username: x.username().to_string(),
                email: x.email().to_string(),
                role: x.role().to_string(),
                status: x.status().to_string(),
            })
            .collect(),
    ))
}
pub async fn set_company_status(
    State(s): State<AppState>,
    Extension(u): Extension<AuthenticatedUser>,
    Path(id): Path<Uuid>,
    Json(r): Json<StatusRequest>,
) -> Result<StatusCode, ApiError> {
    admin(&u)?;
    let st = match r.status.as_str() {
        "active" => TenantStatus::Active,
        "suspended" => TenantStatus::Suspended,
        _ => return Err(ApiError::bad_request("status must be active or suspended")),
    };
    if !s
        .platform_tenant_repository
        .set_status(domain::TenantId(id), st)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to change company status", e))?
    {
        return Err(ApiError::not_found("company not found"));
    }
    let entry = domain::AuditLogEntry {
        id: domain::AuditLogEntryId::new(),
        actor_user_id: u.user_id(),
        tenant_id: Some(domain::TenantId(id)),
        action: if st == TenantStatus::Suspended {
            domain::AuditAction::PlatformCompanySuspended
        } else {
            domain::AuditAction::PlatformCompanyActivated
        },
        target_id: id.to_string(),
        metadata: serde_json::json!({"status":st.as_str()}),
        created_at: chrono::Utc::now(),
    };
    s.audit_log_entry_repository
        .create(&entry)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to audit company status", e))?;
    Ok(StatusCode::NO_CONTENT)
}
pub async fn set_user_status(
    State(s): State<AppState>,
    Extension(u): Extension<AuthenticatedUser>,
    Path(id): Path<Uuid>,
    Json(r): Json<StatusRequest>,
) -> Result<StatusCode, ApiError> {
    admin(&u)?;
    if id == u.user_id().0 {
        return Err(ApiError::bad_request(
            "cannot suspend the current administrator",
        ));
    }
    let st = match r.status.as_str() {
        "active" => UserStatus::Active,
        "suspended" => UserStatus::Suspended,
        "deleted" => UserStatus::Deleted,
        _ => {
            return Err(ApiError::bad_request(
                "status must be active, suspended, or deleted",
            ));
        }
    };
    let uid = domain::UserId(id);
    let mut target = s
        .platform_user_repository
        .find_by_id(&uid)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load user", e))?
        .ok_or_else(|| ApiError::not_found("user not found"))?;
    match st {
        UserStatus::Active => target.activate(chrono::Utc::now()),
        UserStatus::Suspended => target.suspend(chrono::Utc::now()),
        UserStatus::Deleted => target.delete(chrono::Utc::now()),
        UserStatus::PendingVerification => {}
    }
    s.platform_user_repository
        .update(&target)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to update user", e))?;
    let entry = domain::AuditLogEntry {
        id: domain::AuditLogEntryId::new(),
        actor_user_id: u.user_id(),
        tenant_id: None,
        action: if st == UserStatus::Suspended {
            domain::AuditAction::PlatformUserSuspended
        } else {
            domain::AuditAction::PlatformUserActivated
        },
        target_id: id.to_string(),
        metadata: serde_json::json!({"status":st.to_string()}),
        created_at: chrono::Utc::now(),
    };
    s.audit_log_entry_repository
        .create(&entry)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to audit user status", e))?;
    Ok(StatusCode::NO_CONTENT)
}
pub async fn adjust_company_wallet(
    State(s): State<AppState>,
    Extension(u): Extension<AuthenticatedUser>,
    Path(id): Path<Uuid>,
    Json(r): Json<WalletAdjustmentRequest>,
) -> Result<Json<domain::WalletTransaction>, ApiError> {
    admin(&u)?;
    if r.idempotency_key.trim().is_empty() || r.idempotency_key.len() > 200 {
        return Err(ApiError::bad_request(
            "idempotency_key must be 1-200 characters",
        ));
    }
    let tx = s
        .wallet_repository
        .adjust_balance(
            id,
            r.delta_units,
            &r.idempotency_key,
            r.description.as_deref(),
        )
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let audit = domain::FinancialAuditLogEntry::new(
        u.user_id().0.to_string(),
        Some(id),
        "platform_wallet_adjusted",
        "wallet",
        Some(tx.id.to_string()),
        serde_json::json!({"delta_units":r.delta_units}),
    );
    s.financial_audit_log_repository
        .create(&audit)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to audit wallet adjustment", e))?;
    Ok(Json(tx))
}
pub async fn grant_company_promo(
    State(s): State<AppState>,
    Extension(u): Extension<AuthenticatedUser>,
    Path(id): Path<Uuid>,
    Json(r): Json<WalletAdjustmentRequest>,
) -> Result<Json<domain::WalletTransaction>, ApiError> {
    admin(&u)?;
    if r.delta_units <= 0 {
        return Err(ApiError::bad_request("promotional credit must be positive"));
    }
    if r.idempotency_key.trim().is_empty() || r.idempotency_key.len() > 200 {
        return Err(ApiError::bad_request(
            "idempotency_key must be 1-200 characters",
        ));
    }
    let expires = chrono::Utc::now() + chrono::Duration::days(30);
    let tx = s
        .wallet_repository
        .grant_promotional_credit(
            id,
            r.delta_units,
            expires,
            &r.idempotency_key,
            r.description.as_deref(),
        )
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let audit = domain::FinancialAuditLogEntry::new(
        u.user_id().0.to_string(),
        Some(id),
        "platform_promotional_credit_granted",
        "wallet",
        Some(tx.id.to_string()),
        serde_json::json!({"amount_units":r.delta_units,"expires_at":expires}),
    );
    s.financial_audit_log_repository
        .create(&audit)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to audit promotional credit", e))?;
    Ok(Json(tx))
}

pub async fn audit_records(
    State(s): State<AppState>,
    Extension(u): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<domain::AuditLogEntry>>, ApiError> {
    admin(&u)?;
    Ok(Json(
        s.audit_log_entry_repository
            .list_all(500)
            .await
            .map_err(|e| ApiError::internal_with_source("failed to load audit records", e))?,
    ))
}

pub async fn company_usage(
    State(s): State<AppState>,
    Extension(u): Extension<AuthenticatedUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<application::ports::UsageReport>, ApiError> {
    admin(&u)?;
    let now = chrono::Utc::now();
    Ok(Json(
        s.usage_event_repository
            .report(id, now - chrono::Duration::days(30), now, None, None, 100)
            .await
            .map_err(|e| ApiError::internal_with_source("failed to load company usage", e))?,
    ))
}
