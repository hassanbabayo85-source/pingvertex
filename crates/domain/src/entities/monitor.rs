use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::shared::ids::{MonitorId, UserId};

use crate::{
    enums::HttpMethod,
    value_objects::{CheckInterval, ExpectedStatus, MonitorName, RegionCode, Timeout, Url},
};

/// Data required to create a monitor.
#[derive(Debug, Clone)]
pub struct CreateMonitor {
    pub id: MonitorId,
    pub user_id: UserId,
    pub name: MonitorName,
    pub url: String,
    pub method: HttpMethod,
    pub expected_status: u16,
    pub interval_seconds: u32,
    pub timeout_seconds: u32,
    /// Regions this monitor should be checked from.
    /// Empty means "use the default single region" (`us-east`).
    pub regions: Vec<RegionCode>,
}

/// A monitor configured by a user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Monitor {
    pub assertions: Vec<crate::entities::Assertion>,
    pub id: MonitorId,
    pub user_id: UserId,
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
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Monitor {
    pub const MAX_ASSERTIONS: usize = 20;

    /// Creates a new monitor.
    pub fn create(input: CreateMonitor) -> Self {
        let now = Utc::now();

        Self {
            id: input.id,
            user_id: input.user_id,
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
            created_at: now,
            assertions: Vec::new(),
            updated_at: now,
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

    pub fn enable(&mut self) {
        self.enabled = true;
        self.updated_at = Utc::now();
    }

    pub fn disable(&mut self) {
        self.enabled = false;
        self.updated_at = Utc::now();
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
