use std::error::Error;
use std::sync::{Arc, Mutex};

use application::ports::TenantInvitationRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{InvitationId, TenantId, TenantInvitation};

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone, Default)]
pub struct InMemoryTenantInvitationRepository {
    invitations: Arc<Mutex<Vec<TenantInvitation>>>,
}

impl InMemoryTenantInvitationRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl TenantInvitationRepository for InMemoryTenantInvitationRepository {
    async fn create(&self, invitation: &TenantInvitation) -> RepoResult<()> {
        self.invitations.lock().unwrap().push(invitation.clone());
        Ok(())
    }

    async fn find_by_prefix(&self, token_prefix: &str) -> RepoResult<Option<TenantInvitation>> {
        Ok(self
            .invitations
            .lock()
            .unwrap()
            .iter()
            .find(|invitation| invitation.token_prefix == token_prefix)
            .cloned())
    }

    async fn find_by_id(&self, id: InvitationId) -> RepoResult<Option<TenantInvitation>> {
        Ok(self
            .invitations
            .lock()
            .unwrap()
            .iter()
            .find(|invitation| invitation.id == id)
            .cloned())
    }

    async fn list_by_tenant(&self, tenant_id: TenantId) -> RepoResult<Vec<TenantInvitation>> {
        Ok(self
            .invitations
            .lock()
            .unwrap()
            .iter()
            .filter(|invitation| invitation.tenant_id == tenant_id)
            .cloned()
            .collect())
    }

    async fn mark_revoked(&self, id: InvitationId, revoked_at: DateTime<Utc>) -> RepoResult<()> {
        if let Some(invitation) = self
            .invitations
            .lock()
            .unwrap()
            .iter_mut()
            .find(|i| i.id == id)
        {
            if invitation.revoked_at.is_none() && invitation.accepted_at.is_none() {
                invitation.revoked_at = Some(revoked_at);
            }
        }
        Ok(())
    }

    async fn mark_accepted(&self, id: InvitationId, accepted_at: DateTime<Utc>) -> RepoResult<()> {
        if let Some(invitation) = self
            .invitations
            .lock()
            .unwrap()
            .iter_mut()
            .find(|i| i.id == id)
        {
            if invitation.revoked_at.is_none() && invitation.accepted_at.is_none() {
                invitation.accepted_at = Some(accepted_at);
            }
        }
        Ok(())
    }
}
