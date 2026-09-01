use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::errors::DomainError;
use crate::shared::ids::{RumPageViewId, RumSessionId, RumSiteId, TenantId};
use crate::shared::policies::TenantOwned;

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
        let path = Self::validate_path(path)?;
        let referrer_path = referrer_path.map(Self::validate_path).transpose()?;
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

    /// Enforces that a path is present, bounded, and free of a query
    /// string or fragment.
    ///
    /// Query strings and fragments are rejected outright rather than
    /// stripped here, because stripping silently would let a caller
    /// believe PII-bearing query params (tokens, emails, etc.) were
    /// handled safely by this layer. The browser SDK (Task D) is
    /// responsible for stripping them *before* the event is ever sent;
    /// this is a defense-in-depth backstop, not the primary control.
    fn validate_path(path: String) -> Result<String, DomainError> {
        let path = path.trim().to_string();
        if path.is_empty() {
            return Err(DomainError::EmptyRumPageViewPath);
        }
        if path.chars().count() > MAX_RUM_PATH_LEN {
            return Err(DomainError::RumPageViewPathTooLong);
        }
        if path.contains('?') || path.contains('#') {
            return Err(DomainError::RumPageViewPathContainsQuery);
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
    fn rejects_path_with_query_string() {
        let (id, t, s, sess) = ids();
        let err = RumPageView::new(
            id,
            t,
            s,
            sess,
            "/dashboard?token=secret".to_string(),
            None,
            RumVitals::default(),
        )
        .unwrap_err();
        assert_eq!(err, DomainError::RumPageViewPathContainsQuery);
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
