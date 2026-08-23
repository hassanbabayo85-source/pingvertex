//! B6 — application-layer coverage for the forgot-password /
//! reset-password flow: happy path, expired token, reused token, and
//! account-enumeration protection.
//!
//! Follows the exact style of `login_user_use_case.rs`: real in-memory
//! infra doubles (`InMemoryUserRepository`, the new
//! `InMemoryResetPasswordTokenRepository`) plus the REAL
//! `Argon2PasswordHasher`/`UuidIdGenerator`, so this actually exercises
//! Argon2 hash/verify and the raw-token prefix format, not just
//! use-case-internal plumbing. `PendingAuthCache` and
//! `PasswordResetMailer` are local fakes since no live Redis/SMTP is
//! available (or desirable) for a unit test.

use std::{
    error::Error,
    sync::{Arc, Mutex},
};

use application::dto::ForgotPasswordRequest;
use application::ports::{
    PasswordHasher, PasswordResetMailer, PasswordResetMailerResult, PendingAuthCache,
    PendingAuthCacheResult, ResetPasswordTokenRepository, UserRepository,
};
use application::use_cases::{
    RequestPasswordResetUseCase, ResetPasswordError, ResetPasswordUseCase,
    session_invalidated_at_cache_key,
};
use async_trait::async_trait;
use chrono::Utc;
use domain::{
    Email, PasswordHash, ResetPasswordToken, ResetPasswordTokenId, User, UserBuilder, UserId,
    UserRole, UserStatus, Username,
};
use infra::{
    Argon2PasswordHasher, InMemoryResetPasswordTokenRepository, InMemoryUserRepository,
    UuidIdGenerator,
};

const STRONG_PASSWORD: &str = "CorrectPassword123!";
const NEW_STRONG_PASSWORD: &str = "BrandNewPassword456!";

#[derive(Clone, Default)]
struct FakeMailer {
    sent: Arc<Mutex<Vec<(String, String)>>>,
}

#[async_trait]
impl PasswordResetMailer for FakeMailer {
    async fn send_password_reset_email(
        &self,
        to: &Email,
        raw_token: &str,
    ) -> PasswordResetMailerResult<()> {
        self.sent
            .lock()
            .unwrap()
            .push((to.as_str().to_string(), raw_token.to_string()));
        Ok(())
    }
}

#[derive(Clone, Default)]
struct FakePendingAuthCache {
    store: Arc<Mutex<std::collections::HashMap<String, String>>>,
}

#[async_trait]
impl PendingAuthCache for FakePendingAuthCache {
    async fn get(&self, key: &str) -> PendingAuthCacheResult<Option<String>> {
        Ok(self.store.lock().unwrap().get(key).cloned())
    }

    async fn set(&self, key: &str, value: &str, _ttl_seconds: u64) -> PendingAuthCacheResult<()> {
        self.store
            .lock()
            .unwrap()
            .insert(key.to_string(), value.to_string());
        Ok(())
    }

    async fn delete(&self, key: &str) -> PendingAuthCacheResult<()> {
        self.store.lock().unwrap().remove(key);
        Ok(())
    }
}

async fn make_active_user(email: &str, password: &str) -> User {
    let hash = Argon2PasswordHasher.hash(password).await.unwrap();
    let now = Utc::now();

    UserBuilder::new()
        .id(UserId::new())
        .username(Username::new("resetflowuser").unwrap())
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

struct Harness {
    user_repository: InMemoryUserRepository,
    token_repository: InMemoryResetPasswordTokenRepository,
    mailer: FakeMailer,
    pending_auth_cache: FakePendingAuthCache,
}

impl Harness {
    fn new() -> Self {
        Self {
            user_repository: InMemoryUserRepository::new(),
            token_repository: InMemoryResetPasswordTokenRepository::new(),
            mailer: FakeMailer::default(),
            pending_auth_cache: FakePendingAuthCache::default(),
        }
    }

    fn forgot_password_use_case(
        &self,
    ) -> RequestPasswordResetUseCase<
        InMemoryUserRepository,
        InMemoryResetPasswordTokenRepository,
        UuidIdGenerator,
        Argon2PasswordHasher,
        FakeMailer,
    > {
        RequestPasswordResetUseCase::new(
            self.user_repository.clone(),
            self.token_repository.clone(),
            UuidIdGenerator::new(),
            Argon2PasswordHasher,
            self.mailer.clone(),
        )
    }

    fn reset_password_use_case(
        &self,
    ) -> ResetPasswordUseCase<
        InMemoryResetPasswordTokenRepository,
        InMemoryUserRepository,
        Argon2PasswordHasher,
        FakePendingAuthCache,
    > {
        ResetPasswordUseCase::new(
            self.token_repository.clone(),
            self.user_repository.clone(),
            Argon2PasswordHasher,
            self.pending_auth_cache.clone(),
        )
    }

    fn last_sent_token(&self) -> String {
        self.mailer.sent.lock().unwrap().last().unwrap().1.clone()
    }
}

#[tokio::test]
async fn full_forgot_password_then_reset_password_happy_path()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let harness = Harness::new();
    let user = make_active_user("resetflow-happy@example.com", STRONG_PASSWORD).await;
    harness.user_repository.create(&user).await?;

    harness
        .forgot_password_use_case()
        .execute(ForgotPasswordRequest {
            email: "resetflow-happy@example.com".to_string(),
        })
        .await?;

    let raw_token = harness.last_sent_token();

    harness
        .reset_password_use_case()
        .execute(raw_token, NEW_STRONG_PASSWORD.to_string())
        .await
        .expect("reset should succeed with a freshly issued, valid token");

    let updated = harness
        .user_repository
        .find_by_id(&user.id())
        .await?
        .expect("user must still exist");

    assert!(
        Argon2PasswordHasher
            .verify(NEW_STRONG_PASSWORD, updated.password_hash().as_str())
            .await?,
        "stored password hash must verify against the NEW password"
    );
    assert!(
        !Argon2PasswordHasher
            .verify(STRONG_PASSWORD, updated.password_hash().as_str())
            .await?,
        "stored password hash must NOT verify against the OLD password anymore"
    );

    // Session invalidation marker must have been written.
    let cache_key = session_invalidated_at_cache_key(user.id());
    assert!(
        harness.pending_auth_cache.get(&cache_key).await?.is_some(),
        "a successful reset must write a session-invalidated-at marker"
    );

    Ok(())
}

