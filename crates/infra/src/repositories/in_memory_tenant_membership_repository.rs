use std::error::Error;

use application::ports::TenantMembershipRepository;
use async_trait::async_trait;
use chrono::Utc;
use domain::TenantMembership;
use domain::shared::ids::{TenantId, UserId};
use domain::shared::types::Role;

#[derive(Debug, Clone, Default)]
pub struct InMemoryTenantMembershipRepository;

impl InMemoryTenantMembershipRepository {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TenantMembershipRepository for InMemoryTenantMembershipRepository {
    async fn create(
        &self,
        _membership: &TenantMembership,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        Ok(())
    }

    async fn find_by_tenant_and_user(
        &self,
        tenant_id: TenantId,
        user_id: UserId,
    ) -> Result<Option<TenantMembership>, Box<dyn Error + Send + Sync>> {
        let now = Utc::now();
        Ok(Some(TenantMembership {
            tenant_id,
            user_id,
            role: Role::Owner,
            invited_at: now,
            joined_at: Some(now),
        }))
    }

    async fn list_by_tenant(
        &self,
        _tenant_id: TenantId,
    ) -> Result<Vec<TenantMembership>, Box<dyn Error + Send + Sync>> {
        Ok(vec![])
    }

    async fn list_by_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<TenantMembership>, Box<dyn Error + Send + Sync>> {
        let now = Utc::now();
        Ok(vec![TenantMembership {
            tenant_id: TenantId::new(),
            user_id,
            role: Role::Owner,
            invited_at: now,
            joined_at: Some(now),
        }])
    }

    async fn update_role(
        &self,
        _tenant_id: TenantId,
        _user_id: UserId,
        _role: Role,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        Ok(())
    }
}
