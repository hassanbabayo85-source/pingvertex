use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::errors::DomainError;
use crate::shared::ids::{RumPageViewId, RumSessionId, RumSiteId, TenantId};
use crate::shared::policies::TenantOwned;
use crate::shared::url_sanitize::strip_query_and_fragment;

pub const MAX_RUM_PATH_LEN: usize = 2048;

/// Timing + Core Web Vitals payload for a single page view.
///
/// All fields are optional because browser support for each metric
/// varies (e.g. INP is unavailable on older browsers) — the SDK sends
/// whatever it was able to observe rather than dropping the whole event.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct RumVitals {
    pub ttfb_ms: Option<i32>,
    pub fcp_ms: Option<i32>,
    pub lcp_ms: Option<i32>,
    pub inp_ms: Option<i32>,
    pub cls: Option<f64>,
    pub load_time_ms: Option<i32>,
}

impl RumVitals {
    fn validate(&self) -> Result<(), DomainError> {
        let non_negative_ints = [
            self.ttfb_ms,
            self.fcp_ms,
            self.lcp_ms,
            self.inp_ms,
            self.load_time_ms,
        ];
        if non_negative_ints.iter().flatten().any(|v| *v < 0) {
            return Err(DomainError::NegativeRumMetricValue);
        }
        if let Some(cls) = self.cls
            && cls < 0.0
        {
            return Err(DomainError::NegativeRumMetricValue);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RumPageView {
    pub id: RumPageViewId,
    pub tenant_id: TenantId,
    pub site_id: RumSiteId,
    pub session_id: RumSessionId,
    pub path: String,
    pub referrer_path: Option<String>,
    pub vitals: RumVitals,
    pub created_at: DateTime<Utc>,
}

impl RumPageView {
    pub fn new(
        id: RumPageViewId,
        tenant_id: TenantId,
        site_id: RumSiteId,
        session_id: RumSessionId,
        path: String,
        referrer_path: Option<String>,
        vitals: RumVitals,
    ) -> Result<Self, DomainError> {
        let path = Self::normalize_path(path)?;
        let referrer_path = referrer_path.map(Self::normalize_path).transpose()?;
        vitals.validate()?;

        Ok(Self {
            id,
            tenant_id,
            site_id,
            session_id,
            path,
            referrer_path,
            vitals,
            created_at: Utc::now(),
        })
    }

    /// Normalizes a page path (or referrer path) before persistence.
    ///
    /// The path is STRIPPED of its query string and fragment, not
    /// rejected. This is the deliberate behavior change that closes the
    /// "RUM data loss / secret redaction" gap:
    ///
    /// * The previous implementation rejected any path containing `?` or
    ///   `#`. That meant a single legitimate SPA route with a tracking
    ///   parameter, or a hash-based route, caused the ENTIRE beacon to
    ///   be dropped — losing the page view AND every JS/resource error
    ///   that was batched with it. That is the "data loss" half of the
    ///   bug.
    /// * It also meant a client that bypassed the SDK (or an older SDK
    ///   build) could still get `?token=secret` persisted, because the
    ///   only thing standing between the wire and the DB was the
    ///   client-side sanitizer. That is the "secret redaction" half.
    ///
    /// Stripping here — in the domain layer, server-side — means the
    /// invariant "no persisted RUM path contains a query string or
    /// fragment" holds regardless of client behavior. See
    /// [`crate::shared::url_sanitize`] for the shared rule.
    ///
    /// After stripping, an empty result (e.g. the input was literally
    /// `"?token=secret"`) is still rejected: there is no meaningful
    /// page view to record.
    fn normalize_path(path: String) -> Result<String, DomainError> {
        let stripped = strip_query_and_fragment(path.trim());
        let path = stripped.trim().to_string();
        if path.is_empty() {
            return Err(DomainError::EmptyRumPageViewPath);
        }
        if path.chars().count() > MAX_RUM_PATH_LEN {
            return Err(DomainError::RumPageViewPathTooLong);
        }
        Ok(path)
    }
}

impl TenantOwned for RumPageView {
    fn tenant_id(&self) -> TenantId {
        self.tenant_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> (RumPageViewId, TenantId, RumSiteId, RumSessionId) {
        (
            RumPageViewId::new(),
            TenantId::new(),
            RumSiteId::new(),
            RumSessionId::new(),
        )
    }

    #[test]
    fn rejects_empty_path() {
        let (id, t, s, sess) = ids();
        let err = RumPageView::new(id, t, s, sess, "".to_string(), None, RumVitals::default())
            .unwrap_err();
        assert_eq!(err, DomainError::EmptyRumPageViewPath);
    }

    #[test]
    fn strips_query_string_from_path() {
        let (id, t, s, sess) = ids();
        let pv = RumPageView::new(
            id,
            t,
            s,
            sess,
            "/dashboard?token=secret".to_string(),
            None,
            RumVitals::default(),
        )
        .unwrap();
        assert_eq!(pv.path, "/dashboard");
        assert!(!pv.path.contains("secret"));
        assert!(!pv.path.contains('?'));
        assert!(!pv.path.contains('#'));
    }

    #[test]
    fn strips_fragment_from_path() {
        let (id, t, s, sess) = ids();
        let pv = RumPageView::new(
            id,
            t,
            s,
            sess,
            "/dashboard#section".to_string(),
            None,
            RumVitals::default(),
        )
        .unwrap();
        assert_eq!(pv.path, "/dashboard");
        assert!(!pv.path.contains('#'));
    }

    #[test]
    fn strips_query_and_fragment_from_full_url_path() {
        let (id, t, s, sess) = ids();
        let pv = RumPageView::new(
            id,
            t,
            s,
            sess,
            "https://example.com/page?token=secret#fragment".to_string(),
            None,
            RumVitals::default(),
        )
        .unwrap();
        assert_eq!(pv.path, "https://example.com/page");
        assert!(!pv.path.contains("secret"));
        assert!(!pv.path.contains('?'));
        assert!(!pv.path.contains('#'));
    }

    #[test]
    fn strips_query_string_from_referrer_path_too() {
        let (id, t, s, sess) = ids();
        let pv = RumPageView::new(
            id,
            t,
            s,
            sess,
            "/home".to_string(),
            Some("/prev?email=a@b.com".to_string()),
            RumVitals::default(),
        )
        .unwrap();
        assert_eq!(pv.referrer_path.as_deref(), Some("/prev"));
        assert!(!pv.referrer_path.as_deref().unwrap().contains("email"));
    }

    #[test]
    fn path_that_becomes_empty_after_stripping_is_rejected() {
        let (id, t, s, sess) = ids();
        let err = RumPageView::new(
            id,
            t,
            s,
            sess,
            "?token=secret".to_string(),
            None,
            RumVitals::default(),
        )
        .unwrap_err();
        assert_eq!(err, DomainError::EmptyRumPageViewPath);
    }

    #[test]
    fn rejects_negative_vitals() {
        let (id, t, s, sess) = ids();
        let vitals = RumVitals {
            lcp_ms: Some(-1),
            ..Default::default()
        };
        let err = RumPageView::new(id, t, s, sess, "/home".to_string(), None, vitals).unwrap_err();
        assert_eq!(err, DomainError::NegativeRumMetricValue);
    }

    #[test]
    fn accepts_valid_page_view() {
        let (id, t, s, sess) = ids();
        let vitals = RumVitals {
            lcp_ms: Some(1200),
            cls: Some(0.05),
            ..Default::default()
        };
        let pv = RumPageView::new(id, t, s, sess, "/home".to_string(), None, vitals).unwrap();
        assert_eq!(pv.path, "/home");
    }
}
