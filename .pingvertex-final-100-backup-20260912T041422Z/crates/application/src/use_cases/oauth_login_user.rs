use std::error::Error;

use chrono::Utc;
use uuid::Uuid;

use crate::{
    dto::AuthResponse,
    ports::{
        IdGenerator, OAuthIdentityRepository, PasswordHasher, RegistrationTransaction,
        TokenService, UserRepository, VerifiedIdentity,
    },
};

use domain::{
    Email, PasswordHash, Tenant, TenantMembership, UserBuilder, UserRole, UserStatus, Username,
    shared::{ids::TenantId, types::Role},
};

type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// How many times to retry appending a random suffix before giving up
/// on deriving a unique username from an email's local part.
const MAX_USERNAME_ATTEMPTS: u8 = 6;

/// Signs a user in via a verified Google/Apple identity, provisioning a
/// brand-new account (with its own tenant, exactly like
/// `RegisterUserUseCase`) the first time a given email is seen.
///
/// Deliberately reuses `RegistrationTransaction` rather than a bare
/// `UserRepository::create` for the new-account path, so an
/// OAuth-created user gets the same atomic User+Tenant+TenantMembership
/// guarantee a password-registered user gets — there is no second,
/// weaker account-creation path hiding in this use case.
///
/// Existing linked identities are matched by `(provider, sub)`. Verified email is
/// consulted only the first time a provider identity is linked to an existing
/// password account, after which the immutable provider subject is authoritative.
pub struct OAuthLoginUseCase<R, G, H, T, RT, O> {
    repository: R,
    id_generator: G,
    password_hasher: H,
    token_service: T,
    registration_transaction: RT,
    oauth_identity_repository: O,
    provider: &'static str,
}

impl<R, G, H, T, RT, O> OAuthLoginUseCase<R, G, H, T, RT, O>
where
    R: UserRepository,
    G: IdGenerator,
    H: PasswordHasher,
    T: TokenService,
    RT: RegistrationTransaction,
    O: OAuthIdentityRepository,
{
    pub fn new(
        repository: R,
        id_generator: G,
        password_hasher: H,
        token_service: T,
        registration_transaction: RT,
        oauth_identity_repository: O,
        provider: &'static str,
    ) -> Self {
        Self {
            repository,
            id_generator,
            password_hasher,
            token_service,
            registration_transaction,
            oauth_identity_repository,
            provider,
        }
    }

    pub async fn execute(&self, identity: VerifiedIdentity) -> UseCaseResult<AuthResponse> {
        if !identity.email_verified {
            // Should not happen in practice (see struct doc comment),
            // but fail closed rather than silently trusting an
            // unverified address if a provider ever reports this.
            return Err("identity provider did not confirm email ownership".into());
        }

        let email = Email::new(identity.email)?;

        // Prefer the provider's immutable subject once it has been linked.
        // Email is used only for first-time linking/provisioning.
        if let Some(user_id) = self
            .oauth_identity_repository
            .find_user_id(self.provider, &identity.subject)
            .await?
        {
            let mut user = self
                .repository
                .find_by_id(&user_id)
                .await?
                .ok_or("linked OAuth account no longer exists")?;
            match user.status() {
                UserStatus::Suspended | UserStatus::Deleted => {
                    return Err("account is not available".into());
                }
                UserStatus::PendingVerification => {
                    user.mark_email_verified(Utc::now());
                    self.repository.update(&user).await?;
                }
                UserStatus::Active => {}
            }
            let token = self.token_service.generate(user.id()).await?;
            return Ok(AuthResponse {
                access_token: token,
            });
        }

        if let Some(mut user) = self.repository.find_by_email(&email).await? {
            match user.status() {
                UserStatus::Suspended | UserStatus::Deleted => {
                    return Err("account is not available".into());
                }
                UserStatus::PendingVerification => {
                    user.mark_email_verified(Utc::now());
                    self.repository.update(&user).await?;
                }
                UserStatus::Active => {}
            }
            self.oauth_identity_repository
                .link(self.provider, &identity.subject, user.id())
                .await?;
            let token = self.token_service.generate(user.id()).await?;
            return Ok(AuthResponse {
                access_token: token,
            });
        }

        // No account with this email yet — provision one. The account
        // is created already-verified: the identity provider proved
        // ownership of the email more strongly than our own
        // click-a-link verification flow does.
        let username = self.unique_username_from_email(&email).await?;

        // Users provisioned via OAuth never set a local password. We
        // still populate `password_hash` (the column is NOT NULL, and
        // changing that would mean a migration plus touching every
        // `UserRepository` implementation and `User`/`UserBuilder` —
        // out of scope for this feature) with a hash of a random,
        // never-returned secret, so `LoginUserUseCase`'s constant-time
        // Argon2 comparison simply fails for any password guess rather
        // than needing a special-cased "this account has no password"
        // branch.
        let unusable_secret = self.id_generator.next_webhook_signing_secret().await;
        let hashed = self.password_hasher.hash(&unusable_secret).await?;

        let now = Utc::now();
        let id = self.id_generator.next_user_id().await;

        let user = UserBuilder::new()
            .id(id)
            .username(username.clone())
            .email(email.clone())
            .password_hash(PasswordHash::new(hashed)?)
            .role(UserRole::User)
            .status(UserStatus::Active)
            .email_verified_at(Some(now))
            .created_at(now)
            .updated_at(now)
            .build()?;

        let tenant = Tenant::new(
            TenantId::new(),
            format!("{}'s workspace", username.as_str()),
            user.id(),
        )?;

        let membership = TenantMembership {
            tenant_id: tenant.id,
            user_id: user.id(),
            role: Role::Owner,
            invited_at: now,
            joined_at: Some(now),
        };

        self.registration_transaction
            .register_user_with_tenant(&user, &tenant, &membership)
            .await?;

        self.oauth_identity_repository
            .link(self.provider, &identity.subject, user.id())
            .await?;

        let token = self.token_service.generate(user.id()).await?;

        Ok(AuthResponse {
            access_token: token,
        })
    }

    /// Derives a `Username` from the email's local part (stripping
    /// characters `Username` doesn't allow), falling back to a short
    /// random suffix on collision. Mirrors the constraints in
    /// `domain::Username::new` (3–32 chars, alnum/`_`/`-` only).
    async fn unique_username_from_email(&self, email: &Email) -> UseCaseResult<Username> {
        let local_part = email.as_str().split('@').next().unwrap_or("user");

        let cleaned: String = local_part
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .take(24)
            .collect();

        let base = if cleaned.chars().count() < 3 {
            format!("user{cleaned}")
        } else {
            cleaned
        };

        for attempt in 0..MAX_USERNAME_ATTEMPTS {
            let candidate = if attempt == 0 {
                base.clone()
            } else {
                let suffix: String = Uuid::new_v4()
                    .simple()
                    .to_string()
                    .chars()
                    .take(6)
                    .collect();
                format!("{base}{suffix}")
            };

            if let Ok(username) = Username::new(candidate)
                && !self.repository.username_exists(&username).await?
            {
                return Ok(username);
            }
        }

        Err("could not derive a unique username for this account".into())
    }
}
