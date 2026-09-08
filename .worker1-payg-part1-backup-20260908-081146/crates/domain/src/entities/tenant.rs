use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::errors::DomainError;
use crate::shared::ids::{TenantId, UserId};

/// Wata kungiya/asusun tenant a cikin tsarin multi-tenant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tenant {
    pub id: TenantId,
    pub name: String,
    pub owner_user_id: UserId,
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
            created_at: Utc::now(),
        })
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
