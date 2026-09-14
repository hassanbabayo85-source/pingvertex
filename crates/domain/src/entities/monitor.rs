use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::shared::ids::{MonitorId, TenantId, UserId};
use crate::shared::policies::{Ownable, TenantOwned};

use crate::{
    enums::{HttpMethod, MonitorKind},
    errors::DomainError,
    value_objects::{
        CheckInterval, ExpectedStatus, HeartbeatToken, MonitorName, RegionCode, Timeout,
    },
};

/// Data required to create a monitor.
#[derive(Debug, Clone)]
pub struct CreateMonitor {
    pub id: MonitorId,
    pub user_id: UserId,
    pub tenant_id: TenantId,
    pub name: MonitorName,
    pub url: String,
    pub method: HttpMethod,
    pub expected_status: u16,
    pub interval_seconds: u32,
    pub timeout_seconds: u32,
    /// Regions this monitor should be checked from.
    /// Empty means "use the default single region" (`us-east`).
    pub regions: Vec<RegionCode>,
    /// The kind of check this monitor performs. Defaults to `Http` for
    /// backward compatibility — same pattern as `regions` defaulting to
    /// the single default region when empty.
    pub kind: MonitorKind,
    pub tcp_port: Option<u16>,
    pub dns_record_type: Option<String>,
    pub dns_expected_value: Option<String>,
    pub heartbeat_token: Option<HeartbeatToken>,
    pub heartbeat_interval_seconds: Option<u32>,
    pub heartbeat_grace_period_seconds: Option<u32>,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
}

