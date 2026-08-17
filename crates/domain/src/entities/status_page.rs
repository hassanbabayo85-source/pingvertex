use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::errors::DomainError;
use crate::shared::ids::{MonitorId, StatusPageId, UserId};
use crate::value_objects::{CustomDomain, HexColor, Subdomain};

/// Hard cap on the number of monitors a single status page may display.
///
/// Without this limit, a client could submit an unbounded `monitor_ids`
/// array, which would then be validated/queried/rendered on every public
/// page load — wasted work for a use case where a real status page
/// realistically shows a handful of services.
pub const MAX_STATUS_PAGE_MONITORS: usize = 50;

/// Hard cap on the byte length of a status page's `logo_url`.
const MAX_LOGO_URL_LENGTH: usize = 2048;

/// The verification state of a `custom_domain` attached to a status page.
///
/// A `CustomDomain` value object only proves the string is *syntactically*
/// a domain — it says nothing about ownership. This type tracks the
/// separate, out-of-band DNS TXT record challenge that proves ownership
/// before the domain is allowed to serve traffic. See the SECURITY note on
/// `StatusPage::verify_custom_domain` for why this check is mandatory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CustomDomainVerification {
    /// A custom domain has been requested but ownership has not yet been
    /// confirmed. `token` is the random value the owner must publish as a
    /// DNS TXT record (`pingvertex-verify=<token>`) at their domain.
    /// `requested_at` lets the worker/cleanup job expire stale, never-
    /// verified requests.
    Pending {
        token: String,
        requested_at: DateTime<Utc>,
    },
    /// Ownership has been confirmed via TXT record lookup. Only a domain
    /// in this state may be used for host-header based routing.
    Verified { verified_at: DateTime<Utc> },
}

/// Data required to create a status page.
#[derive(Debug, Clone)]
pub struct CreateStatusPage {
    pub id: StatusPageId,
    pub user_id: UserId,
    pub name: String,
    pub subdomain: Subdomain,
    pub monitor_ids: Vec<MonitorId>,
}

