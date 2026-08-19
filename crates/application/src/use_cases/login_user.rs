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
///
/// SEC (fixed): the previous value here — "...$dummysaltdummysalt$dummyhash"
/// — was NOT a well-formed Argon2 PHC string (the salt/hash segments were
/// plain ASCII, not valid Base64 of the expected byte length). That made
/// `PasswordHash::new()` in `Argon2PasswordHasher::verify` fail to PARSE
/// this constant, returning `Err("invalid password hash: invalid Base64
/// encoding")` — which `?` then propagated all the way out of `execute()`
/// as a distinct error BEFORE any Argon2 computation ever ran. In practice
/// this meant a login attempt for a non-existent email returned quickly
/// with a different, more specific error than a login attempt for a real
/// account with the wrong password (which does run the full Argon2
/// verification and returns the generic "invalid email or password").
/// That is exactly the user-enumeration oracle this constant exists to
/// prevent — worse than a timing side-channel, since it was distinguishable
/// by response content/status alone, with no timing analysis required.
///
/// This value below IS a real, validly-formatted Argon2id hash (generated
/// with the same parameters used elsewhere in this file: m=19456, t=2,
/// p=1) of an unguessable, never-typed string, so `PasswordHash::new()`
/// parses it successfully and the full Argon2 computation actually runs —
/// restoring both the constant-time behavior and the constant response
/// shape this mitigation is meant to provide.
const DUMMY_PASSWORD_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$wYo4u5354twxR/K/HWCW7g$3Zmj6/MSGlMa/2QpgFadVgV73QxWzpuOBzsGK9Fpyug";

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

        // SEC (defense-in-depth): match on the Result explicitly rather than
        // propagating a verify() error with `?`. A hasher error (malformed
        // stored hash, unexpected parameters, etc.) must fail CLOSED as
        // "invalid email or password" — the same generic response as a
        // wrong password — never as a distinct error type/message/status.
        // Anything else re-opens the exact user-enumeration oracle the
        // DUMMY_PASSWORD_HASH mechanism above exists to close.
        let valid = self
            .password_hasher
            .verify(&request.password, &password_hash)
            .await
            .unwrap_or(false);

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