/// A monitor configured by a user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Monitor {
    pub assertions: Vec<crate::entities::Assertion>,
    pub id: MonitorId,
    pub user_id: UserId,
    pub tenant_id: TenantId,
    pub name: MonitorName,
    pub url: String,
    pub method: HttpMethod,
    pub expected_status: u16,
    pub interval_seconds: u32,
    pub timeout_seconds: u32,
    pub enabled: bool,
    /// Whether this monitor is exposed on the unauthenticated public status
    /// page. Defaults to `false` — an owner must explicitly opt in.
    pub is_public: bool,
    /// Regions this monitor is checked from. Never empty after
    /// `Monitor::create`; defaults to `[us-east]` for backward compatibility.
    pub regions: Vec<RegionCode>,
    pub ssl_check_enabled: bool,
    pub ssl_expiry_warning_days: u32,
    /// The kind of check this monitor performs. Defaults to `Http` for
    /// every monitor created before this field existed.
    pub kind: MonitorKind,
    pub tcp_port: Option<u16>,
    pub dns_record_type: Option<String>,
    pub dns_expected_value: Option<String>,
    pub heartbeat_token: Option<HeartbeatToken>,
    pub heartbeat_interval_seconds: Option<u32>,
    pub heartbeat_grace_period_seconds: Option<u32>,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Monitor {
    pub const MAX_ASSERTIONS: usize = 20;

    /// PART 9 — hard cap on the number of regions a monitor may be checked from.
    /// Mirrors `ALLOWED_REGION_CODES.len()` so an attacker cannot force the API
    /// to persist an unbounded `Vec<RegionCode>`.
    pub const MAX_REGIONS_PER_MONITOR: usize = 4;

    /// PART 9 — hard cap on the byte length of a monitor's DNS expected value.
    pub const MAX_DNS_EXPECTED_VALUE_LENGTH: usize = 2048;

    /// PART 9 — hard cap on the byte length of a monitor's DNS record type.
    /// (The set of valid record types is tiny — `A`, `AAAA`, `CNAME`, `MX`,
    /// `TXT`, `NS`, `SOA`, `PTR`, `SRV`, `CAA` — 32 bytes is generous and
    /// still bounds the field against abuse. TCP's only string field is the
    /// target `Host`, which is already capped at 253 bytes by `Host::new`.)
    pub const MAX_DNS_RECORD_TYPE_LENGTH: usize = 32;

    /// PART 9 — minimum allowed heartbeat interval, in seconds.
    pub const HEARTBEAT_INTERVAL_MIN_SECS: u32 = 30;
    /// PART 9 — maximum allowed heartbeat interval, in seconds.
    pub const HEARTBEAT_INTERVAL_MAX_SECS: u32 = 86_400;
    /// PART 9 — minimum allowed heartbeat grace period, in seconds.
    pub const HEARTBEAT_GRACE_MIN_SECS: u32 = 30;
    /// PART 9 — maximum allowed heartbeat grace period, in seconds.
    pub const HEARTBEAT_GRACE_MAX_SECS: u32 = 86_400;

    /// Creates a new monitor.
    ///
    /// Validates that the fields required by `input.kind` are present
    /// (e.g. a `Tcp` monitor must have `tcp_port` set). `Http` and `Flow`
    /// monitors have no kind-specific required fields on `Monitor` itself
    /// (a `Flow` monitor's steps live in the separate `ApiFlow` aggregate,
    /// see `MonitorKind::Flow`'s doc comment) — existing callers that
    /// never set `kind` (it defaults to `Http`) always succeed here,
    /// preserving full backward compatibility.
    /// PART 9 — validate that a client-supplied DNS expected value is within
    /// the hard byte cap.
    fn validate_dns_expected_value(value: Option<&str>) -> Result<(), DomainError> {
        if let Some(v) = value
            && v.len() > Self::MAX_DNS_EXPECTED_VALUE_LENGTH
        {
            return Err(DomainError::DnsExpectedValueTooLong);
        }
        Ok(())
    }

    /// PART 9 — validate that a client-supplied DNS record type is within
    /// the hard byte cap.
    fn validate_dns_record_type(value: Option<&str>) -> Result<(), DomainError> {
        if let Some(v) = value
            && v.len() > Self::MAX_DNS_RECORD_TYPE_LENGTH
        {
            return Err(DomainError::DnsRecordTypeTooLong);
        }
        Ok(())
    }

    /// PART 9 — cap the heartbeat interval to a bounded range.
    fn validate_heartbeat_interval(value: Option<u32>) -> Result<(), DomainError> {
        if let Some(v) = value {
            if v < Self::HEARTBEAT_INTERVAL_MIN_SECS {
                return Err(DomainError::HeartbeatIntervalTooSmall);
            }
            if v > Self::HEARTBEAT_INTERVAL_MAX_SECS {
                return Err(DomainError::HeartbeatIntervalTooLarge);
            }
        }
        Ok(())
    }

    /// PART 9 — cap the heartbeat grace period to a bounded range.
    fn validate_heartbeat_grace_period(value: Option<u32>) -> Result<(), DomainError> {
        if let Some(v) = value {
            if v < Self::HEARTBEAT_GRACE_MIN_SECS {
                return Err(DomainError::HeartbeatGracePeriodTooSmall);
            }
            if v > Self::HEARTBEAT_GRACE_MAX_SECS {
                return Err(DomainError::HeartbeatGracePeriodTooLarge);
            }
        }
        Ok(())
    }

    /// PART 9 — cap the number of regions and deduplicate them (preserving
    /// insertion order) so persistence never writes the same region twice.
    fn validate_and_dedup_regions(
        regions: Vec<RegionCode>,
    ) -> Result<Vec<RegionCode>, DomainError> {
        if regions.len() > Self::MAX_REGIONS_PER_MONITOR {
            return Err(DomainError::TooManyRegionsPerMonitor);
        }
        let mut deduped: Vec<RegionCode> = Vec::with_capacity(regions.len());
        for region in regions {
            if !deduped.iter().any(|r| r.as_str() == region.as_str()) {
                deduped.push(region);
            }
        }
        Ok(deduped)
    }

    pub fn create(input: CreateMonitor) -> Result<Self, DomainError> {
        Self::validate_kind_fields(
            input.kind,
            input.tcp_port,
            input.dns_record_type.as_deref(),
            input.heartbeat_token.as_ref(),
            input.heartbeat_interval_seconds,
            input.heartbeat_grace_period_seconds,
        )?;

        // PART 9 — validate attacker-controlled strings and cap collections.
        Self::validate_dns_expected_value(input.dns_expected_value.as_deref())?;
        Self::validate_dns_record_type(input.dns_record_type.as_deref())?;
        Self::validate_heartbeat_interval(input.heartbeat_interval_seconds)?;
        Self::validate_heartbeat_grace_period(input.heartbeat_grace_period_seconds)?;
        let mut regions = Self::validate_and_dedup_regions(input.regions)?;
        if regions.is_empty() {
            regions.push(RegionCode::default_region());
        }

        let now = Utc::now();

        Ok(Self {
            id: input.id,
            user_id: input.user_id,
            tenant_id: input.tenant_id,
            name: input.name,
            url: input.url,
            method: input.method,
            expected_status: input.expected_status,
            interval_seconds: input.interval_seconds,
            timeout_seconds: input.timeout_seconds,
            enabled: true,
            is_public: false,
            regions,
            ssl_check_enabled: true,
            ssl_expiry_warning_days: 14,
            kind: input.kind,
            tcp_port: input.tcp_port,
            dns_record_type: input.dns_record_type,
            dns_expected_value: input.dns_expected_value,
            heartbeat_token: input.heartbeat_token,
            heartbeat_interval_seconds: input.heartbeat_interval_seconds,
            heartbeat_grace_period_seconds: input.heartbeat_grace_period_seconds,
            last_heartbeat_at: input.last_heartbeat_at,
            created_at: now,
            assertions: Vec::new(),
            updated_at: now,
        })
    }

    /// Validates that the fields required by `kind` are present.
    fn validate_kind_fields(
        kind: MonitorKind,
        tcp_port: Option<u16>,
        dns_record_type: Option<&str>,
        heartbeat_token: Option<&HeartbeatToken>,
        heartbeat_interval_seconds: Option<u32>,
        heartbeat_grace_period_seconds: Option<u32>,
    ) -> Result<(), DomainError> {
        match kind {
            MonitorKind::Http => Ok(()),
            // A Flow monitor's actual request steps live in the separate
            // `ApiFlow` aggregate (created independently via
            // `CreateApiFlowUseCase`, keyed by this monitor's id) — the
            // `Monitor` row itself has no flow-specific required fields,
            // exactly like `Http`.
            MonitorKind::Flow => Ok(()),
            MonitorKind::Tcp => {
                if tcp_port.is_none() {
                    return Err(DomainError::InvalidMonitorKindFields(
                        "tcp_port is required for a Tcp monitor".into(),
                    ));
                }
                Ok(())
            }
            MonitorKind::Dns => {
                if dns_record_type.is_none() {
                    return Err(DomainError::InvalidMonitorKindFields(
                        "dns_record_type is required for a Dns monitor".into(),
                    ));
                }
                Ok(())
            }
            MonitorKind::Heartbeat => {
                if heartbeat_token.is_none() {
                    return Err(DomainError::InvalidMonitorKindFields(
                        "heartbeat_token is required for a Heartbeat monitor".into(),
                    ));
                }
                if heartbeat_interval_seconds.is_none() {
                    return Err(DomainError::InvalidMonitorKindFields(
                        "heartbeat_interval_seconds is required for a Heartbeat monitor".into(),
                    ));
                }
                if heartbeat_grace_period_seconds.is_none() {
                    return Err(DomainError::InvalidMonitorKindFields(
                        "heartbeat_grace_period_seconds is required for a Heartbeat monitor".into(),
                    ));
                }
                Ok(())
            }
        }
    }

    /// Updates monitor configuration.
    ///
    /// IMPORTANT: `target` is the already-validated target string. The
    /// caller is responsible for validating it as a `Url` (for HTTP/HTTPS)
    /// or `Host` (for TCP/DNS) before calling this method. This method
    /// does NOT validate the target again — it only assigns it.
    pub fn update(
        &mut self,
        name: MonitorName,
        target: String,
        method: HttpMethod,
        expected_status: ExpectedStatus,
        interval: CheckInterval,
        timeout: Timeout,
    ) {
        self.name = name;
        self.url = target;
        self.method = method;
        self.expected_status = expected_status.into();
        self.interval_seconds = interval.into();
        self.timeout_seconds = timeout.into();
        self.updated_at = Utc::now();
    }

    /// Updates the kind-specific configuration of a monitor (e.g. switching
    /// a monitor from `Http` to `Tcp`, or changing its `tcp_port`).
    ///
    /// Validates that the fields required by `kind` are present, mirroring
    /// `Monitor::create`'s validation. If `kind` is `Heartbeat` and the
    /// monitor does not already have a `heartbeat_token` (e.g. it is being
    /// switched to `Heartbeat` for the first time), a new token is
    /// generated. An existing heartbeat token is always preserved so a
    /// monitor that stays `Heartbeat` keeps the same ping URL the user
    /// already configured elsewhere.
    pub fn update_kind_fields(
        &mut self,
        kind: MonitorKind,
        tcp_port: Option<u16>,
        dns_record_type: Option<String>,
        dns_expected_value: Option<String>,
        heartbeat_interval_seconds: Option<u32>,
        heartbeat_grace_period_seconds: Option<u32>,
    ) -> Result<(), DomainError> {
        let heartbeat_token = if kind == MonitorKind::Heartbeat {
            Some(
                self.heartbeat_token
                    .clone()
                    .unwrap_or_else(HeartbeatToken::generate),
            )
        } else {
            self.heartbeat_token.clone()
        };

        Self::validate_kind_fields(
            kind,
            tcp_port,
            dns_record_type.as_deref(),
            heartbeat_token.as_ref(),
            heartbeat_interval_seconds,
            heartbeat_grace_period_seconds,
        )?;

        Self::validate_dns_expected_value(dns_expected_value.as_deref())?;
        Self::validate_dns_record_type(dns_record_type.as_deref())?;
        Self::validate_heartbeat_interval(heartbeat_interval_seconds)?;
        Self::validate_heartbeat_grace_period(heartbeat_grace_period_seconds)?;

        self.kind = kind;
        self.tcp_port = tcp_port;
        self.dns_record_type = dns_record_type;
        self.dns_expected_value = dns_expected_value;
        self.heartbeat_token = heartbeat_token;
        self.heartbeat_interval_seconds = heartbeat_interval_seconds;
        self.heartbeat_grace_period_seconds = heartbeat_grace_period_seconds;
        self.updated_at = Utc::now();

        Ok(())
    }

    pub fn enable(&mut self) {
        self.enabled = true;
        self.updated_at = Utc::now();
    }

    pub fn disable(&mut self) {
        self.enabled = false;
        self.updated_at = Utc::now();
    }

    /// Records a heartbeat ping, updating `last_heartbeat_at` to now.
    pub fn record_heartbeat(&mut self) {
        let now = Utc::now();
        self.last_heartbeat_at = Some(now);
        self.updated_at = now;
    }

    /// Returns all assertions as a read-only slice.
    pub fn assertions(&self) -> &[crate::entities::Assertion] {
        &self.assertions
    }

    pub fn add_assertion(&mut self, assertion: crate::entities::Assertion) -> Result<(), String> {
        if self.assertions.len() >= Self::MAX_ASSERTIONS {
            return Err(format!(
                "A monitor cannot have more than {} assertions.",
                Self::MAX_ASSERTIONS
            ));
        }

        if self.assertions.iter().any(|a| a.id == assertion.id) {
            return Err("An assertion with the same id already exists.".into());
        }

        self.assertions.push(assertion);
        self.updated_at = Utc::now();

        Ok(())
    }

    pub fn remove_assertion(
        &mut self,
        assertion_id: crate::shared::ids::AssertionId,
    ) -> Result<(), String> {
        if let Some(index) = self.assertions.iter().position(|a| a.id == assertion_id) {
            self.assertions.remove(index);
            self.updated_at = Utc::now();
            Ok(())
        } else {
            Err("Assertion not found.".into())
        }
    }

    pub fn update_assertion(
        &mut self,
        assertion: crate::entities::Assertion,
    ) -> Result<(), String> {
        if let Some(existing) = self.assertions.iter_mut().find(|a| a.id == assertion.id) {
            *existing = assertion;
            self.updated_at = Utc::now();
            Ok(())
        } else {
            Err("Assertion not found.".into())
        }
    }

    pub fn reorder_assertions(
        &mut self,
        ordered_ids: &[crate::shared::ids::AssertionId],
    ) -> Result<(), String> {
        if ordered_ids.len() != self.assertions.len() {
            return Err("Invalid assertion ordering.".into());
        }

        let mut reordered = Vec::with_capacity(self.assertions.len());

        for id in ordered_ids {
            let assertion = self
                .assertions
                .iter()
                .find(|a| a.id == *id)
                .cloned()
                .ok_or_else(|| "Invalid assertion ordering.".to_string())?;

            reordered.push(assertion);
        }

        self.assertions = reordered;
        self.updated_at = Utc::now();

        Ok(())
    }
}

