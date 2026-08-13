use std::error::Error;

use chrono::Utc;

use crate::{
    dto::{AuthResponse, RegisterRequest},
    ports::{IdGenerator, PasswordHasher, TokenService, UserRepository},
};

use domain::{Email, PasswordHash, UserBuilder, UserRole, UserStatus, Username};

type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

const MIN_PASSWORD_LENGTH: usize = 12;

fn validate_password(password: &str) -> Result<(), &'static str> {
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

pub struct RegisterUserUseCase<R, G, H, T> {
    repository: R,
    id_generator: G,
    password_hasher: H,
    token_service: T,
}

impl<R, G, H, T> RegisterUserUseCase<R, G, H, T>
where
    R: UserRepository,
    G: IdGenerator,
    H: PasswordHasher,
    T: TokenService,
{
    pub fn new(repository: R, id_generator: G, password_hasher: H, token_service: T) -> Self {
        Self {
            repository,
            id_generator,
            password_hasher,
            token_service,
        }
    }

    pub async fn execute(&self, request: RegisterRequest) -> UseCaseResult<AuthResponse> {
        validate_password(&request.password)?;

        let email = Email::new(request.email)?;
        let username = Username::new(request.username)?;

        // Duk wanda ya riga ya wanzu (email ko username) yana mayar da
        // SAME saƙon kuskure gama gari — ba a bambanta tsakanin biyun ba,
        // domin wannan yana toshe hanyar da mai kai hari zai iya gano ko
        // wane email ko username ne aka riga aka yi rejista dashi
        // (enumeration vulnerability).
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

        self.repository.create(&user).await?;

        let token = self.token_service.generate(user.id()).await?;

        Ok(AuthResponse {
            access_token: token,
        })
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
}
