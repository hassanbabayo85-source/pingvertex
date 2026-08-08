use std::error::Error;

use crate::{
    dto::{AuthResponse, LoginRequest},
    ports::{PasswordHasher, TokenService, UserRepository},
};

use domain::Email;

type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub struct LoginUserUseCase<R, H, T> {
    repository: R,
    password_hasher: H,
    token_service: T,
}

impl<R, H, T> LoginUserUseCase<R, H, T>
where
    R: UserRepository,
    H: PasswordHasher,
    T: TokenService,
{
    pub fn new(repository: R, password_hasher: H, token_service: T) -> Self {
        Self {
            repository,
            password_hasher,
            token_service,
        }
    }

    pub async fn execute(&self, request: LoginRequest) -> UseCaseResult<AuthResponse> {
        let email = Email::new(request.email)?;

        let user = self
            .repository
            .find_by_email(&email)
            .await?
            .ok_or("invalid email or password")?;

        let valid = self
            .password_hasher
            .verify(&request.password, user.password_hash().as_str())
            .await?;

        if !valid {
            return Err("invalid email or password".into());
        }

        let token = self.token_service.generate(user.id()).await?;

        Ok(AuthResponse {
            access_token: token,
        })
    }
}