#[tokio::test]
async fn expired_token_is_rejected() -> Result<(), Box<dyn Error + Send + Sync>> {
    let harness = Harness::new();
    let user = make_active_user("resetflow-expired@example.com", STRONG_PASSWORD).await;
    harness.user_repository.create(&user).await?;

    // Build an already-expired token directly (bypassing the forgot-password
    // use case, which always issues a token with a future expiry) and insert
    // it straight into the repository — the raw token value itself is
    // otherwise perfectly well-formed and its hash matches.
    let raw_token = format!("rst_{}_{}", "a".repeat(16), "b".repeat(64));
    let token_hash = Argon2PasswordHasher.hash(&raw_token).await?;
    let expired_token = ResetPasswordToken::new(
        ResetPasswordTokenId::new(),
        user.id(),
        "rst_aaaaaaaaaaaaaaaa".to_string(),
        token_hash,
        Utc::now() - chrono::Duration::minutes(1),
        Utc::now() - chrono::Duration::minutes(31),
    );
    harness.token_repository.create(&expired_token).await?;

    let result = harness
        .reset_password_use_case()
        .execute(raw_token, NEW_STRONG_PASSWORD.to_string())
        .await;

    assert!(matches!(
        result,
        Err(ResetPasswordError::InvalidOrExpiredToken)
    ));

    Ok(())
}

#[tokio::test]
async fn reused_token_is_rejected() -> Result<(), Box<dyn Error + Send + Sync>> {
    let harness = Harness::new();
    let user = make_active_user("resetflow-reuse@example.com", STRONG_PASSWORD).await;
    harness.user_repository.create(&user).await?;

    harness
        .forgot_password_use_case()
        .execute(ForgotPasswordRequest {
            email: "resetflow-reuse@example.com".to_string(),
        })
        .await?;

    let raw_token = harness.last_sent_token();

    // First redemption succeeds.
    harness
        .reset_password_use_case()
        .execute(raw_token.clone(), NEW_STRONG_PASSWORD.to_string())
        .await
        .expect("first redemption must succeed");

    // Second redemption of the SAME raw token must be rejected — the
    // token was marked used (and any unused duplicates deleted) by the
    // first call.
    let second_result = harness
        .reset_password_use_case()
        .execute(raw_token, "AnotherPassword789!".to_string())
        .await;

    assert!(matches!(
        second_result,
        Err(ResetPasswordError::InvalidOrExpiredToken)
    ));

    Ok(())
}

#[tokio::test]
async fn enumeration_protection_returns_ok_for_nonexistent_email()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let harness = Harness::new();
    // No user is ever created — the email below does not exist.

    let result = harness
        .forgot_password_use_case()
        .execute(ForgotPasswordRequest {
            email: "nobody-at-all@example.com".to_string(),
        })
        .await;

    assert!(
        result.is_ok(),
        "forgot-password for a nonexistent email must return Ok, never an error"
    );
    assert!(
        harness.mailer.sent.lock().unwrap().is_empty(),
        "no reset email may be sent for a nonexistent account"
    );

    Ok(())
}

#[tokio::test]
async fn weak_new_password_is_rejected_without_consuming_the_token()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let harness = Harness::new();
    let user = make_active_user("resetflow-weak@example.com", STRONG_PASSWORD).await;
    harness.user_repository.create(&user).await?;

    harness
        .forgot_password_use_case()
        .execute(ForgotPasswordRequest {
            email: "resetflow-weak@example.com".to_string(),
        })
        .await?;

    let raw_token = harness.last_sent_token();

    let weak_result = harness
        .reset_password_use_case()
        .execute(raw_token.clone(), "short".to_string())
        .await;
    assert!(matches!(
        weak_result,
        Err(ResetPasswordError::WeakPassword(_))
    ));

    // The token must still be redeemable afterward with a strong password
    // — a rejected weak password must not have consumed the token.
    harness
        .reset_password_use_case()
        .execute(raw_token, NEW_STRONG_PASSWORD.to_string())
        .await
        .expect("token must still be valid after a prior weak-password rejection");

    Ok(())
}
