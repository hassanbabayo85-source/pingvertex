//! Application-layer concurrency tests for `VerifyTotpAndLoginUseCase`.
//!
//! These tests prove the two atomicity guarantees required by the task:
//!
//! 1. Pending 2FA token consumption is atomic (`getdel` semantics): two
//!    requests using the SAME `pending_token` cannot both succeed. One
//!    request consumes the token and the other fails closed with
//!    `InvalidPendingTwoFactorToken`.
//!
//! 2. Backup code redemption is atomic (conditional `UPDATE ... WHERE
//!    used_at IS NULL` semantics): two requests using DIFFERENT pending
//!    tokens but the SAME backup code cannot both succeed. Exactly one
//!    marks the code as used; the other receives `InvalidBackupCode`.

use std::error::Error;
use std::sync::Arc;

use application::{
    dto::{AuthResponse, VerifyTotpRequest},
    ports::{
        BackupCodeRepository, PasswordHasher, PendingAuthCache, PendingAuthCacheResult,
        SecretCipher, SecretCipherError, UserRepository,
    },
    use_cases::VerifyTotpAndLoginUseCase,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{BackupCode, BackupCodeId, User, UserBuilder, UserId};
use infra::{InMemoryUserRepository, JsonWebTokenService};

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

// ---------------------------------------------------------------------------
// PendingAuthCache fake with an atomic getdel, mirroring Redis GETDEL.
// ---------------------------------------------------------------------------
#[derive(Clone, Default)]
struct AtomicPendingAuthCache {
    store: Arc<tokio::sync::Mutex<std::collections::HashMap<String, String>>>,
}

impl AtomicPendingAuthCache {
    fn new() -> Self {
        Self {
            store: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
        }
    }
}

#[async_trait]
impl PendingAuthCache for AtomicPendingAuthCache {
    async fn get(&self, key: &str) -> PendingAuthCacheResult<Option<String>> {
        let store = self.store.lock().await;
        Ok(store.get(key).cloned())
    }

    async fn getdel(&self, key: &str) -> PendingAuthCacheResult<Option<String>> {
        let mut store = self.store.lock().await;
        Ok(store.remove(key))
    }

    async fn set(&self, key: &str, value: &str, _ttl_seconds: u64) -> PendingAuthCacheResult<()> {
        let mut store = self.store.lock().await;
        store.insert(key.to_string(), value.to_string());
        Ok(())
    }

    async fn delete(&self, key: &str) -> PendingAuthCacheResult<()> {
        let mut store = self.store.lock().await;
        store.remove(key);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// BackupCodeRepository fake with an atomic mark_used, mirroring the
// conditional UPDATE ... WHERE used_at IS NULL.
// ---------------------------------------------------------------------------
#[derive(Clone, Default)]
struct AtomicBackupCodeRepository {
    codes: Arc<tokio::sync::Mutex<std::collections::HashMap<BackupCodeId, BackupCode>>>,
}

impl AtomicBackupCodeRepository {
    fn new() -> Self {
        Self {
            codes: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
        }
    }
}

#[async_trait]
impl BackupCodeRepository for AtomicBackupCodeRepository {
    async fn create_many(&self, codes: &[BackupCode]) -> RepoResult<()> {
        let mut store = self.codes.lock().await;
        for code in codes {
            store.insert(code.id, code.clone());
        }
        Ok(())
    }

    async fn find_all_by_user(&self, user_id: UserId) -> RepoResult<Vec<BackupCode>> {
        let store = self.codes.lock().await;
        Ok(store
            .values()
            .filter(|code| code.user_id == user_id)
            .cloned()
            .collect())
    }

    async fn mark_used(&self, id: BackupCodeId, used_at: DateTime<Utc>) -> RepoResult<bool> {
        let mut store = self.codes.lock().await;
        if let Some(code) = store.get_mut(&id)
            && code.used_at.is_none()
        {
            code.mark_used(used_at);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn delete_all_for_user(&self, user_id: UserId) -> RepoResult<()> {
        let mut store = self.codes.lock().await;
        store.retain(|_, code| code.user_id != user_id);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Fakes that make TOTP fail so the use case falls back to backup codes.
// ---------------------------------------------------------------------------
#[derive(Clone, Default)]
struct FailingSecretCipher;

impl SecretCipher for FailingSecretCipher {
    fn encrypt_bytes(&self, _plaintext: &[u8]) -> Result<Vec<u8>, SecretCipherError> {
        Err(SecretCipherError::EncryptionFailed(
            "not used in this test".into(),
        ))
    }

    fn decrypt_bytes(&self, _ciphertext: &[u8]) -> Result<Vec<u8>, SecretCipherError> {
        Err(SecretCipherError::DecryptionFailed(
            "not used in this test".into(),
        ))
    }
}

/// PasswordHasher fake that matches exactly one raw backup code against
/// the stored `code_hash` value. This keeps the test deterministic without
/// needing the real Argon2 hasher here.
#[derive(Clone, Default)]
struct FakePasswordHasher;

#[async_trait]
impl PasswordHasher for FakePasswordHasher {
    async fn hash(&self, _raw: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
        Ok("fake-hash".to_string())
    }

    async fn verify(&self, raw: &str, hashed: &str) -> Result<bool, Box<dyn Error + Send + Sync>> {
        Ok(raw == hashed)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------
fn pending_login_cache_key_for(token: &str) -> String {
    format!("totp:pending_login:{token}")
}

async fn make_active_user_with_totp(email: &str) -> User {
    let now = Utc::now();
    let mut user = UserBuilder::new()
        .id(UserId::new())
        .username(domain::Username::new("h3user").unwrap())
        .email(domain::Email::new(email).unwrap())
        .password_hash(domain::PasswordHash::new("fake").unwrap())
        .role(domain::UserRole::User)
        .status(domain::UserStatus::Active)
        .email_verified_at(Some(now))
        .created_at(now)
        .updated_at(now)
        .build()
        .unwrap();

    // Enable 2FA with an empty encrypted secret; FailingSecretCipher will
    // cause the TOTP path to fail, guaranteeing the backup-code path.
    user.enable_totp(Vec::new(), now);
    user
}

async fn make_use_case(
    user_repo: InMemoryUserRepository,
    cache: AtomicPendingAuthCache,
    backup_repo: AtomicBackupCodeRepository,
) -> Arc<
    VerifyTotpAndLoginUseCase<
        InMemoryUserRepository,
        FailingSecretCipher,
        FakePasswordHasher,
        JsonWebTokenService,
        AtomicPendingAuthCache,
        AtomicBackupCodeRepository,
    >,
> {
    Arc::new(VerifyTotpAndLoginUseCase::new(
        user_repo,
        FailingSecretCipher,
        FakePasswordHasher,
        JsonWebTokenService::new("test-secret"),
        cache,
        backup_repo,
    ))
}

async fn setup(
    user: &User,
    cache: &AtomicPendingAuthCache,
    backup_repo: &AtomicBackupCodeRepository,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    cache
        .set(
            &pending_login_cache_key_for("token-valid"),
            &user.id().to_string(),
            300,
        )
        .await?;
    cache
        .set(
            &pending_login_cache_key_for("token-also-valid"),
            &user.id().to_string(),
            300,
        )
        .await?;

    let raw_backup_code = "backup-code-123".to_string();
    let backup_code = BackupCode::new(
        BackupCodeId::new(),
        user.id(),
        raw_backup_code.clone(), // stored as code_hash for simplicity
        Utc::now(),
    );
    backup_repo.create_many(&[backup_code]).await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Test 1: pending token atomicity
// ---------------------------------------------------------------------------
#[tokio::test]
async fn concurrent_pending_token_redemption_allows_only_one_success() {
    let user_repo = InMemoryUserRepository::new();
    let user = make_active_user_with_totp("h3-pending@example.com").await;
    user_repo.create(&user).await.expect("create user");

    let cache = AtomicPendingAuthCache::new();
    let backup_repo = AtomicBackupCodeRepository::new();
    setup(&user, &cache, &backup_repo).await.expect("setup");

    let use_case = make_use_case(user_repo.clone(), cache.clone(), backup_repo).await;

    let token = "token-valid".to_string();
    let code = "backup-code-123".to_string();

    let uc_a = use_case.clone();
    let uc_b = use_case.clone();
    let req_a = VerifyTotpRequest {
        pending_token: token.clone(),
        code: code.clone(),
    };
    let req_b = VerifyTotpRequest {
        pending_token: token,
        code,
    };

    let handle_a = tokio::spawn(async move { uc_a.execute(req_a).await });
    let handle_b = tokio::spawn(async move { uc_b.execute(req_b).await });

    let (res_a, res_b) = tokio::join!(handle_a, handle_b);
    let res_a = res_a.expect("task A panicked");
    let res_b = res_b.expect("task B panicked");

    let success_count = [res_a.is_ok(), res_b.is_ok()]
        .into_iter()
        .filter(|x| *x)
        .count();
    assert_eq!(success_count, 1, "exactly one request must succeed");

    let (ok_result, err_result) = if res_a.is_ok() {
        (res_a, res_b)
    } else {
        (res_b, res_a)
    };

    assert!(ok_result.is_ok());
    let _auth: AuthResponse = ok_result.expect("success result");
    let err = err_result.expect_err("failure result");
    assert_eq!(err.to_string(), "invalid or expired pending 2FA token");
}

// ---------------------------------------------------------------------------
// Test 2: backup code atomicity
// ---------------------------------------------------------------------------
#[tokio::test]
async fn concurrent_backup_code_redemption_allows_only_one_success() {
    let user_repo = InMemoryUserRepository::new();
    let user = make_active_user_with_totp("h3-backup@example.com").await;
    user_repo.create(&user).await.expect("create user");

    let cache = AtomicPendingAuthCache::new();
    let backup_repo = AtomicBackupCodeRepository::new();
    setup(&user, &cache, &backup_repo).await.expect("setup");

    let use_case = make_use_case(user_repo.clone(), cache.clone(), backup_repo).await;

    let token_a = "token-valid".to_string();
    let token_b = "token-also-valid".to_string();
    let code = "backup-code-123".to_string();

    let uc_a = use_case.clone();
    let uc_b = use_case.clone();
    let req_a = VerifyTotpRequest {
        pending_token: token_a,
        code: code.clone(),
    };
    let req_b = VerifyTotpRequest {
        pending_token: token_b,
        code,
    };

    let handle_a = tokio::spawn(async move { uc_a.execute(req_a).await });
    let handle_b = tokio::spawn(async move { uc_b.execute(req_b).await });

    let (res_a, res_b) = tokio::join!(handle_a, handle_b);
    let res_a = res_a.expect("task A panicked");
    let res_b = res_b.expect("task B panicked");

    let success_count = [res_a.is_ok(), res_b.is_ok()]
        .into_iter()
        .filter(|x| *x)
        .count();
    assert_eq!(success_count, 1, "exactly one request must succeed");

    let (ok_result, err_result) = if res_a.is_ok() {
        (res_a, res_b)
    } else {
        (res_b, res_a)
    };

    assert!(ok_result.is_ok());
    let _auth: AuthResponse = ok_result.expect("success result");
    let err = err_result.expect_err("failure result");
    assert_eq!(err.to_string(), "invalid or already-used backup code");
}
