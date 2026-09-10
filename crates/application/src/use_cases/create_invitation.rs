use chrono::{Duration, Utc};
use domain::{
    Email, InvitationId, TenantInvitation,
    shared::types::{AuthenticatedUser, Permission, Role},
};

use crate::ports::{
    AuditLogEntryRepository, IdGenerator, PasswordHasher, TenantInvitationRepository,
};
use crate::use_cases::{
    invitation_common::InvitationError,
    invitation_support::{INVITATION_TOKEN_TTL_HOURS, invitation_token_prefix},
};

pub struct CreateInvitationCommand {
    pub email: String,
    pub role: Role,
}

pub struct CreatedTenantInvitation {
    pub invitation: TenantInvitation,
    pub raw_token: String,
}

/// Creates a new invitation for `email` to join the caller's tenant
/// with `role`.
///
/// Authorization:
/// - Requires `Permission::ManageUsers`.
/// - Only `Owner` may invite `Owner` or `Admin`.
/// - `Owner` or `Admin` may invite `Member`, `Viewer`, or `Auditor`.
///
/// SECURITY:
/// - The raw invitation token is returned exactly once to the caller;
///   it is never logged or persisted.
/// - Only the Argon2id hash is stored.
/// - The stored email is normalized to lowercase before persistence.
pub struct CreateInvitationUseCase<G, H, R> {
    id_generator: G,
    password_hasher: H,
    invitation_repository: R,
    audit_log_repository: Option<std::sync::Arc<dyn AuditLogEntryRepository>>,
}

impl<G, H, R> CreateInvitationUseCase<G, H, R>
where
    G: IdGenerator,
    H: PasswordHasher,
    R: TenantInvitationRepository,
{
    pub fn new(id_generator: G, password_hasher: H, invitation_repository: R) -> Self {
        Self {
            id_generator,
            password_hasher,
            invitation_repository,
            audit_log_repository: None,
        }
    }

    pub fn with_audit_log_repository(
        mut self,
        repository: std::sync::Arc<dyn AuditLogEntryRepository>,
    ) -> Self {
        self.audit_log_repository = Some(repository);
        self
    }

    pub async fn execute(
        &self,
        command: CreateInvitationCommand,
        actor: AuthenticatedUser,
    ) -> Result<CreatedTenantInvitation, InvitationError> {
        if !actor.can(Permission::ManageUsers) {
            return Err(InvitationError::Forbidden);
        }

        let actor_role = actor.role();
        match command.role {
            Role::Owner | Role::Admin => {
                if actor_role != Role::Owner {
                    return Err(InvitationError::Forbidden);
                }
            }
            Role::Operator | Role::Viewer | Role::Auditor => {
                if !matches!(actor_role, Role::Owner | Role::Admin) {
                    return Err(InvitationError::Forbidden);
                }
            }
        }

        let email = Email::new(command.email)
            .map_err(|e| InvitationError::InvalidOperation(e.to_string()))?;

        let raw_token = self.id_generator.next_invitation_token().await;
        let token_prefix =
            invitation_token_prefix(&raw_token).map_err(InvitationError::Internal)?;
        let token_hash = self
            .password_hasher
            .hash(&raw_token)
            .await
            .map_err(InvitationError::Internal)?;

        let now = Utc::now();
        let expires_at = now + Duration::hours(INVITATION_TOKEN_TTL_HOURS);

        let invitation = TenantInvitation::new(
            InvitationId::new(),
            actor.tenant_context().tenant_id(),
            email.as_str().to_string(),
            command.role,
            token_prefix,
            token_hash,
            expires_at,
            now,
        );

        self.invitation_repository
            .create(&invitation)
            .await
            .map_err(InvitationError::Internal)?;

        if let Some(audit) = &self.audit_log_repository {
            let entry = domain::AuditLogEntry {
                id: domain::AuditLogEntryId::new(),
                actor_user_id: actor.user_id(),
                tenant_id: Some(invitation.tenant_id),
                action: domain::AuditAction::MemberInvited,
                target_id: invitation.id.into_uuid().to_string(),
                metadata: serde_json::json!({
                    "role": invitation.role,
                }),
                created_at: now,
            };
            audit
                .create(&entry)
                .await
                .map_err(InvitationError::Internal)?;
        }

        Ok(CreatedTenantInvitation {
            invitation,
            raw_token,
        })
    }
}
