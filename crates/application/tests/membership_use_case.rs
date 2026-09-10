use std::sync::Arc;

use application::ports::{TenantMembershipRepository, UserRepository};
use application::use_cases::{
    AddMemberCommand, AddMemberUseCase, ChangeMemberRoleCommand, ChangeMemberRoleUseCase,
    ListMembersUseCase, MembershipError, RemoveMemberUseCase,
};
use domain::shared::ids::{TenantId, UserId};
use domain::shared::types::{AuthenticatedUser, Role, TenantContext};
use domain::{TenantMembership, User, UserBuilder, UserRole, UserStatus};
use infra::{InMemoryTenantMembershipRepository, InMemoryUserRepository};

fn make_user(id: UserId) -> User {
    UserBuilder::new()
        .id(id)
        .username(domain::Username::new("user".to_string()).unwrap())
        .email(domain::Email::new("user@example.com".to_string()).unwrap())
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
async fn owner_can_add_admin_and_member() {
    let user_repo = Arc::new(InMemoryUserRepository::new());
    let membership_repo = Arc::new(InMemoryTenantMembershipRepository::new());

    let tenant_id = TenantId::new();
    let owner = UserId::new();
    let target_admin = UserId::new();
    let target_member = UserId::new();

    user_repo.create(&make_user(owner)).await.unwrap();
    user_repo.create(&make_user(target_admin)).await.unwrap();
    user_repo.create(&make_user(target_member)).await.unwrap();

    // Seed owner membership
    let now = chrono::Utc::now();
    membership_repo
        .create(&TenantMembership {
            tenant_id,
            user_id: owner,
            role: Role::Owner,
            invited_at: now,
            joined_at: Some(now),
        })
        .await
        .unwrap();

    let actor = auth_user(owner, tenant_id, Role::Owner);

    let use_case = AddMemberUseCase::new(membership_repo.clone(), user_repo.clone());

    let added_admin = use_case
        .execute(
            AddMemberCommand {
                user_id: target_admin,
                role: Role::Admin,
            },
            actor,
        )
        .await
        .unwrap();
    assert_eq!(added_admin.role, Role::Admin);

    let added_member = use_case
        .execute(
            AddMemberCommand {
                user_id: target_member,
                role: Role::Operator,
            },
            actor,
        )
        .await
        .unwrap();
    assert_eq!(added_member.role, Role::Operator);
}

#[tokio::test]
async fn admin_cannot_add_owner_or_admin() {
    let user_repo = Arc::new(InMemoryUserRepository::new());
    let membership_repo = Arc::new(InMemoryTenantMembershipRepository::new());

    let tenant_id = TenantId::new();
    let admin = UserId::new();
    let target = UserId::new();

    user_repo.create(&make_user(admin)).await.unwrap();
    user_repo.create(&make_user(target)).await.unwrap();

    let now = chrono::Utc::now();
    membership_repo
        .create(&TenantMembership {
            tenant_id,
            user_id: admin,
            role: Role::Admin,
            invited_at: now,
            joined_at: Some(now),
        })
        .await
        .unwrap();

    let actor = auth_user(admin, tenant_id, Role::Admin);
    let use_case = AddMemberUseCase::new(membership_repo.clone(), user_repo.clone());

    let result = use_case
        .execute(
            AddMemberCommand {
                user_id: target,
                role: Role::Owner,
            },
            actor,
        )
        .await;
    assert!(matches!(result, Err(MembershipError::Forbidden)));

    let result = use_case
        .execute(
            AddMemberCommand {
                user_id: target,
                role: Role::Admin,
            },
            actor,
        )
        .await;
    assert!(matches!(result, Err(MembershipError::Forbidden)));
}

#[tokio::test]
async fn operator_cannot_list_members() {
    let membership_repo = Arc::new(InMemoryTenantMembershipRepository::new());
    let tenant_id = TenantId::new();
    let member = UserId::new();

    let now = chrono::Utc::now();
    membership_repo
        .create(&TenantMembership {
            tenant_id,
            user_id: member,
            role: Role::Operator,
            invited_at: now,
            joined_at: Some(now),
        })
        .await
        .unwrap();

    let actor = auth_user(member, tenant_id, Role::Operator);
    let use_case = ListMembersUseCase::new(membership_repo.clone());
    let result = use_case.execute(actor).await;
    assert!(matches!(result, Err(MembershipError::Forbidden)));
}

#[tokio::test]
async fn owner_can_remove_member() {
    let user_repo = Arc::new(InMemoryUserRepository::new());
    let membership_repo = Arc::new(InMemoryTenantMembershipRepository::new());

    let tenant_id = TenantId::new();
    let owner = UserId::new();
    let target = UserId::new();

    user_repo.create(&make_user(owner)).await.unwrap();
    user_repo.create(&make_user(target)).await.unwrap();

    let now = chrono::Utc::now();
    membership_repo
        .create(&TenantMembership {
            tenant_id,
            user_id: owner,
            role: Role::Owner,
            invited_at: now,
            joined_at: Some(now),
        })
        .await
        .unwrap();
    membership_repo
        .create(&TenantMembership {
            tenant_id,
            user_id: target,
            role: Role::Operator,
            invited_at: now,
            joined_at: Some(now),
        })
        .await
        .unwrap();

    let actor = auth_user(owner, tenant_id, Role::Owner);
    let use_case = RemoveMemberUseCase::new(membership_repo.clone());
    use_case.execute(target, actor).await.unwrap();

    assert!(
        membership_repo
            .find_by_tenant_and_user(tenant_id, target)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn owner_cannot_demote_last_owner() {
    let user_repo = Arc::new(InMemoryUserRepository::new());
    let membership_repo = Arc::new(InMemoryTenantMembershipRepository::new());

    let tenant_id = TenantId::new();
    let owner = UserId::new();

    user_repo.create(&make_user(owner)).await.unwrap();

    let now = chrono::Utc::now();
    membership_repo
        .create(&TenantMembership {
            tenant_id,
            user_id: owner,
            role: Role::Owner,
            invited_at: now,
            joined_at: Some(now),
        })
        .await
        .unwrap();

    let actor = auth_user(owner, tenant_id, Role::Owner);
    let use_case = ChangeMemberRoleUseCase::new(membership_repo.clone());
    let result = use_case
        .execute(
            ChangeMemberRoleCommand {
                user_id: owner,
                new_role: Role::Operator,
            },
            actor,
        )
        .await;
    assert!(matches!(result, Err(MembershipError::InvalidOperation(_))));
}
