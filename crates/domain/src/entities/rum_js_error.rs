use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::errors::DomainError;
use crate::shared::ids::{RumJsErrorId, RumPageViewId, RumSessionId, RumSiteId, TenantId};
use crate::shared::policies::TenantOwned;
use crate::shared::url_sanitize::strip_query_and_fragment;

pub const MAX_RUM_JS_ERROR_MESSAGE_LEN: usize = 2000;

/// A JavaScript error observed in a visitor's browser.
///
/// SECURITY: `message`, `source` originate entirely from the browser and
/// are therefore attacker-controlled input (see Task G security
/// self-check). This entity only truncates/bounds `message`, normalizes
/// `source`, and does NOT sanitize HTML, because it isn't HTML — it's
/// stored as plain text. Any consumer that renders this in a dashboard
/// (Task F/frontend) MUST treat it as plain text and escape on render.
/// It must never be interpolated into HTML, SQL, or shell commands
/// unescaped.
///
/// `source` is normalized (query string and fragment stripped) in `new`
/// before the entity is constructed — see [`crate::shared::url_sanitize`].
/// A `source` that is empty after stripping (e.g. it was literally
/// `"?token=secret"`) is stored as `None`, not as an empty string.
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

        // Strip `?query=…` and `#fragment`. If the result is empty,
        // treat it as "no source" rather than storing an empty string —
        // an empty source is indistinguishable from a missing one
        // downstream and carries no information.
        let source = source
            .map(|s| strip_query_and_fragment(s.trim()))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

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

    #[test]
    fn strips_query_string_from_source() {
        let err = RumJsError::new(
            RumJsErrorId::new(),
            TenantId::new(),
            RumSiteId::new(),
            RumSessionId::new(),
            None,
            "boom".to_string(),
            Some("https://example.com/bundle.js?token=secret".to_string()),
            Some(1),
            Some(1),
        )
        .unwrap();
        let source = err.source.as_deref().unwrap();
        assert_eq!(source, "https://example.com/bundle.js");
        assert!(!source.contains("secret"));
        assert!(!source.contains('?'));
    }

    #[test]
    fn strips_fragment_from_source() {
        let err = RumJsError::new(
            RumJsErrorId::new(),
            TenantId::new(),
            RumSiteId::new(),
            RumSessionId::new(),
            None,
            "boom".to_string(),
            Some("https://example.com/bundle.js#frag".to_string()),
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            err.source.as_deref(),
            Some("https://example.com/bundle.js")
        );
        assert!(!err.source.as_deref().unwrap().contains('#'));
    }

    #[test]
    fn strips_query_and_fragment_from_full_url_source() {
        let err = RumJsError::new(
            RumJsErrorId::new(),
            TenantId::new(),
            RumSiteId::new(),
            RumSessionId::new(),
            None,
            "boom".to_string(),
            Some("https://example.com/page?token=secret#fragment".to_string()),
            None,
            None,
        )
        .unwrap();
        let source = err.source.as_deref().unwrap();
        assert_eq!(source, "https://example.com/page");
        assert!(!source.contains("secret"));
        assert!(!source.contains('?'));
        assert!(!source.contains('#'));
    }

    #[test]
    fn every_named_secret_param_is_stripped_from_source() {
        for param in [
            "?token=t",
            "?code=c",
            "?password=p",
            "?secret=s",
            "?key=k",
            "?email=e",
        ] {
            let src = format!("https://example.com/page{param}");
            let err = RumJsError::new(
                RumJsErrorId::new(),
                TenantId::new(),
                RumSiteId::new(),
                RumSessionId::new(),
                None,
                "boom".to_string(),
                Some(src),
                None,
                None,
            )
            .unwrap();
            assert_eq!(
                err.source.as_deref(),
                Some("https://example.com/page")
            );
        }
    }

    #[test]
    fn source_that_becomes_empty_after_stripping_becomes_none() {
        let err = RumJsError::new(
            RumJsErrorId::new(),
            TenantId::new(),
            RumSiteId::new(),
            RumSessionId::new(),
            None,
            "boom".to_string(),
            Some("?token=secret".to_string()),
            None,
            None,
        )
        .unwrap();
        assert!(err.source.is_none());
    }

    #[test]
    fn none_source_stays_none() {
        let err = RumJsError::new(
            RumJsErrorId::new(),
            TenantId::new(),
            RumSiteId::new(),
            RumSessionId::new(),
            None,
            "boom".to_string(),
            None,
            None,
            None,
        )
        .unwrap();
        assert!(err.source.is_none());
    }
}
