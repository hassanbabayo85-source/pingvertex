use std::sync::Arc;

use chrono::{Duration, Utc};
use domain::{
    Email, InvitationId, TenantInvitation,
    entities::MAX_TEAM_MEMBERS,
    shared::types::{AuthenticatedUser, Permission, Role},
};

use crate::errors::PlanLimitError;
use crate::ports::{
    AuditLogEntryRepository, EntitlementResolver, IdGenerator, PasswordHasher,
    TenantInvitationRepository, TenantMembershipRepository,
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
/// TASK 4A — `max_team_members` quota:
/// - If **both** `entitlement_resolver` and `membership_repository` are
///   wired, the use case counts `(existing_members + pending_invitations)`
///   and rejects when that sum already reaches the tenant's limit.
/// - If either dependency is missing, the quota check is skipped
///   (backward compatibility for code paths not yet updated). Production
///   handlers MUST wire both.
///
/// LIMITATION: the count-then-create is **not** atomic across
/// `create_invitation` and `add_member`. Two simultaneous invitations
/// can both pass the check and both be created. This is acceptable
/// because `accept_invitation` (the point where the slot is actually
/// consumed) enforces the quota **atomically** via
/// `save_if_within_quota`. Worst case: a small number of extra pending
/// invitations that expire unaccepted.
pub struct CreateInvitationUseCase<G, H, R> {
    id_generator: G,
    password_hasher: H,
    invitation_repository: R,
    audit_log_repository: Option<Arc<dyn AuditLogEntryRepository>>,
    entitlement_resolver: Option<Arc<dyn EntitlementResolver>>,
    membership_repository: Option<Arc<dyn TenantMembershipRepository>>,
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
            entitlement_resolver: None,
            membership_repository: None,
        }
    }

    pub fn with_audit_log_repository(
        mut self,
        repository: Arc<dyn AuditLogEntryRepository>,
    ) -> Self {
        self.audit_log_repository = Some(repository);
        self
    }

    /// TASK 4A — enables `max_team_members` quota enforcement when
    /// combined with `.with_membership_repository(...)`.
    pub fn with_entitlement_resolver(
        mut self,
        resolver: Arc<dyn EntitlementResolver>,
    ) -> Self {
        self.entitlement_resolver = Some(resolver);
        self
    }

    /// TASK 4A — required (together with `entitlement_resolver`) for
    /// quota enforcement.
    pub fn with_membership_repository(
        mut self,
        repo: Arc<dyn TenantMembershipRepository>,
    ) -> Self {
        self.membership_repository = Some(repo);
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

        let tenant_id = actor.tenant_context().tenant_id();
        let now = Utc::now();

        // ── TASK 4A: quota check (best-effort) ──────────────────
        //
        // Only runs when both dependencies are wired. See the type
        // doc comment for the atomicity limitation and why it is safe.
        if let (Some(resolver), Some(membership_repo)) =
            (&self.entitlement_resolver, &self.membership_repository)
        {
            let entitlements = resolver
                .resolve(tenant_id)
                .await
                .map_err(InvitationError::Internal)?;

            let members = membership_repo
                .count_for_tenant(tenant_id)
                .await
                .map_err(InvitationError::Internal)?;
            let pending = self
                .invitation_repository
                .count_pending_for_tenant(tenant_id, now)
                .await
                .map_err(InvitationError::Internal)?;

            if members + pending >= u64::from(entitlements.max_team_members) {
                return Err(InvitationError::Internal(Box::new(PlanLimitError::new(
                    MAX_TEAM_MEMBERS,
                    members + pending,
                    u64::from(entitlements.max_team_members),
                    "upgrade_plan",
                ))));
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

        let expires_at = now + Duration::hours(INVITATION_TOKEN_TTL_HOURS);

        let invitation = TenantInvitation::new(
            InvitationId::new(),
            tenant_id,
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
