use crate::{error::ApiError, state::AppState};
use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
};
use domain::shared::types::AuthenticatedUser;
use domain::{TenantStatus, UserStatus};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Platform-admin gate. NOTE: this only checks the platform-admin
/// flag; the additional "recent MFA" step-up requirement is enforced
/// by the `require_recent_mfa` middleware at the router layer. Keeping
/// the two checks separate lets read-only endpoints skip the MFA gate
/// in the future without also weakening the platform-admin check.
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
        out.push(CompanyResponse {
            id: t.id.0.to_string(),
            name: t.name,
            owner_user_id: t.owner_user_id.0.to_string(),
            account_type: t.account_type.as_str().into(),
            status: t.status.as_str().into(),
            wallet_balance_units: s
                .wallet_repository
                .find(t.id.0)
                .await
                .map_err(|e| ApiError::internal_with_source("failed to load company wallet", e))?
                .map(|w| w.balance_units)
                .unwrap_or(0),
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

    // Read the current tenant state FIRST so the audit record can
    // capture `old_state` alongside `new_state`. This is a read, not a
    // CAS, so a concurrent change between read and write is possible;
    // the audit `new_state` reflects exactly what THIS call requested,
    // which is the value we want in the trail.
    let old_state = s
        .platform_tenant_repository
        .find_by_id(domain::TenantId(id))
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load company", e))?
        .map(|t| t.status.as_str().to_string());

    if !s
        .platform_tenant_repository
        .set_status(domain::TenantId(id), st)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to change company status", e))?
    {
        return Err(ApiError::not_found("company not found"));
    }

    let new_state = st.as_str().to_string();
    let action = if st == TenantStatus::Suspended {
        domain::AuditAction::PlatformCompanySuspended
    } else {
        domain::AuditAction::PlatformCompanyActivated
    };

    // Enriched audit metadata. `actor_user_id` and `target_id` are
    // stored both as first-class columns (on the entry) AND inside
    // `metadata` so the JSON payload is self-contained for downstream
    // consumers that only read `metadata`.
    let entry = domain::AuditLogEntry {
        id: domain::AuditLogEntryId::new(),
        actor_user_id: u.user_id(),
        tenant_id: Some(domain::TenantId(id)),
        action,
        target_id: id.to_string(),
        metadata: serde_json::json!({
            "actor_user_id": u.user_id().0.to_string(),
            "target_id": id.to_string(),
            "action": if st == TenantStatus::Suspended { "company_suspend" } else { "company_activate" },
            "old_state": old_state,
            "new_state": new_state,
        }),
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

    // SECURITY: an admin must never be able to suspend or delete their
    // OWN account through this endpoint. Doing so would leave the
    // platform without an actor in the audit trail for the very act
    // that removed them, and (for the sole-admin case) is
    // indistinguishable from a privilege-removal denial of service.
    // This invariant is preserved by the platform-admin hardening
    // tests.
    if id == u.user_id().0 {
        return Err(ApiError::bad_request(
            "cannot change the status of the current administrator",
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

    let old_state = target.status().to_string();

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

    let action = match st {
        UserStatus::Suspended => domain::AuditAction::PlatformUserSuspended,
        UserStatus::Deleted => domain::AuditAction::PlatformUserDeleted,
        _ => domain::AuditAction::PlatformUserActivated,
    };
    let action_label = match st {
        UserStatus::Suspended => "user_suspend",
        UserStatus::Deleted => "user_delete",
        _ => "user_activate",
    };

    let entry = domain::AuditLogEntry {
        id: domain::AuditLogEntryId::new(),
        actor_user_id: u.user_id(),
        tenant_id: None,
        action,
        target_id: id.to_string(),
        metadata: serde_json::json!({
            "actor_user_id": u.user_id().0.to_string(),
            "target_id": id.to_string(),
            "action": action_label,
            "old_state": old_state,
            "new_state": st.to_string(),
        }),
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

    // Snapshot the balance BEFORE the adjustment so the audit trail
    // records the `old_state` alongside the `new_state` and the delta.
    let old_balance_units = s
        .wallet_repository
        .find(id)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load wallet", e))?
        .map(|w| w.balance_units);

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

    let new_balance_units = s
        .wallet_repository
        .find(id)
        .await
        .ok()
        .flatten()
        .map(|w| w.balance_units);

    // Financial audit entry. `old_state`/`new_state` here are wallet
    // balances; `amount` is the signed delta; `idempotency_key` is the
    // caller-supplied request identifier. Together these let
    // investigators reconstruct exactly what happened, whether the
    // operation was retried, and by how much the balance moved.
    let audit = domain::FinancialAuditLogEntry::new(
        u.user_id().0.to_string(),
        Some(id),
        "platform_wallet_adjusted",
        "wallet",
        Some(tx.id.to_string()),
        serde_json::json!({
            "actor_user_id": u.user_id().0.to_string(),
            "target_id": id.to_string(),
            "action": "wallet_adjustment",
            "old_state": old_balance_units,
            "new_state": new_balance_units,
            "amount": r.delta_units,
            "idempotency_key": r.idempotency_key,
            "description": r.description,
        }),
    );
    s.financial_audit_log_repository
        .create(&audit)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to audit wallet adjustment", e))?;

    // Also record in the generic platform audit stream so it shows up
    // in `GET /api/v1/admin/audit` alongside non-financial events.
    let generic_entry = domain::AuditLogEntry {
        id: domain::AuditLogEntryId::new(),
        actor_user_id: u.user_id(),
        tenant_id: Some(domain::TenantId(id)),
        action: domain::AuditAction::PlatformWalletAdjusted,
        target_id: id.to_string(),
        metadata: serde_json::json!({
            "actor_user_id": u.user_id().0.to_string(),
            "target_id": id.to_string(),
            "action": "wallet_adjustment",
            "amount": r.delta_units,
            "idempotency_key": r.idempotency_key,
            "transaction_id": tx.id.to_string(),
        }),
        created_at: chrono::Utc::now(),
    };
    s.audit_log_entry_repository
        .create(&generic_entry)
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

    let old_balance_units = s
        .wallet_repository
        .find(id)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load wallet", e))?
        .map(|w| w.balance_units);

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

    let new_balance_units = s
        .wallet_repository
        .find(id)
        .await
        .ok()
        .flatten()
        .map(|w| w.balance_units);

    let audit = domain::FinancialAuditLogEntry::new(
        u.user_id().0.to_string(),
        Some(id),
        "platform_promotional_credit_granted",
        "wallet",
        Some(tx.id.to_string()),
        serde_json::json!({
            "actor_user_id": u.user_id().0.to_string(),
            "target_id": id.to_string(),
            "action": "promotional_credit",
            "old_state": old_balance_units,
            "new_state": new_balance_units,
            "amount": r.delta_units,
            "expires_at": expires,
            "idempotency_key": r.idempotency_key,
            "description": r.description,
        }),
    );
    s.financial_audit_log_repository
        .create(&audit)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to audit promotional credit", e))?;

    let generic_entry = domain::AuditLogEntry {
        id: domain::AuditLogEntryId::new(),
        actor_user_id: u.user_id(),
        tenant_id: Some(domain::TenantId(id)),
        action: domain::AuditAction::PlatformPromotionalCreditGranted,
        target_id: id.to_string(),
        metadata: serde_json::json!({
            "actor_user_id": u.user_id().0.to_string(),
            "target_id": id.to_string(),
            "action": "promotional_credit",
            "amount": r.delta_units,
            "expires_at": expires,
            "idempotency_key": r.idempotency_key,
            "transaction_id": tx.id.to_string(),
        }),
        created_at: chrono::Utc::now(),
    };
    s.audit_log_entry_repository
        .create(&generic_entry)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to audit promotional credit", e))?;

    Ok(Json(tx))
}

#[derive(Debug, Deserialize)]
pub struct PlatformAuditQuery {
    pub cursor: Option<String>,
    pub limit: Option<u32>,
    pub actor: Option<Uuid>,
    pub tenant: Option<Uuid>,
    pub action: Option<String>,
    pub target: Option<String>,
    pub from: Option<chrono::DateTime<chrono::Utc>>,
    pub to: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn audit_records(
    State(s): State<AppState>,
    Extension(u): Extension<AuthenticatedUser>,
    Query(query): Query<PlatformAuditQuery>,
) -> Result<Json<crate::handlers::audit::AuditPageResponse>, ApiError> {
    admin(&u)?;
    crate::handlers::audit::_validate_date_range(query.from, query.to)?;

    let limit = query.limit.unwrap_or(50).clamp(1, 100) as usize;
    let cursor_generic = crate::handlers::audit::decode_cursor_for_admin(query.cursor.as_deref())?;

    let generic_filter = application::ports::AuditLogFilter {
        actor_user_id: query.actor.map(domain::UserId),
        tenant_id: query.tenant.map(domain::TenantId),
        action: query.action.clone(),
        target_id: query.target.clone(),
        from: query.from,
        to: query.to,
    };

    let generic_page = s
        .audit_log_entry_repository
        .list_page(&generic_filter, cursor_generic, (limit + 1) as u32)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load audit records", e))?;

    let financial_filter = application::ports::FinancialAuditLogFilter {
        action: query.action.clone(),
        resource_id: query.target.clone(),
        actor: query.actor.map(|u| u.to_string()),
        from: query.from,
        to: query.to,
        ..Default::default()
    };

    let financial_page = s
        .financial_audit_log_repository
        .list_page_admin(
            query.tenant,
            &financial_filter,
            cursor_generic.map(|c| application::ports::FinancialAuditLogCursor {
                created_at: c.created_at,
                id: c.id,
            }),
            (limit + 1) as u32,
        )
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load financial audit records", e))?;

    let mut items: Vec<crate::handlers::audit::AuditEntryResponse> =
        Vec::with_capacity(generic_page.entries.len() + financial_page.entries.len());
    for entry in generic_page.entries {
        items.push(crate::handlers::audit::response_for_admin(entry));
    }
    for entry in financial_page.entries {
        items.push(crate::handlers::audit::AuditEntryResponse {
            id: entry.id.to_string(),
            actor_user_id: entry.actor,
            tenant_id: entry.tenant_id.map(|t| t.to_string()),
            action: entry.action,
            target_id: entry.resource_id.unwrap_or(entry.resource),
            metadata: entry.metadata,
            created_at: entry.created_at,
            source: "financial".to_owned(),
        });
    }

    items.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| b.id.cmp(&a.id))
    });

    let has_more = items.len() > limit || generic_page.has_more || financial_page.has_more;
    items.truncate(limit);

    let next_cursor = if has_more {
        items.last().and_then(|last| {
            Uuid::parse_str(&last.id)
                .ok()
                .map(|id| crate::handlers::audit::encode_cursor_payload(last.created_at, id))
        })
    } else {
        None
    };

    Ok(Json(crate::handlers::audit::AuditPageResponse {
        items,
        next_cursor,
    }))
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
