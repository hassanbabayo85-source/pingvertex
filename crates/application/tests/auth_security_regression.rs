use std::{
    error::Error,
    sync::{Arc, Mutex},
};

use application::dto::LoginRequest;
use application::ports::{
    PasswordHasher, PendingAuthCache, PendingAuthCacheResult, ResetPasswordTokenRepository,
    UserRepository,
};
use application::use_cases::{
    ChangePasswordError, ChangePasswordUseCase, LoginUserUseCase, ResetPasswordError,
    ResetPasswordUseCase, session_invalidated_at_cache_key, session_revocation_timestamp,
};
use async_trait::async_trait;
use chrono::{Duration, Utc};
use domain::{
    Email, PasswordHash, ResetPasswordToken, ResetPasswordTokenId, User, UserBuilder, UserId,
    UserRole, UserStatus, Username,
};
use infra::{
    Argon2PasswordHasher, InMemoryResetPasswordTokenRepository, InMemoryUserRepository,
    JsonWebTokenService, UuidIdGenerator,
};

const CURRENT_PW: &str = "CorrectPassword123!";
const NEW_PW: &str = "BrandNewPassword456!";
const TOKEN_PREFIX: &str = "rst_aaaaaaaaaaaaaaaa";
const TEST_JWT_SECRET: &str = "test-secret-please-do-not-use-in-prod";

#[derive(Clone, Default)]
struct RecordingCache {
    store: Arc<Mutex<std::collections::HashMap<String, String>>>,
}

#[async_trait]
impl PendingAuthCache for RecordingCache {
    async fn get(&self, key: &str) -> PendingAuthCacheResult<Option<String>> {
        Ok(self.store.lock().unwrap().get(key).cloned())
    }
    async fn getdel(&self, key: &str) -> PendingAuthCacheResult<Option<String>> {
        Ok(self.store.lock().unwrap().remove(key))
    }
    async fn set(&self, key: &str, value: &str, _ttl: u64) -> PendingAuthCacheResult<()> {
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

#[derive(Clone, Default)]
struct FailingCache;

#[async_trait]
impl PendingAuthCache for FailingCache {
    async fn get(&self, _key: &str) -> PendingAuthCacheResult<Option<String>> {
        Err("simulated redis outage".into())
    }
    async fn getdel(&self, _key: &str) -> PendingAuthCacheResult<Option<String>> {
        Err("simulated redis outage".into())
    }
    async fn set(&self, _key: &str, _value: &str, _ttl: u64) -> PendingAuthCacheResult<()> {
        Err("simulated redis outage".into())
    }
    async fn delete(&self, _key: &str) -> PendingAuthCacheResult<()> {
        Err("simulated redis outage".into())
    }
}

async fn make_active_user(email: &str, password: &str) -> User {
    let hash = Argon2PasswordHasher.hash(password).await.unwrap();
    let now = Utc::now();
    UserBuilder::new()
        .id(UserId::new())
        .username(Username::new("securityregression").unwrap())
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

async fn make_pending_user(email: &str, password: &str) -> User {
    let hash = Argon2PasswordHasher.hash(password).await.unwrap();
    let now = Utc::now();
    UserBuilder::new()
        .id(UserId::new())
        .username(Username::new("pendingregression").unwrap())
        .email(Email::new(email).unwrap())
        .password_hash(PasswordHash::new(hash).unwrap())
        .role(UserRole::User)
        .status(UserStatus::PendingVerification)
        .created_at(now)
        .updated_at(now)
        .build()
        .unwrap()
}

async fn insert_token(
    repo: &InMemoryResetPasswordTokenRepository,
    user_id: UserId,
    raw_token: &str,
    expires_in_minutes: i64,
) {
    let hash = Argon2PasswordHasher.hash(raw_token).await.unwrap();
    let now = Utc::now();
    let token = ResetPasswordToken {
        id: ResetPasswordTokenId::new(),
        user_id,
        token_prefix: TOKEN_PREFIX.to_string(),
        token_hash: hash,
        expires_at: now + Duration::minutes(expires_in_minutes),
        used_at: None,
        created_at: now,
    };
    repo.create(&token).await.unwrap();
}

fn full_raw_token() -> String {
    format!("rst_{}_{}", "a".repeat(16), "b".repeat(64))
}

async fn current_user_password_hash(repo: &InMemoryUserRepository, id: UserId) -> String {
    repo.find_by_id(&id)
        .await
        .unwrap()
        .unwrap()
        .password_hash()
        .as_str()
        .to_string()
}

#[tokio::test]
async fn password_change_invalidates_same_second_jwt() -> Result<(), Box<dyn Error + Send + Sync>> {
    let users = InMemoryUserRepository::new();
    let user = make_active_user("same-second-change@example.com", CURRENT_PW).await;
    users.create(&user).await?;
    let cache = RecordingCache::default();

    let t_before = Utc::now().timestamp();
    ChangePasswordUseCase::new(users.clone(), Argon2PasswordHasher, cache.clone())
        .execute(user.id(), CURRENT_PW.to_string(), NEW_PW.to_string())
        .await?;
    let t_after = Utc::now().timestamp();

    let marker: i64 = cache
        .get(&session_invalidated_at_cache_key(user.id()))
        .await?
        .unwrap()
        .parse()?;

    assert!(
        marker > t_before,
        "marker ({marker}) must be > t_before ({t_before})"
    );
    assert!(marker <= t_after + 1);
    assert!(t_before < marker);
    Ok(())
}

#[tokio::test]
async fn password_reset_invalidates_same_second_jwt() -> Result<(), Box<dyn Error + Send + Sync>> {
    let users = InMemoryUserRepository::new();
    let user = make_active_user("same-second-reset@example.com", CURRENT_PW).await;
    users.create(&user).await?;
    let tokens = InMemoryResetPasswordTokenRepository::new();
    let raw = full_raw_token();
    insert_token(&tokens, user.id(), &raw, 30).await;
    let cache = RecordingCache::default();

    let t_before = Utc::now().timestamp();
    ResetPasswordUseCase::new(
        tokens.clone(),
        users.clone(),
        Argon2PasswordHasher,
        cache.clone(),
    )
    .execute(raw, NEW_PW.to_string())
    .await?;
    let t_after = Utc::now().timestamp();

    let marker: i64 = cache
        .get(&session_invalidated_at_cache_key(user.id()))
        .await?
        .unwrap()
        .parse()?;

    assert!(marker > t_before);
    assert!(marker <= t_after + 1);

    let revoked = |iat: i64| iat < marker;
    assert!(revoked(t_before - 1));
    assert!(revoked(t_before));
    assert!(!revoked(t_after + 1));
    Ok(())
}

#[test]
fn session_revocation_timestamp_is_strictly_newer_than_now() {
    for t in [0_i64, 1, 1_000, 1_700_000_000, i64::MAX - 1] {
        let marker = session_revocation_timestamp(t);
        assert!(marker > t, "t={t} marker={marker}");
    }
}

#[tokio::test]
async fn password_change_with_redis_failure_leaves_password_unchanged()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let users = InMemoryUserRepository::new();
    let user = make_active_user("changepw-redis-fail@example.com", CURRENT_PW).await;
    users.create(&user).await?;
    let before = current_user_password_hash(&users, user.id()).await;

    let result = ChangePasswordUseCase::new(users.clone(), Argon2PasswordHasher, FailingCache)
        .execute(user.id(), CURRENT_PW.to_string(), NEW_PW.to_string())
        .await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        ChangePasswordError::Internal(_)
    ));

