use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::errors::DomainError;
use crate::shared::ids::{RumPageViewId, RumResourceErrorId, RumSessionId, RumSiteId, TenantId};
use crate::shared::policies::TenantOwned;

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
        let resource_url = resource_url.trim().to_string();
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
}
