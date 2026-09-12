use application::ports::{EmailVerificationTokenRepository, PasswordHasher, UserRepository};
use application::use_cases::{VerifyEmailError, VerifyEmailUseCase};
use async_trait::async_trait;
use chrono::{Duration, Utc};
use domain::{
    Email, EmailVerificationToken, EmailVerificationTokenId, PasswordHash, User, UserBuilder,
    UserId, UserRole, UserStatus, Username,
};
use infra::{InMemoryEmailVerificationTokenRepository, InMemoryUserRepository};

struct PlaintextHasher;

#[async_trait]
impl PasswordHasher for PlaintextHasher {
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

fn valid_raw_token() -> String {
    format!("evt_{}_{}", "0".repeat(16), "0".repeat(64))
}

fn make_user(id: UserId) -> User {
    UserBuilder::new()
        .id(id)
        .username(Username::new("user").unwrap())
        .email(Email::new("user@example.com").unwrap())
        .password_hash(PasswordHash::new("hash".to_string()).unwrap())
        .role(UserRole::User)
        .status(UserStatus::PendingVerification)
        .email_verified_at(None)
        .created_at(Utc::now())
        .updated_at(Utc::now())
        .build()
        .unwrap()
}

fn make_token(
    user_id: UserId,
    raw: &str,
    expires_at: chrono::DateTime<Utc>,
) -> EmailVerificationToken {
    EmailVerificationToken::new(
        EmailVerificationTokenId::new(),
        user_id,
        "evt_0000000000000000".to_string(),
        raw.to_string(),
        expires_at,
        Utc::now(),
    )
}

#[tokio::test]
async fn valid_token_marks_user_verified_and_token_used() {
    let user_id = UserId::generate();
    let raw = valid_raw_token();

    let user_repo = InMemoryUserRepository::new();
    user_repo.create(&make_user(user_id)).await.unwrap();

    let token_repo = InMemoryEmailVerificationTokenRepository::new();
    token_repo
        .create(&make_token(user_id, &raw, Utc::now() + Duration::hours(1)))
        .await
        .unwrap();

    let use_case = VerifyEmailUseCase::new(token_repo.clone(), user_repo.clone(), PlaintextHasher);
    use_case.execute(raw.clone()).await.unwrap();

    let token = token_repo
        .find_by_prefix("evt_0000000000000000")
        .await
        .unwrap()
        .unwrap();
    assert!(token.is_used());

    let user = user_repo.find_by_id(&user_id).await.unwrap().unwrap();
    assert!(user.is_email_verified());
    assert_eq!(user.status(), UserStatus::Active);
}

#[tokio::test]
async fn expired_token_is_rejected() {
    let user_id = UserId::generate();
    let raw = valid_raw_token();

    let user_repo = InMemoryUserRepository::new();
    user_repo.create(&make_user(user_id)).await.unwrap();

    let token_repo = InMemoryEmailVerificationTokenRepository::new();
    token_repo
        .create(&make_token(
            user_id,
            &raw,
            Utc::now() - Duration::seconds(1),
        ))
        .await
        .unwrap();

    let use_case = VerifyEmailUseCase::new(token_repo.clone(), user_repo.clone(), PlaintextHasher);
    let result = use_case.execute(raw).await;

    assert!(matches!(
        result,
        Err(VerifyEmailError::InvalidOrExpiredToken)
    ));
}

#[tokio::test]
async fn used_token_is_rejected() {
    let user_id = UserId::generate();
    let raw = valid_raw_token();

    let user_repo = InMemoryUserRepository::new();
    user_repo.create(&make_user(user_id)).await.unwrap();

    let token_repo = InMemoryEmailVerificationTokenRepository::new();
    let mut token = make_token(user_id, &raw, Utc::now() + Duration::hours(1));
    token.mark_used(Utc::now());
    token_repo.create(&token).await.unwrap();

    let use_case = VerifyEmailUseCase::new(token_repo.clone(), user_repo.clone(), PlaintextHasher);
    let result = use_case.execute(raw).await;

    assert!(matches!(
        result,
        Err(VerifyEmailError::InvalidOrExpiredToken)
    ));
}
