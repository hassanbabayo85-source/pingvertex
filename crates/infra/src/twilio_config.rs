use std::env;

/// Known placeholder/example Twilio auth tokens that must never be
/// accepted, mirroring `KNOWN_PLACEHOLDER_SMTP_PASSWORDS` in
/// `crates/infra/src/config.rs` (`SmtpConfig`) and
/// `KNOWN_LEAKED_OR_EXAMPLE_SECRETS` in `crates/api/src/config/mod.rs`
/// (`JWT_SECRET`) — same idea, applied to `TWILIO_AUTH_TOKEN`.
const KNOWN_PLACEHOLDER_TWILIO_AUTH_TOKENS: &[&str] = &[
    "changeme",
    "change-me",
    "your-auth-token",
    "your-twilio-auth-token",
    "example",
];

/// Twilio (or Twilio-compatible) SMS provider credentials.
///
/// OPTIONAL, same reasoning as `SmtpConfig`/`S3Config`: a self-hosted
/// deployment that doesn't use SMS notification channels shouldn't be
/// forced to configure Twilio just to boot. `SmsNotificationService`
/// treats a missing/invalid `TwilioConfig` as "SMS notifications are
/// unavailable" at send time, not as a fatal startup error — same
/// pattern `NotificationDispatcher::from_env` already uses for
/// `SmtpConfig`.
///
/// Replaces the three separate ad-hoc `std::env::var("TWILIO_*").ok()`
/// reads previously inlined in `SmsNotificationService::notify`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TwilioConfig {
    pub account_sid: String,
    pub auth_token: String,
    pub from_number: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TwilioConfigError {
    MissingAccountSid,
    MissingAuthToken,
    AuthTokenIsKnownPlaceholder,
    MissingFromNumber,
}

impl std::fmt::Display for TwilioConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingAccountSid => {
                write!(f, "TWILIO_ACCOUNT_SID environment variable is not set")
            }
            Self::MissingAuthToken => {
                write!(f, "TWILIO_AUTH_TOKEN environment variable is not set")
            }
            Self::AuthTokenIsKnownPlaceholder => write!(
                f,
                "TWILIO_AUTH_TOKEN matches a known example/placeholder value; set a real credential"
            ),
            Self::MissingFromNumber => {
                write!(f, "TWILIO_FROM_NUMBER environment variable is not set")
            }
        }
    }
}

impl std::error::Error for TwilioConfigError {}

fn non_empty_env(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

impl TwilioConfig {
    /// Loads Twilio configuration from environment variables.
    pub fn try_from_env() -> Result<Self, TwilioConfigError> {
        let account_sid =
            non_empty_env("TWILIO_ACCOUNT_SID").ok_or(TwilioConfigError::MissingAccountSid)?;

        let auth_token =
            non_empty_env("TWILIO_AUTH_TOKEN").ok_or(TwilioConfigError::MissingAuthToken)?;

        let lowered_token = auth_token.to_lowercase();
        if KNOWN_PLACEHOLDER_TWILIO_AUTH_TOKENS
            .iter()
            .any(|known| known.to_lowercase() == lowered_token)
        {
            return Err(TwilioConfigError::AuthTokenIsKnownPlaceholder);
        }

        let from_number =
            non_empty_env("TWILIO_FROM_NUMBER").ok_or(TwilioConfigError::MissingFromNumber)?;

        Ok(Self {
            account_sid,
            auth_token,
            from_number,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    fn clear_twilio_env() {
        unsafe {
            for key in [
                "TWILIO_ACCOUNT_SID",
                "TWILIO_AUTH_TOKEN",
                "TWILIO_FROM_NUMBER",
            ] {
                env::remove_var(key);
            }
        }
    }

    #[test]
    #[serial(twilio_env)]
    fn fails_closed_when_account_sid_missing() {
        clear_twilio_env();
        assert_eq!(
            TwilioConfig::try_from_env(),
            Err(TwilioConfigError::MissingAccountSid)
        );
    }

    #[test]
    #[serial(twilio_env)]
    fn rejects_known_placeholder_auth_token() {
        clear_twilio_env();
        unsafe {
            env::set_var("TWILIO_ACCOUNT_SID", "ACxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx");
            env::set_var("TWILIO_AUTH_TOKEN", "changeme");
            env::set_var("TWILIO_FROM_NUMBER", "+15555550100");
        }

        assert_eq!(
            TwilioConfig::try_from_env(),
            Err(TwilioConfigError::AuthTokenIsKnownPlaceholder)
        );

        clear_twilio_env();
    }

    #[test]
    #[serial(twilio_env)]
    fn accepts_a_complete_valid_configuration() {
        clear_twilio_env();
        unsafe {
            env::set_var("TWILIO_ACCOUNT_SID", "ACxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx");
            env::set_var("TWILIO_AUTH_TOKEN", "a-real-generated-auth-token-123");
            env::set_var("TWILIO_FROM_NUMBER", "+15555550100");
        }

        let config = TwilioConfig::try_from_env().expect("expected valid config");
        assert_eq!(config.account_sid, "ACxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx");
        assert_eq!(config.from_number, "+15555550100");

        clear_twilio_env();
    }
}
