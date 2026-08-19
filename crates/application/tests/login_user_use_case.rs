use std::error::Error;

use application::dto::LoginRequest;
use application::ports::{PasswordHasher, UserRepository};
use application::use_cases::LoginUserUseCase;
use async_trait::async_trait;
use chrono::Utc;
use domain::{Email, PasswordHash, User, UserBuilder, UserId, UserRole, UserStatus, Username};
use infra::{Argon2PasswordHasher, JsonWebTokenService};

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

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
) -> LoginUserUseCase<MockUserRepository, Argon2PasswordHasher, JsonWebTokenService> {
    LoginUserUseCase::new(
        MockUserRepository { user },
        Argon2PasswordHasher,
        JsonWebTokenService::new("test-secret"),
    )
}

/// THE regression test for Bug #3.
///
/// Before the fix, a login attempt for a non-existent email hit the
/// malformed `DUMMY_PASSWORD_HASH` constant, which `PasswordHash::new()`
/// failed to parse, producing a DISTINCT error
/// ("invalid password hash: invalid Base64 encoding") that never went
/// through the generic "invalid email or password" path a wrong-password
/// attempt against a real account produces. That distinction was a hard
/// user-enumeration oracle via response content alone.
///
/// This test asserts both paths now produce the exact same error string,
/// so if this constant, or the `?`-vs-`unwrap_or(false)` handling around
/// `verify()`, ever regresses to reintroduce a distinguishable error,
/// this test fails.
#[tokio::test]
async fn nonexistent_email_and_wrong_password_for_existing_user_produce_identical_error() {
    // Path A: email does not exist at all.
    let nonexistent_email_use_case = make_use_case(None);
    let nonexistent_email_err = nonexistent_email_use_case
        .execute(LoginRequest {
            email: "nonexistent@example.com".into(),
            password: "WrongPassword999!".into(),
        })
        .await
        .unwrap_err();

    // Path B: email exists, but the password is wrong.
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

    // Explicitly guard against the exact regression that caused Bug #3:
    // the error must never leak hashing/parsing internals.
    assert!(!nonexistent_email_err.to_string().contains("Base64"));
    assert!(!nonexistent_email_err.to_string().contains("hash"));
}

/// Sanity check: a correct password for an existing, active user must
/// still succeed. This guards against an overly-broad fix (e.g. someone
/// "fixing" Bug #3 by making `verify()` always return `false`) that would
/// technically close the oracle by breaking login entirely.
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

/// A password longer than `MAX_PASSWORD_LENGTH` must still return the
/// generic "invalid email or password" message, never a distinct
/// length-related message — otherwise the length check itself becomes a
/// second, smaller oracle.
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