/// A user/team-owned public status page, e.g. served at
/// `acme.pingvertex.com` or a verified custom domain like
/// `status.acme.com`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusPage {
    pub id: StatusPageId,
    pub user_id: UserId,
    pub name: String,
    pub subdomain: Subdomain,
    /// A custom domain the owner wants to serve this page from, together
    /// with its verification state. `None` until the owner requests one.
    ///
    /// SECURITY: code that resolves a `Host` header to a status page MUST
    /// only match against domains where this is
    /// `Some((domain, CustomDomainVerification::Verified { .. }))` — never
    /// `Pending`. See `verify_custom_domain` below.
    pub custom_domain: Option<(CustomDomain, CustomDomainVerification)>,
    pub logo_url: Option<String>,
    pub primary_color: Option<HexColor>,
    pub monitor_ids: Vec<MonitorId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl StatusPage {
    /// Creates a new status page. `custom_domain`, `logo_url`, and
    /// `primary_color` all start unset — the owner attaches these via the
    /// dedicated methods below, each of which validates its input.
    pub fn create(input: CreateStatusPage) -> Result<Self, DomainError> {
        if input.monitor_ids.len() > MAX_STATUS_PAGE_MONITORS {
            return Err(DomainError::TooManyStatusPageMonitors);
        }

        let name = input.name.trim();
        if name.is_empty() {
            return Err(DomainError::EmptyStatusPageName);
        }
        if name.chars().count() > 100 {
            return Err(DomainError::StatusPageNameTooLong);
        }

        let now = Utc::now();

        Ok(Self {
            id: input.id,
            user_id: input.user_id,
            name: name.to_owned(),
            subdomain: input.subdomain,
            custom_domain: None,
            logo_url: None,
            primary_color: None,
            monitor_ids: input.monitor_ids,
            created_at: now,
            updated_at: now,
        })
    }

    /// Updates the display name.
    pub fn rename(&mut self, name: impl Into<String>) -> Result<(), DomainError> {
        let name = name.into();
        let name = name.trim();

        if name.is_empty() {
            return Err(DomainError::EmptyStatusPageName);
        }
        if name.chars().count() > 100 {
            return Err(DomainError::StatusPageNameTooLong);
        }

        self.name = name.to_owned();
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Replaces the set of monitors shown on this page.
    pub fn set_monitors(&mut self, monitor_ids: Vec<MonitorId>) -> Result<(), DomainError> {
        if monitor_ids.len() > MAX_STATUS_PAGE_MONITORS {
            return Err(DomainError::TooManyStatusPageMonitors);
        }

        self.monitor_ids = monitor_ids;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Sets the branding logo URL. `Url`-validated at the boundary where
    /// the file is actually uploaded (see infra logo-upload handler);
    /// here we only bound the length of the stored URL, since this value
    /// is expected to be a CDN URL returned by that upload step, not
    /// arbitrary user-supplied text.
    pub fn set_logo_url(&mut self, logo_url: Option<String>) -> Result<(), DomainError> {
        if let Some(ref url) = logo_url {
            if url.len() > MAX_LOGO_URL_LENGTH {
                return Err(DomainError::LogoUrlTooLong);
            }
            if url::Url::parse(url).is_err() {
                return Err(DomainError::InvalidLogoUrl);
            }
        }

        self.logo_url = logo_url;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Sets the primary brand color.
    pub fn set_primary_color(&mut self, color: Option<HexColor>) {
        self.primary_color = color;
        self.updated_at = Utc::now();
    }

    /// Begins attaching a custom domain: generates a random verification
    /// token the owner must publish as a DNS TXT record
    /// (`pingvertex-verify=<token>`) before the domain can serve traffic.
    ///
    /// Always transitions into `Pending` — even if a domain was previously
    /// `Verified`, changing the domain requires re-proving ownership of
    /// the *new* value.
    pub fn request_custom_domain(&mut self, domain: CustomDomain) {
        // Two concatenated UUIDv4s (no hyphens) give a 64-character
        // random, URL-safe token — plenty of entropy for a DNS TXT
        // record verification challenge, and reuses `uuid` (already a
        // domain dependency) instead of pulling in a new `rand`
        // dependency just for this.
        let token = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );

        self.custom_domain = Some((
            domain,
            CustomDomainVerification::Pending {
                token,
                requested_at: Utc::now(),
            },
        ));
        self.updated_at = Utc::now();
    }

    /// Returns the pending verification token for the current custom
    /// domain, if any is awaiting verification. Infra's TXT-record
    /// verification worker reads this to know what value to look for.
    pub fn pending_verification_token(&self) -> Option<&str> {
        match &self.custom_domain {
            Some((_, CustomDomainVerification::Pending { token, .. })) => Some(token.as_str()),
            _ => None,
        }
    }

    /// Marks the current custom domain as verified.
    ///
    /// SECURITY: this must ONLY be called after the caller (the TXT
    /// verification worker in infra) has independently confirmed, via a
    /// live DNS TXT record lookup, that the domain's TXT records contain
    /// `pingvertex-verify=<token>` for the token returned by
    /// `pending_verification_token`. This method itself performs NO I/O
    /// and trusts its caller completely — the domain layer cannot resolve
    /// DNS. Calling this without having actually checked the TXT record
    /// would let anyone claim ownership of any domain and hijack its
    /// status-page traffic (see task-level SECURITY REQUIREMENTS).
    pub fn mark_custom_domain_verified(&mut self) -> Result<(), DomainError> {
        let Some((domain, verification)) = self.custom_domain.take() else {
            return Err(DomainError::EmptyCustomDomain);
        };

        match verification {
            CustomDomainVerification::Pending { .. } => {
                self.custom_domain = Some((
                    domain,
                    CustomDomainVerification::Verified {
                        verified_at: Utc::now(),
                    },
                ));
                self.updated_at = Utc::now();
                Ok(())
            }
            already_verified @ CustomDomainVerification::Verified { .. } => {
                // Idempotent: already verified, nothing to do.
                self.custom_domain = Some((domain, already_verified));
                Ok(())
            }
        }
    }

    /// Removes the custom domain entirely (falls back to the subdomain).
    pub fn remove_custom_domain(&mut self) {
        self.custom_domain = None;
        self.updated_at = Utc::now();
    }

    /// Returns the verified custom domain string, if any. This is the
    /// ONLY accessor API-layer host-routing code should use — it makes it
    /// structurally impossible to accidentally match against a
    /// still-`Pending` domain.
    pub fn verified_custom_domain(&self) -> Option<&str> {
        match &self.custom_domain {
            Some((domain, CustomDomainVerification::Verified { .. })) => Some(domain.as_str()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> StatusPage {
        StatusPage::create(CreateStatusPage {
            id: StatusPageId::new(),
            user_id: UserId::new(),
            name: "Acme Status".to_string(),
            subdomain: Subdomain::new("acme").unwrap(),
            monitor_ids: vec![],
        })
        .unwrap()
    }

    #[test]
    fn create_rejects_empty_name() {
        let result = StatusPage::create(CreateStatusPage {
            id: StatusPageId::new(),
            user_id: UserId::new(),
            name: "   ".to_string(),
            subdomain: Subdomain::new("acme").unwrap(),
            monitor_ids: vec![],
        });
        assert_eq!(result.err(), Some(DomainError::EmptyStatusPageName));
    }

    #[test]
    fn create_rejects_too_many_monitors() {
        let monitor_ids = (0..51).map(|_| MonitorId::new()).collect();
        let result = StatusPage::create(CreateStatusPage {
            id: StatusPageId::new(),
            user_id: UserId::new(),
            name: "Acme".to_string(),
            subdomain: Subdomain::new("acme").unwrap(),
            monitor_ids,
        });
        assert_eq!(result.err(), Some(DomainError::TooManyStatusPageMonitors));
    }

    #[test]
    fn custom_domain_starts_pending_and_requires_explicit_verification() {
        let mut page = sample();
        assert!(page.verified_custom_domain().is_none());

        page.request_custom_domain(CustomDomain::new("status.acme.com").unwrap());

        // Not verified yet, even though a domain is attached.
        assert!(page.verified_custom_domain().is_none());
        assert!(page.pending_verification_token().is_some());
    }

    #[test]
    fn verifying_moves_domain_to_verified_state() {
        let mut page = sample();
        page.request_custom_domain(CustomDomain::new("status.acme.com").unwrap());

        page.mark_custom_domain_verified().unwrap();

        assert_eq!(page.verified_custom_domain(), Some("status.acme.com"));
        assert!(page.pending_verification_token().is_none());
    }

    #[test]
    fn mark_verified_without_a_domain_errors() {
        let mut page = sample();
        assert!(page.mark_custom_domain_verified().is_err());
    }

    #[test]
    fn changing_domain_resets_to_pending() {
        let mut page = sample();
        page.request_custom_domain(CustomDomain::new("status.acme.com").unwrap());
        page.mark_custom_domain_verified().unwrap();
        assert!(page.verified_custom_domain().is_some());

        // Attaching a NEW domain must require re-verification — it must
        // not inherit the previous domain's Verified state.
        page.request_custom_domain(CustomDomain::new("status2.acme.com").unwrap());
        assert!(page.verified_custom_domain().is_none());
    }

    #[test]
    fn set_logo_url_rejects_invalid_url() {
        let mut page = sample();
        let result = page.set_logo_url(Some("not a url".to_string()));
        assert_eq!(result, Err(DomainError::InvalidLogoUrl));
    }

    #[test]
    fn set_logo_url_accepts_valid_url() {
        let mut page = sample();
        page.set_logo_url(Some("https://cdn.pingvertex.com/logo.png".to_string()))
            .unwrap();
        assert_eq!(
            page.logo_url.as_deref(),
            Some("https://cdn.pingvertex.com/logo.png")
        );
    }
}