    let after = current_user_password_hash(&users, user.id()).await;
    assert_eq!(before, after);
    Ok(())
}

#[tokio::test]
async fn password_reset_with_redis_failure_leaves_password_unchanged_and_token_reusable()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let users = InMemoryUserRepository::new();
    let user = make_active_user("resetpw-redis-fail@example.com", CURRENT_PW).await;
    users.create(&user).await?;
    let tokens = InMemoryResetPasswordTokenRepository::new();
    let raw = full_raw_token();
    insert_token(&tokens, user.id(), &raw, 30).await;
    let before = current_user_password_hash(&users, user.id()).await;

    let result = ResetPasswordUseCase::new(
        tokens.clone(),
        users.clone(),
        Argon2PasswordHasher,
        FailingCache,
    )
    .execute(raw.clone(), NEW_PW.to_string())
    .await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        ResetPasswordError::Internal(_)
    ));

    let after = current_user_password_hash(&users, user.id()).await;
    assert_eq!(before, after);

    let ok = ResetPasswordUseCase::new(
        tokens.clone(),
        users.clone(),
        Argon2PasswordHasher,
        RecordingCache::default(),
    )
    .execute(raw, NEW_PW.to_string())
    .await;
    assert!(ok.is_ok());
    Ok(())
}

#[tokio::test]
async fn reset_token_cannot_be_reused() -> Result<(), Box<dyn Error + Send + Sync>> {
    let users = InMemoryUserRepository::new();
    let user = make_active_user("resetpw-reuse@example.com", CURRENT_PW).await;
    users.create(&user).await?;
    let tokens = InMemoryResetPasswordTokenRepository::new();
    let raw = full_raw_token();
    insert_token(&tokens, user.id(), &raw, 30).await;

    let cache = RecordingCache::default();
    ResetPasswordUseCase::new(
        tokens.clone(),
        users.clone(),
        Argon2PasswordHasher,
        cache.clone(),
    )
    .execute(raw.clone(), NEW_PW.to_string())
    .await?;

    let second =
        ResetPasswordUseCase::new(tokens.clone(), users.clone(), Argon2PasswordHasher, cache)
            .execute(raw, "EvenNewerPassword789!".to_string())
            .await;

    assert!(matches!(
        second.unwrap_err(),
        ResetPasswordError::InvalidOrExpiredToken
    ));
    Ok(())
}

#[tokio::test]
async fn pending_verification_login_returns_same_error_as_wrong_password()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let users = InMemoryUserRepository::new();
    let pending = make_pending_user("pending-leak@example.com", CURRENT_PW).await;
    users.create(&pending).await?;

    let cache = RecordingCache::default();
    let use_case = LoginUserUseCase::new(
        users.clone(),
        Argon2PasswordHasher,
        JsonWebTokenService::new(TEST_JWT_SECRET),
        UuidIdGenerator,
        cache.clone(),
    );

    let pending_err = use_case
        .execute(LoginRequest {
            email: "pending-leak@example.com".to_string(),
            password: CURRENT_PW.to_string(),
        })
        .await
        .unwrap_err()
        .to_string();

    let wrong_err = use_case
        .execute(LoginRequest {
            email: "pending-leak@example.com".to_string(),
            password: "definitely-the-wrong-one".to_string(),
        })
        .await
        .unwrap_err()
        .to_string();

    let nonexistent_err = use_case
        .execute(LoginRequest {
            email: "nobody-here@example.com".to_string(),
            password: CURRENT_PW.to_string(),
        })
        .await
        .unwrap_err()
        .to_string();

    assert_eq!(pending_err, "invalid email or password");
    assert_eq!(wrong_err, pending_err);
    assert_eq!(nonexistent_err, pending_err);
    Ok(())
}
