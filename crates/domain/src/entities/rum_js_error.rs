use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::errors::DomainError;
use crate::shared::ids::{RumJsErrorId, RumPageViewId, RumSessionId, RumSiteId, TenantId};
use crate::shared::policies::TenantOwned;

pub const MAX_RUM_JS_ERROR_MESSAGE_LEN: usize = 2000;

/// A JavaScript error observed in a visitor's browser.
///
/// SECURITY: `message`, `source` originate entirely from the browser and
/// are therefore attacker-controlled input (see Task G security
/// self-check). This entity only truncates/bounds the data; it does NOT
/// sanitize HTML, because it isn't HTML — it's stored as plain text.
/// Any consumer that renders this in a dashboard (Task F/frontend) MUST
/// treat it as plain text and escape on render. It must never be
/// interpolated into HTML, SQL, or shell commands unescaped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RumJsError {
    pub id: RumJsErrorId,
    pub tenant_id: TenantId,
    pub site_id: RumSiteId,
    pub session_id: RumSessionId,
    pub page_view_id: Option<RumPageViewId>,
    pub message: String,
    pub source: Option<String>,
    pub line_number: Option<i32>,
    pub column_number: Option<i32>,
    pub created_at: DateTime<Utc>,
}

impl RumJsError {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: RumJsErrorId,
        tenant_id: TenantId,
        site_id: RumSiteId,
        session_id: RumSessionId,
        page_view_id: Option<RumPageViewId>,
        message: String,
        source: Option<String>,
        line_number: Option<i32>,
        column_number: Option<i32>,
    ) -> Result<Self, DomainError> {
        let message = message.trim().to_string();
        if message.is_empty() {
            return Err(DomainError::EmptyRumJsErrorMessage);
        }

        // Truncate rather than reject: a malformed/huge error message is
        // still a legitimate error the developer wants to know about, it
        // just doesn't need to be stored in full.
        let message: String = message.chars().take(MAX_RUM_JS_ERROR_MESSAGE_LEN).collect();

        Ok(Self {
            id,
            tenant_id,
            site_id,
            session_id,
            page_view_id,
            message,
            source,
            line_number,
            column_number,
            created_at: Utc::now(),
        })
    }
}

impl TenantOwned for RumJsError {
    fn tenant_id(&self) -> TenantId {
        self.tenant_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_message() {
        let err = RumJsError::new(
            RumJsErrorId::new(),
            TenantId::new(),
            RumSiteId::new(),
            RumSessionId::new(),
            None,
            "   ".to_string(),
            None,
            None,
            None,
        )
        .unwrap_err();
        assert_eq!(err, DomainError::EmptyRumJsErrorMessage);
    }

    #[test]
    fn truncates_long_message() {
        let err = RumJsError::new(
            RumJsErrorId::new(),
            TenantId::new(),
            RumSiteId::new(),
            RumSessionId::new(),
            None,
            "x".repeat(MAX_RUM_JS_ERROR_MESSAGE_LEN + 500),
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(err.message.chars().count(), MAX_RUM_JS_ERROR_MESSAGE_LEN);
    }
}
