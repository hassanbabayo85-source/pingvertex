use crate::errors::DomainError;

/// Maximum length, in characters, allowed for either the username or the
/// password field of `ProxyCredentials`.
const MAX_PROXY_CREDENTIAL_FIELD_LENGTH: usize = 256;

/// Username/password credentials for authenticating against a proxy.
///
/// SECURITY: this type deliberately does NOT derive `Debug`. It implements
/// `Debug` by hand below so that `password` always prints as
/// `"[REDACTED]"` — this means an accidental `tracing::debug!("{:?}", creds)`
/// (or any other `{:?}`/log call) elsewhere in the codebase can never leak
/// the password into logs.
#[derive(PartialEq)]
pub struct ProxyCredentials {
    username: String,
    password: String,
}

impl ProxyCredentials {
    /// Creates validated proxy credentials.
    pub fn new(
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let username = username.into();
        let password = password.into();

        if username.is_empty() && password.is_empty() {
            return Err(DomainError::EmptyProxyCredentials);
        }

        if username.len() > MAX_PROXY_CREDENTIAL_FIELD_LENGTH
            || password.len() > MAX_PROXY_CREDENTIAL_FIELD_LENGTH
        {
            return Err(DomainError::ProxyCredentialFieldTooLong);
        }

        Ok(Self { username, password })
    }

    /// Returns the username.
    pub fn username(&self) -> &str {
        &self.username
    }

    /// Returns the password.
    pub fn password(&self) -> &str {
        &self.password
    }
}

impl std::fmt::Debug for ProxyCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProxyCredentials")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_username_and_password() {
        let creds = ProxyCredentials::new("user", "pass").unwrap();
        assert_eq!(creds.username(), "user");
        assert_eq!(creds.password(), "pass");
    }

    #[test]
    fn accepts_username_only() {
        assert!(ProxyCredentials::new("user", "").is_ok());
    }

    #[test]
    fn accepts_password_only() {
        assert!(ProxyCredentials::new("", "pass").is_ok());
    }

    #[test]
    fn rejects_both_empty() {
        assert_eq!(
            ProxyCredentials::new("", ""),
            Err(DomainError::EmptyProxyCredentials)
        );
    }

    #[test]
    fn rejects_username_longer_than_256_characters() {
        let long_username = "a".repeat(257);
        assert_eq!(
            ProxyCredentials::new(long_username, "pass"),
            Err(DomainError::ProxyCredentialFieldTooLong)
        );
    }

    #[test]
    fn rejects_password_longer_than_256_characters() {
        let long_password = "a".repeat(257);
        assert_eq!(
            ProxyCredentials::new("user", long_password),
            Err(DomainError::ProxyCredentialFieldTooLong)
        );
    }

    #[test]
    fn debug_output_redacts_password() {
        let creds = ProxyCredentials::new("user", "super-secret").unwrap();
        let debug_output = format!("{creds:?}");

        assert!(debug_output.contains("user"));
        assert!(debug_output.contains("[REDACTED]"));
        assert!(!debug_output.contains("super-secret"));
    }
}
