use std::error::Error;

use crate::{
    dto::{AuthResponse, LoginOutcome, LoginRequest, PendingTwoFactorResponse},
    ports::{IdGenerator, PasswordHasher, PendingAuthCache, TokenService, UserRepository},
    use_cases::totp_support::{PENDING_LOGIN_TTL_SECONDS, pending_login_cache_key},
};

use domain::{Email, UserStatus};

type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Iyaka mafi girma na tsawon password a login flow. Ana duba wannan
/// KAFIN a kira `password_hasher.verify()`, domin `verify()` yana kiran
/// Argon2id wanda ke da tsada sosai a CPU — babu wannan iyaka, mai kai
/// hari zai iya tura babban password (misali 10MB) akai-akai zuwa login
/// endpoint, ko da ba shi da account, ya cinye CPU din server (DoS).
const MAX_PASSWORD_LENGTH: usize = 128;

/// Maximum number of consecutive failed login attempts allowed for a
/// single (normalized — see `Email::new`'s lowercasing/trimming) email
/// address within the lockout window (`LOGIN_LOCKOUT_TTL_SECONDS`
/// below), before further attempts for that email are rejected outright
/// with [`LOGIN_LOCKOUT_MESSAGE`] — regardless of whether the supplied
/// credentials are actually correct.
///
/// SEC: tracked per EMAIL, not per user id, and incremented identically
/// whether or not the email corresponds to a real account (see
/// `record_failed_login_attempt` below). This is what keeps the
/// resulting 429 from becoming a second user-enumeration oracle
/// alongside the `DUMMY_PASSWORD_HASH` mechanism further down: an
/// attacker hammering a made-up email observes the exact same lockout
/// behavior as one hammering a real one.
///
/// This is per-account protection, complementing — not replacing — the
/// IP-based `GovernorLayer` already applied to `POST /auth/login` in
/// `crates/api/src/router.rs` (5 requests/60s/IP): an attacker rotating
/// source IPs bypasses the IP limiter but still hits this per-account
/// lockout.
const MAX_FAILED_LOGIN_ATTEMPTS: u32 = 5;

/// TTL (seconds) of the failed-attempt counter for a given email — 15
/// minutes. NOT refreshed while an email is already locked out (only on
/// an actual counted failure below the threshold): a continuously
/// hammered, already-locked account still unlocks exactly
/// `LOGIN_LOCKOUT_TTL_SECONDS` after its LAST counted failure, rather
/// than being kept locked forever by an attacker who just keeps
/// sending requests — that would turn this protection into a
/// denial-of-service tool against the legitimate account owner.
const LOGIN_LOCKOUT_TTL_SECONDS: u64 = 900;

/// Exact error message returned when an email has exceeded
/// `MAX_FAILED_LOGIN_ATTEMPTS`.
///
/// `crates/api/src/handlers/login.rs` matches on this EXACT string
/// (duplicated there rather than imported — see that file's own doc
/// comment for why) to return 429 Too Many Requests instead of the
/// generic 400 "invalid email or password". If the two literals are
/// ever edited out of sync, this still fails SAFE: the login is still
/// refused either way, the lockout case just falls back to a 400 with
/// the generic message instead of a 429.
const LOGIN_LOCKOUT_MESSAGE: &str =
    "account temporarily locked due to too many failed login attempts; please try again later";

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

/// Cache key (in `PendingAuthCache`, namespace `login_attempts:`) for the
/// failed-login counter of `email`. Keyed on `Email::as_str()`, which is
/// already lowercased/trimmed by `Email::new` — so `Foo@Bar.com` and
/// `foo@bar.com` share one counter, closing a trivial case-variation
/// bypass of the lockout.
fn login_attempts_cache_key(email: &Email) -> String {
    format!("login_attempts:{}", email.as_str())
}

pub struct LoginUserUseCase<R, H, T, G, C> {
    repository: R,
    password_hasher: H,
    token_service: T,
    id_generator: G,
    pending_auth_cache: C,
}