impl TenantOwned for Monitor {
    fn tenant_id(&self) -> TenantId {
        self.tenant_id
    }
}

impl Ownable for Monitor {
    fn owner_id(&self) -> UserId {
        self.user_id
    }
}

#[cfg(test)]
mod part9_tests {
    use super::*;
    use crate::enums::{HttpMethod, MonitorKind};
    use crate::shared::ids::{MonitorId, TenantId, UserId};
    use crate::value_objects::MonitorName;

    fn rc(s: &str) -> RegionCode {
        RegionCode::new(s).unwrap()
    }

    fn base_input(regions: Vec<RegionCode>) -> CreateMonitor {
        CreateMonitor {
            id: MonitorId::new(),
            user_id: UserId::new(),
            tenant_id: TenantId::new(),
            name: MonitorName::new("test").unwrap(),
            url: "https://example.com".to_string(),
            method: HttpMethod::Get,
            expected_status: 200,
            interval_seconds: 60,
            timeout_seconds: 10,
            regions,
            kind: MonitorKind::Http,
            tcp_port: None,
            dns_record_type: None,
            dns_expected_value: None,
            heartbeat_token: None,
            heartbeat_interval_seconds: None,
            heartbeat_grace_period_seconds: None,
            last_heartbeat_at: None,
        }
    }

