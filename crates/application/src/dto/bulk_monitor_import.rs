use crate::dto::CreateMonitorRequest;
use domain::{HttpMethod, MonitorKind, RegionCode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkMonitorImportRow {
    /// Original 1-based CSV line number. This is internal preview metadata and
    /// is never accepted from / emitted to API clients.
    #[serde(skip)]
    pub source_row: usize,
    pub name: String,
    pub url: String,
    pub kind: MonitorKind,
    pub interval_seconds: u32,
    pub timeout_seconds: u32,
    pub method: HttpMethod,
    pub expected_status: u16,
    pub tcp_port: Option<u16>,
    pub dns_record_type: Option<String>,
    pub dns_expected_value: Option<String>,
    pub heartbeat_interval_seconds: Option<u32>,
    pub heartbeat_grace_period_seconds: Option<u32>,
    pub is_public: bool,
    pub ssl_check_enabled: bool,
    pub ssl_expiry_warning_days: u32,
    pub regions: Vec<RegionCode>,
    pub auth_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BulkMonitorImportError {
    pub row: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BulkMonitorImportPricingVersion {
    pub operation: String,
    pub unit_price: i64,
    pub currency: String,
    pub version: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkMonitorImportPreview {
    pub total_rows: usize,
    pub valid_rows: usize,
    pub invalid_rows: usize,
    pub duplicate_rows: usize,
    pub forecast_days: u32,
    pub estimated_execution_quantity: u64,
    pub estimated_payg_cost_units: i64,
    pub currency: String,
    /// Present when every operation in this preview resolves to the same
    /// immutable pricing version. Mixed-operation previews may legitimately
    /// span multiple versions, in which case this is `None` and
    /// `pricing_versions` is authoritative.
    pub pricing_version: Option<i64>,
    pub pricing_versions: Vec<BulkMonitorImportPricingVersion>,
    pub payg_eligible: bool,
    pub errors: Vec<BulkMonitorImportError>,
}

impl BulkMonitorImportRow {
    pub fn into_create_request(self) -> CreateMonitorRequest {
        CreateMonitorRequest {
            name: self.name,
            url: self.url,
            method: self.method,
            expected_status: self.expected_status,
            interval_seconds: self.interval_seconds,
            timeout_seconds: self.timeout_seconds,
            assertions: Vec::new(),
            is_public: self.is_public,
            ssl_check_enabled: self.ssl_check_enabled,
            ssl_expiry_warning_days: self.ssl_expiry_warning_days,
            regions: self.regions,
            kind: self.kind,
            tcp_port: self.tcp_port,
            dns_record_type: self.dns_record_type,
            dns_expected_value: self.dns_expected_value,
            heartbeat_interval_seconds: self.heartbeat_interval_seconds,
            heartbeat_grace_period_seconds: self.heartbeat_grace_period_seconds,
        }
    }
}
