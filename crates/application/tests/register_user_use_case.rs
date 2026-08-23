use std::error::Error;

use application::dto::RegisterRequest;
use application::ports::{RegistrationTransaction, UserRepository};
use application::use_cases::RegisterUserUseCase;
use async_trait::async_trait;
use domain::{Email, Tenant, TenantMembership, User, UserId, Username};
use infra::{Argon2PasswordHasher, JsonWebTokenService, UuidIdGenerator};

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Test double for `UserRepository` whose `email_exists` /
/// `username_exists` responses are configured per test, so we can force
/// each collision path independently.
struct MockUserRepository {
    email_exists: bool,
    username_exists: bool,
}

#[async_trait]
impl UserRepository for MockUserRepository {
    async fn create(&self, _user: &User) -> RepoResult<()> {
        Ok(())
    }

    async fn update(&self, _user: &User) -> RepoResult<()> {
        Ok(())
    }

    async fn find_by_id(&self, _id: &UserId) -> RepoResult<Option<User>> {
        Ok(None)
    }

    async fn find_by_email(&self, _email: &Email) -> RepoResult<Option<User>> {
        Ok(None)
    }

    async fn find_by_username(&self, _username: &Username) -> RepoResult<Option<User>> {
        Ok(None)
    }

    async fn delete(&self, _id: &UserId) -> RepoResult<()> {
        Ok(())
    }

    async fn email_exists(&self, _email: &Email) -> RepoResult<bool> {
        Ok(self.email_exists)
    }

    async fn username_exists(&self, _username: &Username) -> RepoResult<bool> {
        Ok(self.username_exists)
    }
}

/// Test-only no-op implementation of the fifth generic parameter required
/// by `RegisterUserUseCase`. Application-level tests cannot depend on
/// `infra`'s concrete repositories, so we define a local stand-in here.
#[derive(Clone, Default)]
struct MockRegistrationTransaction;

#[async_trait]
impl RegistrationTransaction for MockRegistrationTransaction {
    async fn register_user_with_tenant(
        &self,
        _user: &User,
        _tenant: &Tenant,
        _membership: &TenantMembership,
    ) -> RepoResult<()> {
        Ok(())
    }
}

fn make_use_case(
    email_exists: bool,
    username_exists: bool,
) -> RegisterUserUseCase<
    MockUserRepository,
    UuidIdGenerator,
    Argon2PasswordHasher,
    JsonWebTokenService,
    MockRegistrationTransaction,
> {
    RegisterUserUseCase::new(
        MockUserRepository {
            email_exists,
            username_exists,
        },
        UuidIdGenerator::new(),
        Argon2PasswordHasher,
        JsonWebTokenService::new("test-secret"),
        MockRegistrationTransaction,
    )
}

fn valid_request() -> RegisterRequest {
    RegisterRequest {
        username: "validuser".into(),
        email: "user@example.com".into(),
        password: "SecurePassword123!".into(),
    }
}

#[tokio::test]
async fn email_and_username_collisions_produce_identical_error_message() {
    let email_collision_use_case = make_use_case(true, false);
    let email_err = email_collision_use_case
        .execute(valid_request())
        .await
        .unwrap_err();

    let username_collision_use_case = make_use_case(false, true);
    let username_err = username_collision_use_case
        .execute(valid_request())
        .await
        .unwrap_err();

    assert_eq!(email_err.to_string(), username_err.to_string());
    assert_eq!(
        email_err.to_string(),
        "registration failed: email or username already in use"
    );

    assert_eq!(
        email_err.to_string(),
        "registration failed: email or username already in use"
    );
    assert_eq!(
        username_err.to_string(),
        "registration failed: email or username already in use"
    );
}

#[tokio::test]
async fn other_validation_errors_keep_specific_messages() {
    let use_case = make_use_case(false, false);

    let mut request = valid_request();
    request.password = "short".into();

    let err = use_case.execute(request).await.unwrap_err();

    assert_eq!(
        err.to_string(),
        "password must be at least 12 characters long"
    );
}