    #[test]
    fn accepts_exactly_max_unique_regions() {
        let regions = vec![
            rc("us-east"),
            rc("eu-west"),
            rc("af-west"),
            rc("ap-southeast"),
        ];
        assert_eq!(regions.len(), Monitor::MAX_REGIONS_PER_MONITOR);
        let monitor = Monitor::create(base_input(regions)).unwrap();
        assert_eq!(monitor.regions.len(), 4);
    }

    #[test]
    fn rejects_max_plus_one_regions() {
        let regions = vec![
            rc("us-east"),
            rc("us-east"),
            rc("us-east"),
            rc("us-east"),
            rc("us-east"),
        ];
        assert_eq!(regions.len(), Monitor::MAX_REGIONS_PER_MONITOR + 1);
        let err = Monitor::create(base_input(regions)).unwrap_err();
        assert_eq!(err, DomainError::TooManyRegionsPerMonitor);
    }

    #[test]
    fn deduplicates_regions_before_persistence() {
        let regions = vec![rc("us-east"), rc("us-east"), rc("eu-west"), rc("us-east")];
        let monitor = Monitor::create(base_input(regions)).unwrap();
        assert_eq!(monitor.regions.len(), 2);
        assert_eq!(monitor.regions[0].as_str(), "us-east");
        assert_eq!(monitor.regions[1].as_str(), "eu-west");
    }

