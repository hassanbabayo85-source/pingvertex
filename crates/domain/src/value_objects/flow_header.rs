//! Sensitivity-aware HTTP header value object for API flow steps.
//!
//! A `FlowHeader` is a name/value pair plus an explicit `secret`
//! classification. The classification drives three separate behaviors:
//!
//! - `Debug` output redacts the value when `secret == true`.
//! - API response DTOs render the value as `***REDACTED***` when
//!   `secret == true`.
//! - Repositories encrypt the value at rest when `secret == true`.
//!
//! Non-secret header values behave exactly like a plain `(String, String)`
//! tuple — stored, returned, and logged verbatim.
//!
//! The `secret` flag is authoritative. The name-based classifier
//! [`FlowHeader::is_sensitive_name`] is only a *default* used at the
//! request boundary to auto-classify headers whose names are known to
//! carry credentials when the client does not say otherwise. Nothing
//! downstream re-checks the name list for correctness — the flag is what
//! the redaction/encryption paths actually key off of.

use crate::errors::DomainError;

/// Hard cap on the number of headers a single flow step may carry.
pub const MAX_HEADERS_PER_STEP: usize = 20;

/// Hard cap on the byte length of a header name.
///
/// Header names are constrained by RFC 7230 token grammar; 256 bytes is
/// far beyond any legitimate header name (real names top out around 30
/// bytes) and is consistent with the length caps used elsewhere in this
/// crate.
pub const MAX_HEADER_NAME_LENGTH: usize = 256;

/// Hard cap on the byte length of a header value. Kept in lockstep with
/// `api_flow_step::MAX_HEADER_VALUE_LENGTH`.
pub const MAX_HEADER_VALUE_LENGTH: usize = 2048;

/// Value object: a single HTTP header on an `ApiFlowStep`.
///
/// Fields are private so that every `FlowHeader` has passed through
/// [`FlowHeader::new`]'s validation (non-empty name, name/value length
/// caps). Serialization is deliberately NOT derived: the on-disk shape
/// is owned by the repository that persists it (see
/// `PostgresApiFlowRepository`) and the on-the-wire shape is owned by the
/// HTTP DTO layer. This type is the in-memory domain representation.
#[derive(Clone, PartialEq, Eq)]
pub struct FlowHeader {
    name: String,
    value: String,
    secret: bool,
}

impl FlowHeader {
    /// Builds a header, validating name/value lengths and applying the
    /// caller's explicit `secret` classification.
    pub fn new(
        name: impl Into<String>,
        value: impl Into<String>,
        secret: bool,
    ) -> Result<Self, DomainError> {
        let name = name.into();
        let value = value.into();

        if name.is_empty() {
            return Err(DomainError::EmptyHeaderName);
        }
        if name.len() > MAX_HEADER_NAME_LENGTH {
            return Err(DomainError::HeaderNameTooLong);
        }
        if value.len() > MAX_HEADER_VALUE_LENGTH {
            return Err(DomainError::HeaderValueTooLong);
        }

        Ok(Self {
            name,
            value,
            secret,
        })
    }

    /// Convenience constructor for a non-secret header.
    pub fn plain(name: impl Into<String>, value: impl Into<String>) -> Result<Self, DomainError> {
        Self::new(name, value, false)
    }

    /// Convenience constructor for a secret header.
    pub fn secret(name: impl Into<String>, value: impl Into<String>) -> Result<Self, DomainError> {
        Self::new(name, value, true)
    }

    /// The header's name, verbatim (not lowercased).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The header's plaintext value.
    ///
    /// Always returns the plaintext the header was constructed with. It
    /// is the *callers* (`Debug`, response DTOs, persistence) that are
    /// responsible for redacting or encrypting before exposing it.
    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn is_secret(&self) -> bool {
        self.secret
    }

