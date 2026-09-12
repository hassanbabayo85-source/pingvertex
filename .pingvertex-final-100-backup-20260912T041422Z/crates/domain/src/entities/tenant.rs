use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::errors::DomainError;
use crate::shared::ids::{TenantId, UserId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountType {
    Individual,
    Company,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TenantStatus {
    #[default]
    Active,
    Suspended,
}

impl TenantStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Suspended => "suspended",
        }
    }
}

impl std::str::FromStr for TenantStatus {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "active" => Ok(Self::Active),
            "suspended" => Ok(Self::Suspended),
            other => Err(DomainError::InvalidTenantStatus(other.to_owned())),
        }
    }
}

impl AccountType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Individual => "individual",
            Self::Company => "company",
        }
    }
}

impl std::str::FromStr for AccountType {
    type Err = DomainError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "individual" => Ok(Self::Individual),
            "company" => Ok(Self::Company),
            other => Err(DomainError::InvalidTenantAccountType(other.to_owned())),
        }
    }
}

/// Wata kungiya/asusun tenant a cikin tsarin multi-tenant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tenant {
    pub id: TenantId,
    pub name: String,
    pub owner_user_id: UserId,
    pub account_type: AccountType,
    pub status: TenantStatus,
    pub created_at: DateTime<Utc>,
}

impl Tenant {
    /// Max tsawon suna: haruffa 100.
    pub const MAX_NAME_LENGTH: usize = 100;

    /// Kirkiri sabon Tenant tare da validation akan `name`.
    pub fn new(
        id: TenantId,
        name: impl Into<String>,
        owner_user_id: UserId,
    ) -> Result<Self, DomainError> {
        let name = name.into();
        let name = name.trim();

        if name.is_empty() {
            return Err(DomainError::EmptyTenantName);
        }

        if name.chars().count() > Self::MAX_NAME_LENGTH {
            return Err(DomainError::TenantNameTooLong);
        }

        Ok(Self {
            id,
            name: name.to_owned(),
            owner_user_id,
            account_type: AccountType::Individual,
            status: TenantStatus::Active,
            created_at: Utc::now(),
        })
    }

    /// Creates a Company tenant while keeping the same wallet/PAYG engine.
    pub fn new_company(
        id: TenantId,
        name: impl Into<String>,
        owner_user_id: UserId,
    ) -> Result<Self, DomainError> {
        let mut tenant = Self::new(id, name, owner_user_id)?;
        tenant.account_type = AccountType::Company;
        Ok(tenant)
    }

    pub fn suspend(&mut self) {
        self.status = TenantStatus::Suspended;
    }

    pub fn activate(&mut self) {
        self.status = TenantStatus::Active;
    }

    pub fn is_active(&self) -> bool {
        self.status == TenantStatus::Active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_name() {
        let tenant = Tenant::new(TenantId::new(), "Acme Inc", UserId::new()).unwrap();

        assert_eq!(tenant.name, "Acme Inc");
    }

    #[test]
    fn trims_whitespace() {
        let tenant = Tenant::new(TenantId::new(), "   Acme Inc   ", UserId::new()).unwrap();

        assert_eq!(tenant.name, "Acme Inc");
    }

    #[test]
    fn rejects_empty_name() {
        let result = Tenant::new(TenantId::new(), "", UserId::new());

        assert_eq!(result, Err(DomainError::EmptyTenantName));
    }

    #[test]
    fn rejects_whitespace_only_name() {
        let result = Tenant::new(TenantId::new(), "      ", UserId::new());

        assert_eq!(result, Err(DomainError::EmptyTenantName));
    }

    #[test]
    fn rejects_name_longer_than_100_chars() {
        let value = "a".repeat(101);

        let result = Tenant::new(TenantId::new(), value, UserId::new());

        assert_eq!(result, Err(DomainError::TenantNameTooLong));
    }
}