    #[test]
    fn empty_regions_defaults_to_single_default_region() {
        let monitor = Monitor::create(base_input(vec![])).unwrap();
        assert_eq!(monitor.regions.len(), 1);
        assert_eq!(monitor.regions[0].as_str(), "us-east");
    }

    #[test]
    fn accepts_dns_expected_value_at_max() {
        let mut input = base_input(vec![]);
        input.dns_expected_value = Some("a".repeat(Monitor::MAX_DNS_EXPECTED_VALUE_LENGTH));
        assert!(Monitor::create(input).is_ok());
    }

    #[test]
    fn rejects_dns_expected_value_over_max() {
        let mut input = base_input(vec![]);
        input.dns_expected_value =
            Some("a".repeat(Monitor::MAX_DNS_EXPECTED_VALUE_LENGTH + 1));
        assert_eq!(
            Monitor::create(input).unwrap_err(),
            DomainError::DnsExpectedValueTooLong
        );
    }

    #[test]
    fn rejects_dns_record_type_over_max() {
        let mut input = base_input(vec![]);
        input.dns_record_type = Some("a".repeat(Monitor::MAX_DNS_RECORD_TYPE_LENGTH + 1));
        assert_eq!(
            Monitor::create(input).unwrap_err(),
            DomainError::DnsRecordTypeTooLong
        );
    }

