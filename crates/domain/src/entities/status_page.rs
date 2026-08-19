use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    errors::DomainError,
    shared::{
        ids::{MonitorId, StatusPageId, TenantId, UserId},
        policies::{Ownable, TenantOwned},
    },
    value_objects::{CustomDomain, HexColor, Subdomain, Url},
};

pub const MAX_STATUS_PAGE_MONITORS: usize = 50;
const MAX_STATUS_PAGE_NAME_LENGTH: usize = 100;
const MAX_LOGO_URL_LENGTH: usize = 2048;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateStatusPage {
    pub id: StatusPageId,
    pub user_id: UserId,
    pub tenant_id: TenantId,
    pub name: String,
    pub subdomain: Subdomain,
    pub monitor_ids: Vec<MonitorId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CustomDomainVerification {
    Pending {
        token: String,
        requested_at: DateTime<Utc>,
    },
    Verified {
        verified_at: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StatusPage {
    pub id: StatusPageId,
    pub user_id: UserId,
    pub tenant_id: TenantId,
    pub name: String,
    pub subdomain: Subdomain,
    pub custom_domain: Option<(CustomDomain, CustomDomainVerification)>,
    pub logo_url: Option<String>,
    pub primary_color: Option<HexColor>,
    pub monitor_ids: Vec<MonitorId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl StatusPage {
    pub fn create(input: CreateStatusPage) -> Result<Self, DomainError> {
        Self::validate_name(&input.name)?;
        Self::validate_monitor_ids(&input.monitor_ids)?;

        let now = Utc::now();
        Ok(Self {
            id: input.id,
            user_id: input.user_id,
            tenant_id: input.tenant_id,
            name: input.name,
            subdomain: input.subdomain,
            custom_domain: None,
            logo_url: None,
            primary_color: None,
            monitor_ids: input.monitor_ids,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn rename(&mut self, name: String) -> Result<(), DomainError> {
        Self::validate_name(&name)?;
        self.name = name;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn set_monitors(&mut self, monitor_ids: Vec<MonitorId>) -> Result<(), DomainError> {
        Self::validate_monitor_ids(&monitor_ids)?;
        self.monitor_ids = monitor_ids;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn set_logo_url(&mut self, logo_url: Option<String>) -> Result<(), DomainError> {
        if let Some(raw) = logo_url {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Err(DomainError::InvalidLogoUrl);
            }
            if trimmed.len() > MAX_LOGO_URL_LENGTH {
                return Err(DomainError::LogoUrlTooLong);
            }
            let _ = Url::new(trimmed).map_err(|_| DomainError::InvalidLogoUrl)?;
            self.logo_url = Some(trimmed.to_owned());
        } else {
            self.logo_url = None;
        }
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn set_primary_color(&mut self, color: Option<HexColor>) {
        self.primary_color = color;
        self.updated_at = Utc::now();
    }

    pub fn request_custom_domain(&mut self, domain: CustomDomain) {
        self.custom_domain = Some((
            domain,
            CustomDomainVerification::Pending {
                token: Uuid::new_v4().to_string(),
                requested_at: Utc::now(),
            },
        ));
        self.updated_at = Utc::now();
    }

    pub fn remove_custom_domain(&mut self) {
        self.custom_domain = None;
        self.updated_at = Utc::now();
    }

    pub fn mark_custom_domain_verified(&mut self) -> Result<(), DomainError> {
        match self.custom_domain.clone() {
            Some((domain, CustomDomainVerification::Pending { .. })) => {
                self.custom_domain = Some((
                    domain,
                    CustomDomainVerification::Verified {
                        verified_at: Utc::now(),
                    },
                ));
                self.updated_at = Utc::now();
                Ok(())
            }
            _ => Ok(()),
        }
    }

    pub fn verified_custom_domain(&self) -> Option<&str> {
        match &self.custom_domain {
            Some((domain, CustomDomainVerification::Verified { .. })) => Some(domain.as_str()),
            _ => None,
        }
    }

    fn validate_name(name: &str) -> Result<(), DomainError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(DomainError::EmptyStatusPageName);
        }
        if name.chars().count() > MAX_STATUS_PAGE_NAME_LENGTH {
            return Err(DomainError::StatusPageNameTooLong);
        }
        Ok(())
    }

    fn validate_monitor_ids(monitor_ids: &[MonitorId]) -> Result<(), DomainError> {
        if monitor_ids.len() > MAX_STATUS_PAGE_MONITORS {
            return Err(DomainError::TooManyStatusPageMonitors);
        }
        Ok(())
    }
}

impl Ownable for StatusPage {
    fn owner_id(&self) -> UserId {
        self.user_id
    }
}

impl TenantOwned for StatusPage {
    fn tenant_id(&self) -> TenantId {
        self.tenant_id
    }
}