    /// Case-insensitive check whether `name` is a header whose value
    /// conventionally carries a credential (Authorization, Cookie,
    /// X-API-Key, etc.) and should therefore *default* to `secret = true`
    /// when the client does not explicitly say otherwise.
    ///
    /// This is a convenience default, NOT the enforcement mechanism:
    /// - A client may explicitly set `secret: false` on a name in this
    ///   list (uncommon, but supported).
    /// - A client may explicitly set `secret: true` on any name.
    /// - All downstream behavior (Debug redaction, response redaction,
    ///   at-rest encryption) is driven by the `secret` field on the
    ///   constructed `FlowHeader`, never by re-checking this list.
    pub fn is_sensitive_name(name: &str) -> bool {
        let lowered = name.to_ascii_lowercase();
        matches!(
            lowered.as_str(),
            "authorization"
                | "proxy-authorization"
                | "authentication"
                | "cookie"
                | "set-cookie"
                | "x-api-key"
                | "x-api-token"
                | "x-api-secret"
                | "x-auth-token"
                | "x-access-token"
                | "x-csrf-token"
                | "x-xsrf-token"
                | "api-key"
                | "proxy-authenticate"
                | "www-authenticate"
                | "x-amz-security-token"
                | "x-amz-credential"
                | "x-amz-signature"
        )
    }
}

/// Manual `Debug` that redacts `value` when `secret == true`.
///
/// Deriving `Debug` here would defeat the whole point of the value
/// object: any `tracing::info!(?flow)` or `format!("{flow:?}")` would
/// print the plaintext Authorization/Cookie/API-key value into logs.
/// Non-secret headers still render their value verbatim.
impl std::fmt::Debug for FlowHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let rendered_value: &str = if self.secret {
            "***REDACTED***"
        } else {
            self.value.as_str()
        };
        f.debug_struct("FlowHeader")
            .field("name", &self.name)
            .field("value", &rendered_value)
            .field("secret", &self.secret)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructs_a_plain_header() {
        let h = FlowHeader::plain("Content-Type", "application/json").unwrap();
        assert_eq!(h.name(), "Content-Type");
        assert_eq!(h.value(), "application/json");
        assert!(!h.is_secret());
    }

    #[test]
    fn constructs_a_secret_header() {
        let h = FlowHeader::secret("Authorization", "Bearer t").unwrap();
        assert_eq!(h.name(), "Authorization");
        assert_eq!(h.value(), "Bearer t");
        assert!(h.is_secret());
    }

    #[test]
    fn rejects_empty_name() {
        assert_eq!(
            FlowHeader::new("", "v", false).unwrap_err(),
            DomainError::EmptyHeaderName
        );
    }

    #[test]
    fn rejects_name_too_long() {
        let name = "a".repeat(MAX_HEADER_NAME_LENGTH + 1);
        assert_eq!(
            FlowHeader::new(name, "v", false).unwrap_err(),
            DomainError::HeaderNameTooLong
        );
    }

    #[test]
    fn rejects_value_too_long() {
        let value = "a".repeat(MAX_HEADER_VALUE_LENGTH + 1);
        assert_eq!(
            FlowHeader::new("X-H", value, false).unwrap_err(),
            DomainError::HeaderValueTooLong
        );
    }

    #[test]
    fn debug_redacts_secret_value() {
        let h = FlowHeader::secret("Authorization", "Bearer SUPER_SECRET").unwrap();
        let rendered = format!("{h:?}");
        assert!(!rendered.contains("SUPER_SECRET"));
        assert!(rendered.contains("***REDACTED***"));
        assert!(rendered.contains("Authorization"));
    }

    #[test]
    fn debug_shows_plain_value() {
        let h = FlowHeader::plain("Content-Type", "application/json").unwrap();
        let rendered = format!("{h:?}");
        assert!(rendered.contains("application/json"));
    }

    #[test]
    fn sensitive_name_is_case_insensitive() {
        assert!(FlowHeader::is_sensitive_name("Authorization"));
        assert!(FlowHeader::is_sensitive_name("authorization"));
        assert!(FlowHeader::is_sensitive_name("AUTHORIZATION"));
        assert!(FlowHeader::is_sensitive_name("X-Api-Key"));
        assert!(FlowHeader::is_sensitive_name("x-api-key"));
        assert!(!FlowHeader::is_sensitive_name("Content-Type"));
    }
}
