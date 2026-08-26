use std::error::Error;

use application::dto::LoginRequest;
use application::ports::{
    PasswordHasher, PendingAuthCache, PendingAuthCacheResult, UserRepository,
};
use application::use_cases::LoginUserUseCase;
use async_trait::async_trait;
use chrono::Utc;
use domain::{Email, PasswordHash, User, UserBuilder, UserId, UserRole, UserStatus, Username};
use infra::{Argon2PasswordHasher, JsonWebTokenService, UuidIdGenerator};

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// In-memory `PendingAuthCache` test double — the real
/// `RedisPendingAuthCache` needs an actual Redis instance, which these
/// unit tests have no business depending on.
#[derive(Clone, Default)]
struct InMemoryPendingAuthCache;

#[async_trait]
impl PendingAuthCache for InMemoryPendingAuthCache {
    async fn get(&self, _key: &str) -> PendingAuthCacheResult<Option<String>> {
        Ok(None)
    }

    async fn getdel(&self, _key: &str) -> PendingAuthCacheResult<Option<String>> {
        Ok(None)
    }

    async fn set(&self, _key: &str, _value: &str, _ttl_seconds: u64) -> PendingAuthCacheResult<()> {
        Ok(())
    }

    async fn delete(&self, _key: &str) -> PendingAuthCacheResult<()> {
        Ok(())
    }
}

/// Test double for `UserRepository` whose `find_by_email` response is
/// configured per test, so we can force either the "no such user" path or
/// the "user exists, wrong password" path independently — these are
/// exactly the two paths Bug #3 (the user-enumeration oracle) made
/// distinguishable from each other via response status/body.
struct MockUserRepository {
    user: Option<User>,
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
        Ok(self.user.clone())
    }

    async fn find_by_email(&self, _email: &Email) -> RepoResult<Option<User>> {
        Ok(self.user.clone())
    }

    async fn find_by_username(&self, _username: &Username) -> RepoResult<Option<User>> {
        Ok(self.user.clone())
    }

    async fn delete(&self, _id: &UserId) -> RepoResult<()> {
        Ok(())
    }

    async fn email_exists(&self, _email: &Email) -> RepoResult<bool> {
        Ok(self.user.is_some())
    }

    async fn username_exists(&self, _username: &Username) -> RepoResult<bool> {
        Ok(self.user.is_some())
    }
}

/// Builds a real, Active user whose stored password hash is a genuine
/// Argon2id hash of `password`, produced by the SAME real hasher
/// (`infra::Argon2PasswordHasher`) the use case is wired up with in
/// production. Using the real hasher (not a stub) is deliberate: this is
/// what actually exercises `PasswordHash::new()` parsing, which is where
/// Bug #3's root cause lived.
async fn make_active_user(email: &str, password: &str) -> User {
    let hash = Argon2PasswordHasher.hash(password).await.unwrap();
    let now = Utc::now();

    UserBuilder::new()
        .id(UserId::new())
        .username(Username::new("regressionuser").unwrap())
        .email(Email::new(email).unwrap())
        .password_hash(PasswordHash::new(hash).unwrap())
        .role(UserRole::User)
        .status(UserStatus::Active)
        .email_verified_at(Some(now))
        .created_at(now)
        .updated_at(now)
        .build()
        .unwrap()
}

fn make_use_case(
    user: Option<User>,
) -> LoginUserUseCase<
    MockUserRepository,
    Argon2PasswordHasher,
    JsonWebTokenService,
    UuidIdGenerator,
    InMemoryPendingAuthCache,
> {
    LoginUserUseCase::new(
        MockUserRepository { user },
        Argon2PasswordHasher,
        JsonWebTokenService::new("test-secret"),
        UuidIdGenerator,
        InMemoryPendingAuthCache,
    )
}

#[tokio::test]
async fn nonexistent_email_and_wrong_password_for_existing_user_produce_identical_error() {
    let nonexistent_email_use_case = make_use_case(None);
    let nonexistent_email_err = nonexistent_email_use_case
        .execute(LoginRequest {
            email: "nonexistent@example.com".into(),
            password: "WrongPassword999!".into(),
        })
        .await
        .unwrap_err();

    let existing_user = make_active_user("realuser@example.com", "CorrectPassword123!").await;
    let wrong_password_use_case = make_use_case(Some(existing_user));
    let wrong_password_err = wrong_password_use_case
        .execute(LoginRequest {
            email: "realuser@example.com".into(),
            password: "WrongPassword999!".into(),
        })
        .await
        .unwrap_err();

    assert_eq!(
        nonexistent_email_err.to_string(),
        wrong_password_err.to_string(),
        "a non-existent email and a wrong password for a real account must \
         produce the identical error — any difference is a user-enumeration \
         oracle (this is exactly Bug #3)"
    );

    assert_eq!(
        nonexistent_email_err.to_string(),
        "invalid email or password"
    );

    assert!(!nonexistent_email_err.to_string().contains("Base64"));
    assert!(!nonexistent_email_err.to_string().contains("hash"));
}

#[tokio::test]
async fn correct_password_for_existing_active_user_succeeds() {
    let user = make_active_user("gooduser@example.com", "CorrectPassword123!").await;
    let use_case = make_use_case(Some(user));

    let result = use_case
        .execute(LoginRequest {
            email: "gooduser@example.com".into(),
            password: "CorrectPassword123!".into(),
        })
        .await;

    assert!(
        result.is_ok(),
        "expected successful login with the correct password, got: {:?}",
        result.err().map(|e| e.to_string())
    );
}

#[tokio::test]
async fn overlong_password_produces_generic_error_not_a_distinct_message() {
    let use_case = make_use_case(None);

    let overlong_password = "a".repeat(129);
    let err = use_case
        .execute(LoginRequest {
            email: "whoever@example.com".into(),
            password: overlong_password,
        })
        .await
        .unwrap_err();

    assert_eq!(err.to_string(), "invalid email or password");
}
