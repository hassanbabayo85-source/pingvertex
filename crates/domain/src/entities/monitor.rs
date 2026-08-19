use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::shared::ids::{MonitorId, TenantId, UserId};
use crate::shared::policies::{Ownable, TenantOwned};

use crate::{
    enums::{HttpMethod, MonitorKind},
    errors::DomainError,
    value_objects::{
        CheckInterval, ExpectedStatus, HeartbeatToken, MonitorName, RegionCode, Timeout, Url,
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

    /// Creates a new monitor.
    ///
    /// Validates that the fields required by `input.kind` are present
    /// (e.g. a `Tcp` monitor must have `tcp_port` set). `Http` monitors
    /// have no kind-specific required fields, so existing callers that
    /// never set `kind` (it defaults to `Http`) always succeed here —
    /// this preserves full backward compatibility.
    pub fn create(input: CreateMonitor) -> Result<Self, DomainError> {
        Self::validate_kind_fields(
            input.kind,
            input.tcp_port,
            input.dns_record_type.as_deref(),
            input.heartbeat_token.as_ref(),
            input.heartbeat_interval_seconds,
            input.heartbeat_grace_period_seconds,
        )?;

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
            regions: if input.regions.is_empty() {
                vec![RegionCode::default_region()]
            } else {
                input.regions
            },
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
    pub fn update(
        &mut self,
        name: MonitorName,
        url: Url,
        method: HttpMethod,
        expected_status: ExpectedStatus,
        interval: CheckInterval,
        timeout: Timeout,
    ) {
        self.name = name;
        self.url = url.as_str().to_owned();
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
