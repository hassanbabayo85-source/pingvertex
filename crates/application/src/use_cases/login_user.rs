use std::error::Error;

use crate::{
    dto::{AuthResponse, LoginRequest},
    ports::{PasswordHasher, TokenService, UserRepository},
};

use domain::{Email, UserStatus};

type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Iyaka mafi girma na tsawon password a login flow. Ana duba wannan
/// KAFIN a kira `password_hasher.verify()`, domin `verify()` yana kiran
/// Argon2id wanda ke da tsada sosai a CPU — babu wannan iyaka, mai kai
/// hari zai iya tura babban password (misali 10MB) akai-akai zuwa login
/// endpoint, ko da ba shi da account, ya cinye CPU din server (DoS).
const MAX_PASSWORD_LENGTH: usize = 128;

/// Argon2id hash of a value nobody will ever type, used so that a login
/// attempt for a non-existent email still pays the cost of a full hash
/// verification. This keeps response time roughly constant whether or not
/// the account exists, closing a timing side-channel that could otherwise
/// be used to enumerate registered emails.
const DUMMY_PASSWORD_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$dummysaltdummysalt$dummyhash";

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

        // Duba tsawon password KAFIN a yi query zuwa database ko kira
        // verify(). Ana mayar da SAME saƙon kuskure na "invalid email or
        // password" (ba wani saƙo daban ba) domin kar a fallasa cewa
        // akwai irin wannan iyakar tsawo — hakan zai iya taimakawa mai
        // kai hari ya bambanta amsoshi (oracle) tsakanin kuskuren
        // password/email da kuskuren tsawon password.
        if request.password.chars().count() > MAX_PASSWORD_LENGTH {
            return Err("invalid email or password".into());
        }

        let user = self.repository.find_by_email(&email).await?;

        // KOYAUSHE a yi verify, ko da user bai wanzu ba, domin lokacin amsa
        // ya kasance daidai a kowane hali (constant-time behavior) —
        // wannan yana toshe timing side-channel da zai iya bawa mai kai
        // hari damar gane wanne email ke wanzu a database.
        let password_hash = user
            .as_ref()
            .map(|u| u.password_hash().as_str().to_string())
            .unwrap_or_else(|| DUMMY_PASSWORD_HASH.to_string());

        let valid = self
            .password_hasher
            .verify(&request.password, &password_hash)
            .await?;

        if !valid || user.is_none() {
            return Err("invalid email or password".into());
        }

        let user = user.unwrap();

        // A yanzu ana kyale login ga duk wanda ba a dakatar da shi ba ko
        // ba a goge shi ba. `PendingVerification` ana ɗaukarsa daidai da
        // `Active` saboda babu wani email-verification flow tukuna da zai
        // hana shi; za a sake duba wannan idan an gina wannan flow.
        match user.status() {
            UserStatus::Active | UserStatus::PendingVerification => {}
            UserStatus::Suspended => {
                return Err("account suspended".into());
            }
            UserStatus::Deleted => {
                return Err("invalid email or password".into());
            }
        }

        let token = self.token_service.generate(user.id()).await?;

        Ok(AuthResponse {
            access_token: token,
        })
    }
}
