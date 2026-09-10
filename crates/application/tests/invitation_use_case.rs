use std::sync::Arc;

use application::ports::{
    IdGenerator, PasswordHasher, TenantInvitationRepository, TenantMembershipRepository,
    UserRepository,
};
use application::use_cases::{
    AcceptInvitationUseCase, CreateInvitationCommand, CreateInvitationUseCase, InvitationError,
};
use domain::shared::ids::{TenantId, UserId};
use domain::shared::types::{AuthenticatedUser, Role, TenantContext};
use domain::{TenantMembership, User, UserBuilder, UserRole, UserStatus};
use infra::{
    InMemoryTenantInvitationRepository, InMemoryTenantMembershipRepository, InMemoryUserRepository,
    UuidIdGenerator,
};

use async_trait::async_trait;

#[derive(Clone)]
struct TestPasswordHasher;

#[async_trait]
impl PasswordHasher for TestPasswordHasher {
    async fn hash(
        &self,
        password: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok(password.to_string())
    }

    async fn verify(
        &self,
        password: &str,
        hash: &str,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        Ok(password == hash)
    }
}

fn make_user(id: UserId, email: &str) -> User {
    UserBuilder::new()
        .id(id)
        .username(domain::Username::new("testuser".to_string()).unwrap())
        .email(domain::Email::new(email).unwrap())
        .password_hash(domain::PasswordHash::new("hash".to_string()).unwrap())
        .role(UserRole::User)
        .status(UserStatus::Active)
        .email_verified_at(None)
        .totp_secret(None)
        .totp_enabled(false)
        .created_at(chrono::Utc::now())
        .updated_at(chrono::Utc::now())
        .build()
        .unwrap()
}

fn auth_user(id: UserId, tenant_id: TenantId, role: Role) -> AuthenticatedUser {
    AuthenticatedUser::new(id, role, TenantContext::with_actor(tenant_id, id))
}

#[tokio::test]
async fn full_invite_accept_flow_creates_membership_with_invited_role() {
    let user_repo = Arc::new(InMemoryUserRepository::new());
    let membership_repo = Arc::new(InMemoryTenantMembershipRepository::new());
    let invitation_repo = Arc::new(InMemoryTenantInvitationRepository::new());

    let tenant_id = TenantId::new();
    let owner = UserId::new();
    let invitee = UserId::new();
    let invitee_email = "invitee@example.com";

    user_repo
        .create(&make_user(owner, "owner@example.com"))
        .await
        .unwrap();
    user_repo
        .create(&make_user(invitee, invitee_email))
        .await
        .unwrap();

    let owner_actor = auth_user(owner, tenant_id, Role::Owner);

    let create_use_case = CreateInvitationUseCase::new(
        UuidIdGenerator::new(),
        TestPasswordHasher,
        invitation_repo.clone(),
    );

    let created = create_use_case
        .execute(
            CreateInvitationCommand {
                email: invitee_email.to_string(),
                role: Role::Admin,
            },
            owner_actor,
        )
        .await
        .unwrap();

    assert_eq!(created.invitation.email, invitee_email);
    assert_eq!(created.invitation.role, Role::Admin);

    let accept_use_case = AcceptInvitationUseCase::new(
        invitation_repo.clone(),
        membership_repo.clone(),
        user_repo.clone(),
        TestPasswordHasher,
    );

    let invitee_actor = auth_user(invitee, TenantId::new(), Role::Operator);

    let membership = accept_use_case
        .execute(created.raw_token, invitee_actor)
        .await
        .unwrap();

    assert_eq!(membership.tenant_id, tenant_id);
    assert_eq!(membership.user_id, invitee);
    assert_eq!(membership.role, Role::Admin);
    assert!(membership.joined_at.is_some());

    let stored = membership_repo
        .find_by_tenant_and_user(tenant_id, invitee)
        .await
        .unwrap();
    assert!(stored.is_some());
}