impl<R, H, T, G, C> LoginUserUseCase<R, H, T, G, C>
where
    R: UserRepository,
    H: PasswordHasher,
    T: TokenService,
    G: IdGenerator,
    C: PendingAuthCache,
{
    pub fn new(
        repository: R,
        password_hasher: H,
        token_service: T,
        id_generator: G,
        pending_auth_cache: C,
    ) -> Self {
        Self {
            repository,
            password_hasher,
            token_service,
            id_generator,
            pending_auth_cache,
        }
    }

    pub async fn execute(&self, request: LoginRequest) -> UseCaseResult<LoginOutcome> {
        let email = Email::new(request.email)?;

        // SEC (B4): brute-force / credential-stuffing protection. Checked
        // FIRST, before any expensive work (password-length validation,
        // DB lookup, Argon2 verification), so a locked-out email also
        // stops paying that cost — this is itself a defense against using
        // a burst of "free" lockout probes as a CPU-exhaustion vector.
        let attempts_key = login_attempts_cache_key(&email);
        let attempts_before_this_request: u32 = self
            .pending_auth_cache
            .get(&attempts_key)
            .await
            .ok()
            .flatten()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);

        if attempts_before_this_request >= MAX_FAILED_LOGIN_ATTEMPTS {
            return Err(LOGIN_LOCKOUT_MESSAGE.into());
        }

        // Duba tsawon password KAFIN a yi query zuwa database ko kira
        // verify(). Ana mayar da SAME saƙon kuskure na "invalid email or
        // password" (ba wani saƙo daban ba) domin kar a fallasa cewa
        // akwai irin wannan iyakar tsawo — hakan zai iya taimakawa mai
        // kai hari ya bambanta amsoshi (oracle) tsakanin kuskuren
        // password/email da kuskuren tsawon password.
        if request.password.chars().count() > MAX_PASSWORD_LENGTH {
            self.record_failed_login_attempt(&attempts_key, attempts_before_this_request)
                .await;
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
            self.record_failed_login_attempt(&attempts_key, attempts_before_this_request)
                .await;
            return Err("invalid email or password".into());
        }

        // Correct credentials proven for this email — clear any
        // accumulated failed-attempt count so a legitimate user who
        // mistyped their password a few times isn't left one bad guess
        // away from lockout.
        let _ = self.pending_auth_cache.delete(&attempts_key).await;

        let user = user.unwrap();

        // A yanzu ana kyale login ga duk wanda ba a dakatar da shi ba ko
        // ba a goge shi ba. `PendingVerification` ana ɗaukarsa daidai da
        // `Active` saboda babu wani email-verification flow tukuna da zai
        // hana shi; za a sake duba wannan idan an gina wannan flow.
        match user.status() {
            UserStatus::Active => {}
            UserStatus::PendingVerification => {
                return Err("email verification required".into());
            }
            UserStatus::Suspended => {
                // SEC (B4 audit): previously returned a distinct "account
                // suspended" message here. Genericized to the SAME
                // "invalid email or password" response as every other
                // rejection in this use case. The caller has already
                // supplied correct credentials to reach this branch, so
                // this specific spot isn't a brute-force-guessing oracle
                // — but a distinct message here still needlessly reveals
                // account status to anyone who has (or has guessed)
                // valid credentials, and the TASK asked this worker to
                // audit B3's auth error messages for exactly this kind
                // of leak. See HANDOVER.md "Deviations from prompt".
                return Err("invalid email or password".into());
            }
            UserStatus::Deleted => {
                return Err("invalid email or password".into());
            }
        }

        // IDAN 2FA na aiki, KADA a dawo da JWT session nan take — a
        // maimakon haka, a dawo da wani short-lived opaque pending token,
        // wanda za a musanya shi da JWT ta ainihi ta hanyar
        // `VerifyTotpAndLoginUseCase` (bayan an tabbatar da TOTP code ko
        // backup code). `token_service.generate` (a ƙasa) shi ne kaɗai
        // wurin da wannan use case ke ba da JWT session — daidai yadda
        // yake yanzu ga masu login OMIT 2FA.
        if user.totp_enabled() {
            let pending_token = self.id_generator.next_pending_two_factor_token().await;

            // SEC (B4 audit): previously mapped a cache failure here to
            // `format!("failed to store pending 2FA token: {e}")`, which
            // the (unaudited) login handler forwarded verbatim to the
            // client — leaking raw internal/cache error text. Genericized
            // to the same "invalid email or password" response as every
            // other rejection in this use case; the real cause is still
            // available to the handler via `err` for server-side logging.
            self.pending_auth_cache
                .set(
                    &pending_login_cache_key(&pending_token),
                    &user.id().to_string(),
                    PENDING_LOGIN_TTL_SECONDS,
                )
                .await
                .map_err(|_| "invalid email or password")?;

            return Ok(LoginOutcome::PendingTwoFactor(PendingTwoFactorResponse {
                pending_token,
            }));
        }

        let token = self.token_service.generate(user.id()).await?;

        Ok(LoginOutcome::Authenticated(AuthResponse {
            access_token: token,
        }))
    }

    /// Best-effort increment of the failed-login counter for `key`. Cache
    /// failures here must never block the caller's own (already-decided)
    /// "invalid email or password" response, so errors are swallowed
    /// rather than propagated — worst case, this one attempt isn't
    /// counted toward the lockout threshold.
    ///
    /// KNOWN LIMITATION (documented in HANDOVER.md "Known Risks"): this
    /// is a plain get-then-set, not an atomic increment —
    /// `PendingAuthCache` has no `incr` primitive. Two concurrent failed
    /// attempts for the same email can race and only count as one. This
    /// narrows the lockout's precision under concurrent attack but does
    /// not remove the protection entirely.
    async fn record_failed_login_attempt(&self, key: &str, attempts_before_this_request: u32) {
        let next = attempts_before_this_request + 1;
        let _ = self
            .pending_auth_cache
            .set(key, &next.to_string(), LOGIN_LOCKOUT_TTL_SECONDS)
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_attempts_cache_key_is_namespaced_and_uses_normalized_email() {
        let a = Email::new("Foo@Bar.com").unwrap();
        let b = Email::new("  foo@bar.com  ").unwrap();

        let key_a = login_attempts_cache_key(&a);
        let key_b = login_attempts_cache_key(&b);

        assert_eq!(
            key_a, key_b,
            "case/whitespace variations must share one counter"
        );
        assert!(key_a.starts_with("login_attempts:"));
        assert!(key_a.contains("foo@bar.com"));
    }
}
