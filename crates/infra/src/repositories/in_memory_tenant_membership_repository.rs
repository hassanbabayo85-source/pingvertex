use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex};

use application::ports::{LastOwnerGuardOutcome, TenantMembershipRepository};
use async_trait::async_trait;
use chrono::Utc;
use domain::TenantMembership;
use domain::shared::ids::{TenantId, UserId};
use domain::shared::types::Role;

#[derive(Debug, Clone, Default)]
pub struct InMemoryTenantMembershipRepository {
    memberships: Arc<Mutex<HashMap<(TenantId, UserId), TenantMembership>>>,
    fallback_tenant_cache: Arc<Mutex<HashMap<UserId, TenantId>>>,
}

impl InMemoryTenantMembershipRepository {
    pub fn new() -> Self {
        Self {
            memberships: Arc::new(Mutex::new(HashMap::new())),
            fallback_tenant_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl TenantMembershipRepository for InMemoryTenantMembershipRepository {
    async fn create(
        &self,
        membership: &TenantMembership,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.memberships.lock().unwrap().insert(
            (membership.tenant_id, membership.user_id),
            membership.clone(),
        );
        Ok(())
    }

    async fn find_by_tenant_and_user(
        &self,
        tenant_id: TenantId,
        user_id: UserId,
    ) -> Result<Option<TenantMembership>, Box<dyn Error + Send + Sync>> {
        Ok(self
            .memberships
            .lock()
            .unwrap()
            .get(&(tenant_id, user_id))
            .cloned())
    }

    async fn list_by_tenant(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<TenantMembership>, Box<dyn Error + Send + Sync>> {
        Ok(self
            .memberships
            .lock()
            .unwrap()
            .values()
            .filter(|m| m.tenant_id == tenant_id)
            .cloned()
            .collect())
    }

    async fn list_by_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<TenantMembership>, Box<dyn Error + Send + Sync>> {
        let stored = self
            .memberships
            .lock()
            .unwrap()
            .values()
            .filter(|m| m.user_id == user_id)
            .cloned()
            .collect::<Vec<_>>();

        if !stored.is_empty() {
            return Ok(stored);
        }

        // Backwards-compatible fallback for tests that never seeded
        // memberships and rely on a single implicit Owner membership.
        let mut cache = self.fallback_tenant_cache.lock().unwrap();
        let tenant_id = *cache.entry(user_id).or_default();
        let now = Utc::now();
        let membership = TenantMembership {
            tenant_id,
            user_id,
            role: Role::Owner,
            invited_at: now,
            joined_at: Some(now),
        };
        self.memberships
            .lock()
            .unwrap()
            .insert((tenant_id, user_id), membership.clone());
        Ok(vec![membership])
    }

    async fn update_role(
        &self,
        tenant_id: TenantId,
        user_id: UserId,
        role: Role,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut map = self.memberships.lock().unwrap();
        if let Some(membership) = map.get_mut(&(tenant_id, user_id)) {
            membership.role = role;
        }
        Ok(())
    }

    async fn delete(
        &self,
        tenant_id: TenantId,
        user_id: UserId,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.memberships
            .lock()
            .unwrap()
            .remove(&(tenant_id, user_id));
        Ok(())
    }

    async fn remove_member_if_not_last_owner(
        &self,
        tenant_id: TenantId,
        user_id: UserId,
    ) -> Result<LastOwnerGuardOutcome, Box<dyn Error + Send + Sync>> {
        // A single `std::sync::Mutex` guard held across the check-then-act
        // below makes this atomic with respect to any other call on this
        // repository instance — no `.await` happens while the lock is
        // held, so no other task can interleave.
        let mut map = self.memberships.lock().unwrap();
        let Some(target) = map.get(&(tenant_id, user_id)) else {
            return Ok(LastOwnerGuardOutcome::NotFound);
        };

        if target.role == Role::Owner {
            let owner_count = map
                .values()
                .filter(|m| m.tenant_id == tenant_id && m.role == Role::Owner)
                .count();
            if owner_count <= 1 {
                return Ok(LastOwnerGuardOutcome::WouldRemoveLastOwner);
            }
        }

        map.remove(&(tenant_id, user_id));
        Ok(LastOwnerGuardOutcome::Applied)
    }

    async fn update_role_if_not_last_owner(
        &self,
        tenant_id: TenantId,
        user_id: UserId,
        new_role: Role,
    ) -> Result<LastOwnerGuardOutcome, Box<dyn Error + Send + Sync>> {
        let mut map = self.memberships.lock().unwrap();
        let Some(target) = map.get(&(tenant_id, user_id)) else {
            return Ok(LastOwnerGuardOutcome::NotFound);
        };

        if target.role == Role::Owner && new_role != Role::Owner {
            let owner_count = map
                .values()
                .filter(|m| m.tenant_id == tenant_id && m.role == Role::Owner)
                .count();
            if owner_count <= 1 {
                return Ok(LastOwnerGuardOutcome::WouldRemoveLastOwner);
            }
        }

        if let Some(membership) = map.get_mut(&(tenant_id, user_id)) {
            membership.role = new_role;
        }
        Ok(LastOwnerGuardOutcome::Applied)
    }
}
