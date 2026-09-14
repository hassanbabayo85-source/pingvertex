use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::errors::DomainError;
use crate::shared::ids::{RumPageViewId, RumResourceErrorId, RumSessionId, RumSiteId, TenantId};
use crate::shared::policies::TenantOwned;
use crate::shared::url_sanitize::strip_query_and_fragment;

pub const MAX_RUM_RESOURCE_URL_LEN: usize = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RumResourceType {
    Script,
    Stylesheet,
    Image,
    Fetch,
    Other,
}

/// A failed asset or API request observed by the browser (e.g. a 404'd
/// script, a failed `fetch`). Like `RumJsError`, `resource_url` is
/// browser-supplied and untrusted — plain text only on render, never
/// trusted HTML (see Task G security self-check).
///
/// SECURITY: `resource_url` is normalized (query string and fragment
/// stripped) in `new` before the entity is constructed. See
/// [`crate::shared::url_sanitize`] for the shared rule and rationale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RumResourceError {
    pub id: RumResourceErrorId,
    pub tenant_id: TenantId,
    pub site_id: RumSiteId,
    pub session_id: RumSessionId,
    pub page_view_id: Option<RumPageViewId>,
    pub resource_url: String,
    pub resource_type: Option<RumResourceType>,
    pub status_code: Option<i32>,
    pub created_at: DateTime<Utc>,
}

impl RumResourceError {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: RumResourceErrorId,
        tenant_id: TenantId,
        site_id: RumSiteId,
        session_id: RumSessionId,
        page_view_id: Option<RumPageViewId>,
        resource_url: String,
        resource_type: Option<RumResourceType>,
        status_code: Option<i32>,
    ) -> Result<Self, DomainError> {
        // Strip `?query=…` and `#fragment` BEFORE the length check, so a
        // URL whose secret-bearing tail pushes it over the limit is not
        // rejected just because of the part we are about to discard.
        let stripped = strip_query_and_fragment(resource_url.trim());
        let resource_url = stripped.trim().to_string();

        if resource_url.is_empty() {
            return Err(DomainError::EmptyRumResourceUrl);
        }
        if resource_url.chars().count() > MAX_RUM_RESOURCE_URL_LEN {
            return Err(DomainError::RumResourceUrlTooLong);
        }

        Ok(Self {
            id,
            tenant_id,
            site_id,
            session_id,
            page_view_id,
            resource_url,
            resource_type,
            status_code,
            created_at: Utc::now(),
        })
    }
}

impl TenantOwned for RumResourceError {
    fn tenant_id(&self) -> TenantId {
        self.tenant_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_url() {
        let err = RumResourceError::new(
            RumResourceErrorId::new(),
            TenantId::new(),
            RumSiteId::new(),
            RumSessionId::new(),
            None,
            "".to_string(),
            Some(RumResourceType::Script),
            Some(404),
        )
        .unwrap_err();
        assert_eq!(err, DomainError::EmptyRumResourceUrl);
    }

    #[test]
    fn rejects_url_too_long() {
        let err = RumResourceError::new(
            RumResourceErrorId::new(),
            TenantId::new(),
            RumSiteId::new(),
            RumSessionId::new(),
            None,
            "a".repeat(MAX_RUM_RESOURCE_URL_LEN + 1),
            None,
            None,
        )
        .unwrap_err();
        assert_eq!(err, DomainError::RumResourceUrlTooLong);
    }

    #[test]
    fn strips_query_string_from_resource_url() {
        let err = RumResourceError::new(
            RumResourceErrorId::new(),
            TenantId::new(),
            RumSiteId::new(),
            RumSessionId::new(),
            None,
            "https://example.com/script.js?token=secret".to_string(),
            Some(RumResourceType::Script),
            Some(404),
        )
        .unwrap();
        assert_eq!(err.resource_url, "https://example.com/script.js");
        assert!(!err.resource_url.contains("secret"));
        assert!(!err.resource_url.contains('?'));
    }

    #[test]
    fn strips_fragment_from_resource_url() {
        let err = RumResourceError::new(
            RumResourceErrorId::new(),
            TenantId::new(),
            RumSiteId::new(),
            RumSessionId::new(),
            None,
            "https://example.com/script.js#frag".to_string(),
            Some(RumResourceType::Script),
            None,
        )
        .unwrap();
        assert_eq!(err.resource_url, "https://example.com/script.js");
        assert!(!err.resource_url.contains('#'));
    }

    #[test]
    fn strips_query_and_fragment_from_full_resource_url() {
        let err = RumResourceError::new(
            RumResourceErrorId::new(),
            TenantId::new(),
            RumSiteId::new(),
            RumSessionId::new(),
            None,
            "https://example.com/page?token=secret#fragment".to_string(),
            Some(RumResourceType::Fetch),
            Some(500),
        )
        .unwrap();
        assert_eq!(err.resource_url, "https://example.com/page");
        assert!(!err.resource_url.contains("secret"));
        assert!(!err.resource_url.contains('?'));
        assert!(!err.resource_url.contains('#'));
    }

    #[test]
    fn every_named_secret_param_is_stripped_from_resource_url() {
        for param in [
            "?token=t",
            "?code=c",
            "?password=p",
            "?secret=s",
            "?key=k",
            "?email=e",
        ] {
            let url = format!("https://example.com/page{param}");
            let err = RumResourceError::new(
                RumResourceErrorId::new(),
                TenantId::new(),
                RumSiteId::new(),
                RumSessionId::new(),
                None,
                url,
                None,
                None,
            )
            .unwrap();
            assert_eq!(err.resource_url, "https://example.com/page");
        }
    }

    #[test]
    fn resource_url_that_becomes_empty_after_stripping_is_rejected() {
        let err = RumResourceError::new(
            RumResourceErrorId::new(),
            TenantId::new(),
            RumSiteId::new(),
            RumSessionId::new(),
            None,
            "?token=secret".to_string(),
            None,
            None,
        )
        .unwrap_err();
        assert_eq!(err, DomainError::EmptyRumResourceUrl);
    }
}
