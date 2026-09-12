use crate::api::client::{self, ApiError};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct AuditEntry {
    pub id: String,
    pub actor_user_id: String,
    pub tenant_id: Option<String>,
    pub action: String,
    pub target_id: String,
    pub metadata: serde_json::Value,
    pub created_at: String,
    pub source: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct AuditPage {
    pub items: Vec<AuditEntry>,
    pub next_cursor: Option<String>,
}

pub async fn list(
    cursor: Option<&str>,
    action: Option<&str>,
    actor: Option<&str>,
    target: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<AuditPage, ApiError> {
    let mut query = vec!["limit=50".to_owned()];
    if let Some(value) = cursor.filter(|v| !v.is_empty()) {
        query.push(format!(
            "cursor={}",
            js_sys::encode_uri_component(value)
                .as_string()
                .unwrap_or_default()
        ));
    }
    if let Some(value) = action.filter(|v| !v.is_empty()) {
        query.push(format!(
            "action={}",
            js_sys::encode_uri_component(value)
                .as_string()
                .unwrap_or_default()
        ));
    }
    if let Some(value) = actor.filter(|v| !v.is_empty()) {
        query.push(format!(
            "actor={}",
            js_sys::encode_uri_component(value)
                .as_string()
                .unwrap_or_default()
        ));
    }
    if let Some(value) = target.filter(|v| !v.is_empty()) {
        query.push(format!(
            "target={}",
            js_sys::encode_uri_component(value)
                .as_string()
                .unwrap_or_default()
        ));
    }
    if let Some(value) = from.filter(|v| !v.is_empty()) {
        let value = format!("{value}T00:00:00Z");
        query.push(format!(
            "from={}",
            js_sys::encode_uri_component(&value)
                .as_string()
                .unwrap_or_default()
        ));
    }
    if let Some(value) = to.filter(|v| !v.is_empty()) {
        let value = format!("{value}T23:59:59Z");
        query.push(format!(
            "to={}",
            js_sys::encode_uri_component(&value)
                .as_string()
                .unwrap_or_default()
        ));
    }
    client::get_json(&format!("/tenant/audit?{}", query.join("&"))).await
}