    #[test]
    fn accepts_heartbeat_interval_at_boundaries() {
        let mut input = base_input(vec![]);
        input.heartbeat_interval_seconds = Some(Monitor::HEARTBEAT_INTERVAL_MIN_SECS);
        assert!(Monitor::create(input).is_ok());

        let mut input = base_input(vec![]);
        input.heartbeat_interval_seconds = Some(Monitor::HEARTBEAT_INTERVAL_MAX_SECS);
        assert!(Monitor::create(input).is_ok());
    }

    #[test]
    fn rejects_heartbeat_interval_below_min() {
        let mut input = base_input(vec![]);
        input.heartbeat_interval_seconds = Some(Monitor::HEARTBEAT_INTERVAL_MIN_SECS - 1);
        assert_eq!(
            Monitor::create(input).unwrap_err(),
            DomainError::HeartbeatIntervalTooSmall
        );
    }

    #[test]
    fn rejects_heartbeat_interval_above_max() {
        let mut input = base_input(vec![]);
        input.heartbeat_interval_seconds = Some(Monitor::HEARTBEAT_INTERVAL_MAX_SECS + 1);
        assert_eq!(
            Monitor::create(input).unwrap_err(),
            DomainError::HeartbeatIntervalTooLarge
        );
    }

    #[test]
    fn rejects_heartbeat_grace_period_below_min() {
        let mut input = base_input(vec![]);
        input.heartbeat_grace_period_seconds = Some(Monitor::HEARTBEAT_GRACE_MIN_SECS - 1);
        assert_eq!(
            Monitor::create(input).unwrap_err(),
            DomainError::HeartbeatGracePeriodTooSmall
        );
    }

    #[test]
    fn rejects_heartbeat_grace_period_above_max() {
        let mut input = base_input(vec![]);
        input.heartbeat_grace_period_seconds = Some(Monitor::HEARTBEAT_GRACE_MAX_SECS + 1);
        assert_eq!(
            Monitor::create(input).unwrap_err(),
            DomainError::HeartbeatGracePeriodTooLarge
        );
    }
}
