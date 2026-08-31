use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::DomainError;
use crate::shared::ids::{RumSiteId, TenantId};
use crate::shared::policies::TenantOwned;

pub const MAX_RUM_SITE_NAME_LEN: usize = 200;
pub const MAX_RUM_SITE_DOMAIN_LEN: usize = 255;

/// A site (property) that has RUM instrumentation installed.
///
/// `site_key` is the PUBLIC, non-secret identifier handed to the browser
/// SDK. It identifies which site a beacon belongs to; it is not a
/// credential and must never be treated as one (see Task D/C notes).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RumSite {
    pub id: RumSiteId,
    pub tenant_id: TenantId,
    pub site_key: String,
    pub name: String,
    pub domain: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl RumSite {
    pub fn new(
        id: RumSiteId,
        tenant_id: TenantId,
        name: String,
        domain: String,
    ) -> Result<Self, DomainError> {
        let name = name.trim().to_string();
        let domain = domain.trim().to_string();

        if name.is_empty() {
            return Err(DomainError::EmptyRumSiteName);
        }
        if name.chars().count() > MAX_RUM_SITE_NAME_LEN {
            return Err(DomainError::RumSiteNameTooLong);
        }
        if domain.is_empty() {
            return Err(DomainError::EmptyRumSiteDomain);
        }
        if domain.chars().count() > MAX_RUM_SITE_DOMAIN_LEN {
            return Err(DomainError::RumSiteDomainTooLong);
        }

        let now = Utc::now();

        Ok(Self {
            id,
            tenant_id,
            // Generated server-side only; never accepted from a client
            // request. Callers that need a fresh key should use
            // `generate_site_key`.
            site_key: Self::generate_site_key(),
            name,
            domain,
            created_at: now,
            updated_at: now,
        })
    }

    /// Generates a new random, public, non-secret site identifier.
    ///
    /// This is deliberately just a UUID (not a signed token, not derived
    /// from any secret) — its only job is to disambiguate which site a
    /// beacon belongs to. Authorization for *reading* RUM data is always
    /// done via the authenticated dashboard API and tenant_id, never via
    /// possession of a site_key.
    pub fn generate_site_key() -> String {
        format!("rum_{}", Uuid::new_v4().simple())
    }
}

impl TenantOwned for RumSite {
    fn tenant_id(&self) -> TenantId {
        self.tenant_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tenant() -> TenantId {
        TenantId::new()
    }

    #[test]
    fn rejects_empty_name() {
        let err = RumSite::new(
            RumSiteId::new(),
            tenant(),
            "   ".to_string(),
            "example.com".to_string(),
        )
        .unwrap_err();
        assert_eq!(err, DomainError::EmptyRumSiteName);
    }

    #[test]
    fn rejects_empty_domain() {
        let err = RumSite::new(
            RumSiteId::new(),
            tenant(),
            "My Site".to_string(),
            "   ".to_string(),
        )
        .unwrap_err();
        assert_eq!(err, DomainError::EmptyRumSiteDomain);
    }

    #[test]
    fn rejects_name_too_long() {
        let err = RumSite::new(
            RumSiteId::new(),
            tenant(),
            "a".repeat(MAX_RUM_SITE_NAME_LEN + 1),
            "example.com".to_string(),
        )
        .unwrap_err();
        assert_eq!(err, DomainError::RumSiteNameTooLong);
    }

    #[test]
    fn generates_non_empty_public_site_key() {
        let site = RumSite::new(
            RumSiteId::new(),
            tenant(),
            "My Site".to_string(),
            "example.com".to_string(),
        )
        .unwrap();
        assert!(site.site_key.starts_with("rum_"));
    }

    #[test]
    fn tenant_owned_matches_constructor_tenant() {
        let t = tenant();
        let site = RumSite::new(
            RumSiteId::new(),
            t,
            "My Site".to_string(),
            "example.com".to_string(),
        )
        .unwrap();
        assert_eq!(site.tenant_id(), t);
    }
}
