use crate::{error::ApiError, state::AppState};
use axum::{
    Json,
    extract::{Extension, Query, State},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use domain::shared::types::{AuthenticatedUser, Permission};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 100;

#[derive(Debug, Deserialize)]
pub struct AuditQuery {
    pub cursor: Option<String>,
    pub limit: Option<u32>,
    pub action: Option<String>,
    pub actor: Option<Uuid>,
    pub tenant: Option<Uuid>,
    pub target: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntryResponse {
    pub id: String,
    pub actor_user_id: String,
    pub tenant_id: Option<String>,
    pub action: String,
    pub target_id: String,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub source: String,
}
#[derive(Debug, Serialize)]
pub struct AuditPageResponse {
    pub items: Vec<AuditEntryResponse>,
    pub next_cursor: Option<String>,
}
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CursorPayload {
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) id: Uuid,
}

pub(crate) fn decode_cursor(
    raw: Option<&str>,
) -> Result<Option<application::ports::AuditLogCursor>, ApiError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    if raw.len() > 512 {
        return Err(ApiError::bad_request("audit cursor is too long"));
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(raw)
        .map_err(|_| ApiError::bad_request("invalid audit cursor"))?;
    let payload: CursorPayload = serde_json::from_slice(&bytes)
        .map_err(|_| ApiError::bad_request("invalid audit cursor"))?;
    Ok(Some(application::ports::AuditLogCursor {
        created_at: payload.created_at,
        id: payload.id,
    }))
}
pub(crate) fn encode_cursor(entry: &domain::AuditLogEntry) -> String {
    URL_SAFE_NO_PAD.encode(
        serde_json::to_vec(&CursorPayload {
            created_at: entry.created_at,
            id: entry.id.0,
        })
        .expect("audit cursor serialization"),
    )
}
pub(crate) fn response(entry: domain::AuditLogEntry) -> AuditEntryResponse {
    AuditEntryResponse {
        id: entry.id.0.to_string(),
        actor_user_id: entry.actor_user_id.0.to_string(),
        tenant_id: entry.tenant_id.map(|t| t.0.to_string()),
        action: format!("{:?}", entry.action),
        target_id: entry.target_id,
        metadata: entry.metadata,
        created_at: entry.created_at,
        source: "audit".to_owned(),
    }
}
pub(crate) fn decode_cursor_for_admin(
    raw: Option<&str>,
) -> Result<Option<application::ports::AuditLogCursor>, ApiError> {
    decode_cursor(raw)
}
pub(crate) fn encode_cursor_for_admin(entry: &domain::AuditLogEntry) -> String {
    encode_cursor(entry)
}
pub(crate) fn response_for_admin(entry: domain::AuditLogEntry) -> AuditEntryResponse {
    response(entry)
}

pub async fn tenant_audit(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Query(query): Query<AuditQuery>,
) -> Result<Json<AuditPageResponse>, ApiError> {
    user.require(Permission::ViewAuditLog)
        .map_err(|e| ApiError::forbidden(e.to_string()))?;
    if query.tenant.is_some() {
        return Err(ApiError::forbidden(
            "tenant is fixed by the authenticated context",
        ));
    }
    _validate_date_range(query.from, query.to)?;
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT) as usize;
    let cursor = decode_cursor(query.cursor.as_deref())?;
    let tenant_id = user.tenant_context().tenant_id();
    let filter = application::ports::AuditLogFilter {
        actor_user_id: query.actor.map(domain::UserId),
        tenant_id: Some(tenant_id),
        action: query.action.clone(),
        target_id: query.target.clone(),
        from: query.from,
        to: query.to,
    };
    let generic = state
        .audit_log_entry_repository
        .list_page(&filter, cursor, (limit + 1) as u32)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load tenant audit", e))?;
    let financial = state
        .financial_audit_log_repository
        .list_page(
            tenant_id.0,
            &application::ports::FinancialAuditLogFilter {
                action: query.action.clone(),
                resource_id: query.target.clone(),
                from: query.from,
                to: query.to,
                ..Default::default()
            },
            cursor.map(|c| application::ports::FinancialAuditLogCursor {
                created_at: c.created_at,
                id: c.id,
            }),
            (limit + 1) as u32,
        )
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load tenant financial audit", e))?;
    let mut items = Vec::with_capacity(generic.entries.len() + financial.entries.len());
    for entry in generic.entries {
        items.push(response(entry));
    }
    for entry in financial.entries {
        items.push(AuditEntryResponse {
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
    let has_more = items.len() > limit || generic.has_more || financial.has_more;
    items.truncate(limit);
    let next_cursor = if has_more {
        items
            .last()
            .and_then(|last| Uuid::parse_str(&last.id).ok())
            .and_then(|id| {
                items
                    .last()
                    .map(|entry| encode_cursor_payload(entry.created_at, id))
            })
    } else {
        None
    };
    Ok(Json(AuditPageResponse { items, next_cursor }))
}

pub(crate) fn encode_cursor_payload(created_at: DateTime<Utc>, id: Uuid) -> String {
    URL_SAFE_NO_PAD.encode(
        serde_json::to_vec(&CursorPayload { created_at, id }).expect("audit cursor serialization"),
    )
}

pub fn _validate_date_range(
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
) -> Result<(), ApiError> {
    if from.zip(to).is_some_and(|(a, b)| a >= b) {
        return Err(ApiError::bad_request("audit from must be earlier than to"));
    }
    Ok(())
}
