use std::error::Error;

use chrono::Utc;

use crate::{
    dto::RegisterRequest,
    ports::{IdGenerator, PasswordHasher, RegistrationTransaction, UserRepository},
};

use domain::{
    Email, PasswordHash, Tenant, TenantMembership, UserBuilder, UserRole, UserStatus, Username,
    shared::{ids::TenantId, types::Role},
};

type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

const MIN_PASSWORD_LENGTH: usize = 12;
const MAX_PASSWORD_LENGTH: usize = 128;

fn validate_password(password: &str) -> Result<(), &'static str> {
    if password.chars().count() > MAX_PASSWORD_LENGTH {
        return Err("password must not exceed 128 characters");
    }
    if password.trim().is_empty() {
        return Err("password must not be empty");
    }
    if password.chars().count() < MIN_PASSWORD_LENGTH {
        return Err("password must be at least 12 characters long");
    }
    if !password.chars().any(|c| c.is_ascii_uppercase()) {
        return Err("password must contain at least one uppercase letter");
    }
    if !password.chars().any(|c| c.is_ascii_lowercase()) {
        return Err("password must contain at least one lowercase letter");
    }
    if !password.chars().any(|c| c.is_ascii_digit()) {
        return Err("password must contain at least one digit");
    }
    if !password.chars().any(|c| !c.is_ascii_alphanumeric()) {
        return Err("password must contain at least one special character");
    }
    Ok(())
}

pub struct RegisterUserUseCase<R, G, H, RT> {
    repository: R,
    id_generator: G,
    password_hasher: H,
    registration_transaction: RT,
}

impl<R, G, H, RT> RegisterUserUseCase<R, G, H, RT>
where
    R: UserRepository,
    G: IdGenerator,
    H: PasswordHasher,
    RT: RegistrationTransaction,
{
    pub fn new(
        repository: R,
        id_generator: G,
        password_hasher: H,
        registration_transaction: RT,
    ) -> Self {
        Self {
            repository,
            id_generator,
            password_hasher,
            registration_transaction,
        }
    }

    pub async fn execute(&self, request: RegisterRequest) -> UseCaseResult<()> {
        validate_password(&request.password)?;

        let raw_username_for_tenant_name = request.username.clone();

        let email = Email::new(request.email)?;
        let username = Username::new(request.username)?;

        let email_exists = self.repository.email_exists(&email).await?;
        let username_exists = self.repository.username_exists(&username).await?;

        if email_exists || username_exists {
            return Err("registration failed: email or username already in use".into());
        }

        let hashed = self.password_hasher.hash(&request.password).await?;

        let now = Utc::now();
        let id = self.id_generator.next_user_id().await;

        let user = UserBuilder::new()
            .id(id)
            .username(username)
            .email(email)
            .password_hash(PasswordHash::new(hashed)?)
            .role(UserRole::User)
            .status(UserStatus::PendingVerification)
            .email_verified_at(None)
            .created_at(now)
            .updated_at(now)
            .build()?;

        let tenant = Tenant::new(
            TenantId::new(),
            format!("{}'s workspace", raw_username_for_tenant_name),
            user.id(),
        )?;

        let membership = TenantMembership {
            tenant_id: tenant.id,
            user_id: user.id(),
            role: Role::Owner,
            invited_at: now,
            joined_at: Some(now),
        };

        // Ajiye User + Tenant + TenantMembership a matsayin AIKI GUDA ƊAYA
        // (atomic). Idan wani sashi ya gaza, ba za a ajiye komai ba — babu
        // yiwuwar samun "orphaned user" mara tenant kamar da.
        self.registration_transaction
            .register_user_with_tenant(&user, &tenant, &membership)
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::validate_password;

    #[test]
    fn rejects_empty_password() {
        assert!(validate_password("").is_err());
    }
    #[test]
    fn rejects_whitespace_only_password() {
        assert!(validate_password("            ").is_err());
    }
    #[test]
    fn rejects_password_shorter_than_12_characters() {
        assert!(validate_password("Short1!").is_err());
    }
    #[test]
    fn rejects_password_without_uppercase() {
        assert!(validate_password("securepassword123!").is_err());
    }
    #[test]
    fn rejects_password_without_lowercase() {
        assert!(validate_password("SECUREPASSWORD123!").is_err());
    }
    #[test]
    fn rejects_password_without_digit() {
        assert!(validate_password("SecurePassword!").is_err());
    }
    #[test]
    fn rejects_password_without_special_character() {
        assert!(validate_password("SecurePassword123").is_err());
    }
    #[test]
    fn accepts_strong_password() {
        assert!(validate_password("SecurePassword123!").is_ok());
    }
    #[test]
    fn rejects_password_longer_than_128_characters() {
        let long_password = "A1a!".repeat(40);
        assert!(validate_password(&long_password).is_err());
    }
    #[test]
    fn accepts_password_at_128_character_boundary() {
        let boundary_password = format!("{}A1a!", "a".repeat(124));
        assert_eq!(boundary_password.chars().count(), 128);
        assert!(validate_password(&boundary_password).is_ok());
    }
}