#[tokio::test]
async fn expired_invitation_is_rejected() {
    let user_repo = Arc::new(InMemoryUserRepository::new());
    let invitation_repo = Arc::new(InMemoryTenantInvitationRepository::new());

    let tenant_id = TenantId::new();
    let invitee = UserId::new();
    let email = "invitee@example.com";

    user_repo.create(&make_user(invitee, email)).await.unwrap();

    let id_generator = UuidIdGenerator::new();
    let raw_token = id_generator.next_invitation_token().await;
    let prefix = raw_token
        .splitn(3, '_')
        .take(2)
        .collect::<Vec<_>>()
        .join("_");

    let invitation = domain::TenantInvitation::new(
        domain::InvitationId::new(),
        tenant_id,
        email.to_string(),
        Role::Operator,
        prefix,
        raw_token.clone(),
        chrono::Utc::now() - chrono::Duration::seconds(1),
        chrono::Utc::now(),
    );

    invitation_repo.create(&invitation).await.unwrap();

    let accept_use_case = AcceptInvitationUseCase::new(
        invitation_repo.clone(),
        Arc::new(InMemoryTenantMembershipRepository::new()),
        user_repo.clone(),
        TestPasswordHasher,
    );

    let invitee_actor = auth_user(invitee, tenant_id, Role::Operator);
    let result = accept_use_case.execute(raw_token, invitee_actor).await;
    assert!(matches!(result, Err(InvitationError::InvalidToken)));
}

#[tokio::test]
async fn already_member_is_rejected() {
    let user_repo = Arc::new(InMemoryUserRepository::new());
    let membership_repo = Arc::new(InMemoryTenantMembershipRepository::new());
    let invitation_repo = Arc::new(InMemoryTenantInvitationRepository::new());

    let tenant_id = TenantId::new();
    let invitee = UserId::new();
    let email = "invitee@example.com";

    user_repo.create(&make_user(invitee, email)).await.unwrap();

    let now = chrono::Utc::now();
    membership_repo
        .create(&TenantMembership {
            tenant_id,
            user_id: invitee,
            role: Role::Operator,
            invited_at: now,
            joined_at: Some(now),
        })
        .await
        .unwrap();

    let id_generator = UuidIdGenerator::new();
    let raw_token = id_generator.next_invitation_token().await;
    let prefix = raw_token
        .splitn(3, '_')
        .take(2)
        .collect::<Vec<_>>()
        .join("_");

    let invitation = domain::TenantInvitation::new(
        domain::InvitationId::new(),
        tenant_id,
        email.to_string(),
        Role::Operator,
        prefix,
        raw_token.clone(),
        chrono::Utc::now() + chrono::Duration::hours(1),
        chrono::Utc::now(),
    );
    invitation_repo.create(&invitation).await.unwrap();

    let accept_use_case = AcceptInvitationUseCase::new(
        invitation_repo,
        membership_repo,
        user_repo,
        TestPasswordHasher,
    );

    let invitee_actor = auth_user(invitee, tenant_id, Role::Operator);
    let result = accept_use_case.execute(raw_token, invitee_actor).await;
    assert!(matches!(result, Err(InvitationError::Conflict)));
}

#[tokio::test]
async fn wrong_recipient_is_rejected() {
    let user_repo = Arc::new(InMemoryUserRepository::new());
    let invitation_repo = Arc::new(InMemoryTenantInvitationRepository::new());

    let tenant_id = TenantId::new();
    let invitee = UserId::new();
    let wrong_user = UserId::new();
    let invitee_email = "invitee@example.com";

    user_repo
        .create(&make_user(invitee, invitee_email))
        .await
        .unwrap();
    user_repo
        .create(&make_user(wrong_user, "wrong@example.com"))
        .await
        .unwrap();

    let id_generator = UuidIdGenerator::new();
    let raw_token = id_generator.next_invitation_token().await;
    let prefix = raw_token
        .splitn(3, '_')
        .take(2)
        .collect::<Vec<_>>()
        .join("_");

    let invitation = domain::TenantInvitation::new(
        domain::InvitationId::new(),
        tenant_id,
        invitee_email.to_string(),
        Role::Operator,
        prefix,
        raw_token.clone(),
        chrono::Utc::now() + chrono::Duration::hours(1),
        chrono::Utc::now(),
    );
    invitation_repo.create(&invitation).await.unwrap();

    let accept_use_case = AcceptInvitationUseCase::new(
        invitation_repo.clone(),
        Arc::new(InMemoryTenantMembershipRepository::new()),
        user_repo.clone(),
        TestPasswordHasher,
    );

    let wrong_actor = auth_user(wrong_user, tenant_id, Role::Operator);
    let result = accept_use_case.execute(raw_token, wrong_actor).await;
    assert!(matches!(result, Err(InvitationError::InvalidToken)));
}
