use std::error::Error;

use chrono::Utc;

use crate::{
    dto::{AuthResponse, RegisterRequest},
    ports::{IdGenerator, PasswordHasher, TokenService, UserRepository},
};

use domain::{Email, PasswordHash, UserBuilder, UserRole, UserStatus, Username};

type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

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
        let email = Email::new(request.email)?;
        let username = Username::new(request.username)?;

        if self.repository.email_exists(&email).await? {
            return Err("email already exists".into());
        }

        if self.repository.username_exists(&username).await? {
            return Err("username already exists".into());
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
