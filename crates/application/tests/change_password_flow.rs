//! Application-layer coverage for `ChangePasswordUseCase`'s
//! fail-closed session-invalidation fix.
//!
//! Mirrors `reset_password_flow.rs`'s style: real in-memory
//! `InMemoryUserRepository` plus the REAL `Argon2PasswordHasher`, and a
//! local `PendingAuthCache` fake since no live Redis is available (or
//! desirable) for a unit test.
//!
//! The critical regression this file guards against: before this fix,
//! `ChangePasswordUseCase::execute` discarded the `PendingAuthCache::set`
//! error with `let _ = ...`, so a Redis outage let the password change
//! succeed while silently skipping session invalidation (fail-open). The
//! fix makes that write mandatory and ordered before the durable
//! password update, identical to `ResetPasswordUseCase`.

use std::{
    error::Error,
    sync::{Arc, Mutex},
};

use application::ports::{
    PasswordHasher, PendingAuthCache, PendingAuthCacheResult, UserRepository,
};
use application::use_cases::{
    ChangePasswordError, ChangePasswordUseCase, session_invalidated_at_cache_key,
};
use async_trait::async_trait;
use chrono::Utc;
use domain::{Email, PasswordHash, User, UserBuilder, UserId, UserRole, UserStatus, Username};
use infra::{Argon2PasswordHasher, InMemoryUserRepository};

const STRONG_PASSWORD: &str = "CorrectPassword123!";
const NEW_STRONG_PASSWORD: &str = "BrandNewPassword456!";

/// Working fake — behaves like a healthy Redis.
#[derive(Clone, Default)]
struct FakePendingAuthCache {
    store: Arc<Mutex<std::collections::HashMap<String, String>>>,
}

#[async_trait]
impl PendingAuthCache for FakePendingAuthCache {
    async fn get(&self, key: &str) -> PendingAuthCacheResult<Option<String>> {
        Ok(self.store.lock().unwrap().get(key).cloned())
    }

    async fn getdel(&self, key: &str) -> PendingAuthCacheResult<Option<String>> {
        Ok(self.store.lock().unwrap().remove(key))
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

/// Simulates a Redis/PendingAuthCache outage: every `set` fails, exactly
/// like a real `RedisPendingAuthCache` would when Redis is unreachable.
#[derive(Clone, Default)]
struct FailingPendingAuthCache;

#[async_trait]
impl PendingAuthCache for FailingPendingAuthCache {
    async fn get(&self, _key: &str) -> PendingAuthCacheResult<Option<String>> {
        Err("simulated redis outage".into())
    }

    async fn getdel(&self, _key: &str) -> PendingAuthCacheResult<Option<String>> {
        Err("simulated redis outage".into())
    }

    async fn set(&self, _key: &str, _value: &str, _ttl_seconds: u64) -> PendingAuthCacheResult<()> {
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
        .username(Username::new("changepwuser").unwrap())
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

#[tokio::test]
async fn happy_path_changes_password_and_invalidates_session()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let user_repository = InMemoryUserRepository::new();
    let user = make_active_user("changepw-happy@example.com", STRONG_PASSWORD).await;
    user_repository.create(&user).await?;

    let pending_auth_cache = FakePendingAuthCache::default();

    let use_case = ChangePasswordUseCase::new(
        user_repository.clone(),
        Argon2PasswordHasher,
        pending_auth_cache.clone(),
    );

    use_case
        .execute(
            user.id(),
            STRONG_PASSWORD.to_string(),
            NEW_STRONG_PASSWORD.to_string(),
        )
        .await
        .expect("change should succeed with the correct current password");

    let updated = user_repository
        .find_by_id(&user.id())
        .await?
        .expect("user must still exist");

    assert!(
        Argon2PasswordHasher
            .verify(NEW_STRONG_PASSWORD, updated.password_hash().as_str())
            .await?,
        "stored password hash must verify against the NEW password"
    );

    let cache_key = session_invalidated_at_cache_key(user.id());
    assert!(
        pending_auth_cache.get(&cache_key).await?.is_some(),
        "a successful change must write a session-invalidated-at marker"
    );

    Ok(())
}

#[tokio::test]
async fn wrong_current_password_is_rejected() -> Result<(), Box<dyn Error + Send + Sync>> {
    let user_repository = InMemoryUserRepository::new();
    let user = make_active_user("changepw-wrong@example.com", STRONG_PASSWORD).await;
    user_repository.create(&user).await?;

    let use_case = ChangePasswordUseCase::new(
        user_repository.clone(),
        Argon2PasswordHasher,
        FakePendingAuthCache::default(),
    );

    let result = use_case
        .execute(
            user.id(),
            "TotallyWrongPassword999!".to_string(),
            NEW_STRONG_PASSWORD.to_string(),
        )
        .await;

    assert!(matches!(
        result,
        Err(ChangePasswordError::InvalidCurrentPassword)
    ));

    let unchanged = user_repository.find_by_id(&user.id()).await?.unwrap();
    assert!(
        Argon2PasswordHasher
            .verify(STRONG_PASSWORD, unchanged.password_hash().as_str())
            .await?,
        "password must remain the OLD password after a rejected attempt"
    );

    Ok(())
}

#[tokio::test]
async fn weak_new_password_is_rejected() -> Result<(), Box<dyn Error + Send + Sync>> {
    let user_repository = InMemoryUserRepository::new();
    let user = make_active_user("changepw-weak@example.com", STRONG_PASSWORD).await;
    user_repository.create(&user).await?;

    let use_case = ChangePasswordUseCase::new(
        user_repository.clone(),
        Argon2PasswordHasher,
        FakePendingAuthCache::default(),
    );

    let result = use_case
        .execute(user.id(), STRONG_PASSWORD.to_string(), "short".to_string())
        .await;

    assert!(matches!(result, Err(ChangePasswordError::WeakPassword(_))));

    Ok(())
}

/// THE REGRESSION TEST for the fail-open bug this worker fixes.
///
/// When `PendingAuthCache::set` fails (simulated Redis outage), the whole
/// `execute` call must fail AND the password must remain unchanged in
/// the database. Before the fix, the password was changed first and the
/// cache error was silently discarded, leaving old sessions valid.
#[tokio::test]
async fn redis_outage_fails_the_request_and_leaves_password_unchanged()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let user_repository = InMemoryUserRepository::new();
    let user = make_active_user("changepw-redisdown@example.com", STRONG_PASSWORD).await;
    user_repository.create(&user).await?;

    let use_case = ChangePasswordUseCase::new(
        user_repository.clone(),
        Argon2PasswordHasher,
        FailingPendingAuthCache,
    );

    let result = use_case
        .execute(
            user.id(),
            STRONG_PASSWORD.to_string(),
            NEW_STRONG_PASSWORD.to_string(),
        )
        .await;

    assert!(
        matches!(result, Err(ChangePasswordError::Internal(_))),
        "a PendingAuthCache outage must fail the whole change-password \
         request, not silently succeed"
    );

    let unchanged = user_repository
        .find_by_id(&user.id())
        .await?
        .expect("user must still exist");

    assert!(
        Argon2PasswordHasher
            .verify(STRONG_PASSWORD, unchanged.password_hash().as_str())
            .await?,
        "password must remain the OLD password — a Redis outage must not \
         let the password change go through while session invalidation \
         silently fails (this is the fail-open regression this test \
         guards against)"
    );
    assert!(
        !Argon2PasswordHasher
            .verify(NEW_STRONG_PASSWORD, unchanged.password_hash().as_str())
            .await?,
        "the NEW password must NOT have been persisted when the \
         session-invalidation write failed"
    );

    Ok(())
}
