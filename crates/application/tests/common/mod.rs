#![allow(dead_code)]

use std::sync::Arc;

use application::ports::AuditLogEntryRepository;
use domain::shared::ids::{TenantId, UserId};
use domain::shared::types::{AuthenticatedUser, Role, TenantContext};
use infra::InMemoryAuditLogEntryRepository;

pub fn auth_user(user_id: UserId, tenant_id: TenantId) -> AuthenticatedUser {
    AuthenticatedUser::new(
        user_id,
        Role::Owner,
        TenantContext::with_actor(tenant_id, user_id),
    )
}

#[allow(dead_code)]
pub fn new_audit_repo() -> Arc<dyn AuditLogEntryRepository> {
    Arc::new(InMemoryAuditLogEntryRepository::new())
}

pub fn tenant_id() -> TenantId {
    TenantId::new()
}
