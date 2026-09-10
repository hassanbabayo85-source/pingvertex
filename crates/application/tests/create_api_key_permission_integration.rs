//! Regression coverage for the Feature 10 privilege-escalation fix:
//! `CreateApiKeyUseCase::execute` must reject callers that lack
//! `Permission::ManageApiKeys` *before* doing anything else, so that a
//! read-only role (`Role::Auditor`) cannot mint an API key carrying
//! write scopes and use it to bypass its own read-only restriction.

mod common;
use common::*;

use std::sync::Arc;

use application::{
    commands::CreateApiKey,
    use_cases::{CreateApiKeyError, CreateApiKeyUseCase},
};
use domain::enums::ApiKeyScope;
use domain::shared::ids::UserId;
use domain::shared::types::{AuthenticatedUser, Role, TenantContext};
use infra::{
    Argon2PasswordHasher, InMemoryApiKeyRepository, InMemoryAuditLogEntryRepository,
    UuidIdGenerator,
};
use uuid::Uuid;

fn auth_with_role(role: Role) -> AuthenticatedUser {
    let user_id = UserId(Uuid::new_v4());
    let tenant = tenant_id();
    AuthenticatedUser::new(user_id, role, TenantContext::with_actor(tenant, user_id))
}

fn make_use_case() -> CreateApiKeyUseCase {
    CreateApiKeyUseCase::new(
        Arc::new(InMemoryApiKeyRepository::new()),
        Arc::new(UuidIdGenerator::new()),
        Arc::new(Argon2PasswordHasher),
        Arc::new(InMemoryAuditLogEntryRepository::new()),
    )
}

fn valid_command() -> CreateApiKey {
    CreateApiKey {
        name: "CI integration".to_string(),
        scopes: vec![ApiKeyScope::MonitorWrite, ApiKeyScope::StatusPageWrite],
        expires_at: None,
    }
}

#[tokio::test]
async fn auditor_cannot_create_an_api_key() {
    let use_case = make_use_case();
    let auditor = auth_with_role(Role::Auditor);

    let result = use_case.execute(valid_command(), auditor).await;

    match result {
        Err(CreateApiKeyError::Forbidden) => {}
        other => panic!("expected CreateApiKeyError::Forbidden for an Auditor, got {other:?}"),
    }
}

#[tokio::test]
async fn operator_cannot_create_an_api_key() {
    let use_case = make_use_case();
    let member = auth_with_role(Role::Operator);

    let result = use_case.execute(valid_command(), member).await;

    match result {
        Err(CreateApiKeyError::Forbidden) => {}
        other => panic!("expected CreateApiKeyError::Forbidden for a Member, got {other:?}"),
    }
}

#[tokio::test]
async fn admin_can_create_an_api_key() {
    let use_case = make_use_case();
    let admin = auth_with_role(Role::Admin);

    let result = use_case.execute(valid_command(), admin).await;

    assert!(
        result.is_ok(),
        "expected an Admin to be able to create an api key, got {result:?}"
    );
}

#[tokio::test]
async fn owner_can_create_an_api_key() {
    let use_case = make_use_case();
    let owner = auth_with_role(Role::Owner);

    let result = use_case.execute(valid_command(), owner).await;

    assert!(
        result.is_ok(),
        "expected an Owner to be able to create an api key, got {result:?}"
    );
}

/// The permission check must run before field validation, so a
/// forbidden caller gets `Forbidden` (not `EmptyName`/`EmptyScopes`)
/// even when the request body is also invalid — this keeps the
/// authorization boundary from leaking validation details to callers
/// who should not be here at all.
#[tokio::test]
async fn forbidden_takes_priority_over_validation_errors() {
    let use_case = make_use_case();
    let auditor = auth_with_role(Role::Auditor);

    let invalid_command = CreateApiKey {
        name: String::new(),
        scopes: vec![],
        expires_at: None,
    };

    let result = use_case.execute(invalid_command, auditor).await;

    match result {
        Err(CreateApiKeyError::Forbidden) => {}
        other => panic!("expected CreateApiKeyError::Forbidden to take priority, got {other:?}"),
    }
}
