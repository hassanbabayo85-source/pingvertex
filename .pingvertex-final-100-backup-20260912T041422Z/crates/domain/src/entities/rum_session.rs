use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::shared::ids::{RumSessionId, RumSiteId, TenantId};
use crate::shared::policies::TenantOwned;

/// Coarse device classification. Deliberately a closed enum, not a raw
/// browser-supplied string, so the dashboard never has to render
/// arbitrary client text for this field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceType {
    Desktop,
    Mobile,
    Tablet,
    Other,
}

/// A single browser visit/tab-session. Intentionally minimal: no cookies,
/// no IP address, no user identity — see Task D/G PII review. `session_key`
/// is an opaque, client-generated random value used only to group this
/// session's page views and errors together.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RumSession {
    pub id: RumSessionId,
    pub tenant_id: TenantId,
    pub site_id: RumSiteId,
    pub session_key: String,
    pub browser_name: Option<String>,
    pub browser_version: Option<String>,
    pub os_name: Option<String>,
    pub device_type: Option<DeviceType>,
    pub country_code: Option<String>,
    pub started_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

impl RumSession {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: RumSessionId,
        tenant_id: TenantId,
        site_id: RumSiteId,
        session_key: String,
        browser_name: Option<String>,
        browser_version: Option<String>,
        os_name: Option<String>,
        device_type: Option<DeviceType>,
        country_code: Option<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id,
            tenant_id,
            site_id,
            session_key,
            browser_name,
            browser_version,
            os_name,
            device_type,
            // Normalized to uppercase ISO-3166-1 alpha-2 if present; the
            // ingestion layer is responsible for populating this only
            // from server-side geo-IP lookups, never from browser input.
            country_code: country_code.map(|c| c.to_uppercase()),
            started_at: now,
            last_seen_at: now,
        }
    }

    /// Bumps `last_seen_at` when a new event arrives for this session.
    pub fn touch(&mut self) {
        self.last_seen_at = Utc::now();
    }
}

impl TenantOwned for RumSession {
    fn tenant_id(&self) -> TenantId {
        self.tenant_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touch_advances_last_seen_at() {
        let mut session = RumSession::new(
            RumSessionId::new(),
            TenantId::new(),
            RumSiteId::new(),
            "sess_abc".to_string(),
            None,
            None,
            None,
            Some(DeviceType::Desktop),
            None,
        );
        let before = session.last_seen_at;
        std::thread::sleep(std::time::Duration::from_millis(5));
        session.touch();
        assert!(session.last_seen_at >= before);
    }

    #[test]
    fn normalizes_country_code_case() {
        let session = RumSession::new(
            RumSessionId::new(),
            TenantId::new(),
            RumSiteId::new(),
            "sess_abc".to_string(),
            None,
            None,
            None,
            None,
            Some("us".to_string()),
        );
        assert_eq!(session.country_code.as_deref(), Some("US"));
    }
}
