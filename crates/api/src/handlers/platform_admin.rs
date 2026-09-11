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
        action: match st {
            UserStatus::Suspended => domain::AuditAction::PlatformUserSuspended,
            UserStatus::Deleted => domain::AuditAction::PlatformUserDeleted,
            _ => domain::AuditAction::PlatformUserActivated,
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

    // Admin audit is the platform-admin counterpart of `GET /api/v1/tenant/audit`.
    // Like the tenant endpoint, it must merge the generic audit stream
    // (`audit_log`) with the financial audit stream (`financial_audit_log`)
    // so admins see both in a single cursor-paginated response. The tenant
    // endpoint is scoped to exactly one tenant; this one may be scoped to
    // one, many, or no tenant, which is why the financial side goes through
    // `list_page_admin` (accepts `Option<Uuid>`) instead of `list_page`.
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

    // The generic audit stream exposes the audited resource identity as
    // `target_id`; the financial stream uses `resource_id`. Mapping one to
    // the other keeps the "target" filter semantically identical across
    // both streams.
    let financial_filter = application::ports::FinancialAuditLogFilter {
        action: query.action.clone(),
        resource_id: query.target.clone(),
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

    // Same ordering as the repositories (`created_at DESC, id DESC`) so the
    // merged page is stable and cursor-consistent across both streams.
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
