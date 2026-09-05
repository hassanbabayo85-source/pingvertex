use application::ports::{PasswordHasher, UserRepository};
use application::use_cases::{VerifyEmailOtpError, VerifyEmailOtpUseCase};
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
        Ok(password.to_owned())
    }

    async fn verify(
        &self,
        password: &str,
        hash: &str,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        Ok(password == hash)
    }
}

fn pending_user(id: UserId, email: &str) -> User {
    UserBuilder::new()
        .id(id)
        .username(Username::new("otpuser").unwrap())
        .email(Email::new(email).unwrap())
        .password_hash(PasswordHash::new("hash".to_string()).unwrap())
        .role(UserRole::User)
        .status(UserStatus::PendingVerification)
        .email_verified_at(None)
        .created_at(Utc::now())
        .updated_at(Utc::now())
        .build()
        .unwrap()
}

fn token(user_id: UserId, code: &str, expires_at: chrono::DateTime<Utc>) -> EmailVerificationToken {
    EmailVerificationToken::new(
        EmailVerificationTokenId::new(),
        user_id,
        "evt_test_prefix".to_string(),
        code.to_string(),
        expires_at,
        Utc::now(),
    )
}

async fn fixture(
    code: &str,
    expires_at: chrono::DateTime<Utc>,
) -> (
    UserId,
    InMemoryUserRepository,
    InMemoryEmailVerificationTokenRepository,
    VerifyEmailOtpUseCase<
        InMemoryEmailVerificationTokenRepository,
        InMemoryUserRepository,
        PlaintextHasher,
    >,
) {
    let user_id = UserId::generate();
    let users = InMemoryUserRepository::new();
    users
        .create(&pending_user(user_id, "otp@example.com"))
        .await
        .unwrap();
    let tokens = InMemoryEmailVerificationTokenRepository::new();
    tokens
        .create(&token(user_id, code, expires_at))
        .await
        .unwrap();
    let use_case = VerifyEmailOtpUseCase::new(tokens.clone(), users.clone(), PlaintextHasher);
    (user_id, users, tokens, use_case)
}

#[tokio::test]
async fn correct_otp_activates_user_and_consumes_token() {
    let (user_id, users, tokens, use_case) =
        fixture("123456", Utc::now() + Duration::minutes(10)).await;
    use_case
        .execute("otp@example.com".into(), "123456".into())
        .await
        .unwrap();

    let user = users.find_by_id(&user_id).await.unwrap().unwrap();
    assert!(user.is_email_verified());
    assert_eq!(user.status(), UserStatus::Active);
    assert!(
        tokens
            .find_latest_unused_by_user(user_id)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn wrong_otp_does_not_activate_user() {
    let (user_id, users, _, use_case) = fixture("123456", Utc::now() + Duration::minutes(10)).await;
    assert!(matches!(
        use_case
            .execute("otp@example.com".into(), "654321".into())
            .await,
        Err(VerifyEmailOtpError::InvalidCode)
    ));
    assert!(
        !users
            .find_by_id(&user_id)
            .await
            .unwrap()
            .unwrap()
            .is_email_verified()
    );
}

#[tokio::test]
async fn expired_otp_is_rejected() {
    let (user_id, users, _, use_case) = fixture("123456", Utc::now() - Duration::seconds(1)).await;
    assert!(matches!(
        use_case
            .execute("otp@example.com".into(), "123456".into())
            .await,
        Err(VerifyEmailOtpError::InvalidCode)
    ));
    assert!(
        !users
            .find_by_id(&user_id)
            .await
            .unwrap()
            .unwrap()
            .is_email_verified()
    );
}

#[tokio::test]
async fn malformed_otp_is_rejected() {
    let (_, _, _, use_case) = fixture("123456", Utc::now() + Duration::minutes(10)).await;
    for code in ["12345", "1234567", "12ab56", "１２３４５６"] {
        assert!(matches!(
            use_case
                .execute("otp@example.com".into(), code.into())
                .await,
            Err(VerifyEmailOtpError::InvalidCode)
        ));
    }
}

#[tokio::test]
async fn unknown_email_returns_generic_invalid_code() {
    let (_, _, _, use_case) = fixture("123456", Utc::now() + Duration::minutes(10)).await;
    assert!(matches!(
        use_case
            .execute("missing@example.com".into(), "123456".into())
            .await,
        Err(VerifyEmailOtpError::InvalidCode)
    ));
}

#[tokio::test]
async fn already_verified_account_does_not_get_success_oracle() {
    let (user_id, users, _, use_case) = fixture("123456", Utc::now() + Duration::minutes(10)).await;
    let mut user = users.find_by_id(&user_id).await.unwrap().unwrap();
    user.mark_email_verified(Utc::now());
    users.update(&user).await.unwrap();

    assert!(matches!(
        use_case
            .execute("otp@example.com".into(), "000000".into())
            .await,
        Err(VerifyEmailOtpError::InvalidCode)
    ));
}

#[tokio::test]
async fn otp_is_one_time_even_when_redeemed_twice() {
    let (_, _, _, use_case) = fixture("123456", Utc::now() + Duration::minutes(10)).await;
    use_case
        .execute("otp@example.com".into(), "123456".into())
        .await
        .unwrap();
    assert!(matches!(
        use_case
            .execute("otp@example.com".into(), "123456".into())
            .await,
        Err(VerifyEmailOtpError::InvalidCode)
    ));
}

#[tokio::test]
async fn concurrent_redemption_allows_only_one_success() {
    let (_, _, _, use_case) = fixture("123456", Utc::now() + Duration::minutes(10)).await;
    let a = use_case.execute("otp@example.com".into(), "123456".into());
    let b = use_case.execute("otp@example.com".into(), "123456".into());
    let (ra, rb) = tokio::join!(a, b);
    let successes = [ra, rb].into_iter().filter(Result::is_ok).count();
    assert_eq!(successes, 1);
}
