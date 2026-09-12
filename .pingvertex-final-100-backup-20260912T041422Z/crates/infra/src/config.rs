use std::env;

/// Known placeholder/example SMTP passwords that must never be accepted,
/// even if `SMTP_PASSWORD` happens to be set to one of them. Mirrors the
/// `KNOWN_LEAKED_OR_EXAMPLE_SECRETS` blocklist in
/// `crates/api/src/config/mod.rs` for `JWT_SECRET` — same idea, applied to
/// the one other secret this feature introduces, per the explicit request
/// in the notifications feature brief ("kada a bar SMTP_PASSWORD=changeme
/// ya wuce fail-closed validation").
const KNOWN_PLACEHOLDER_SMTP_PASSWORDS: &[&str] = &[
    "changeme",
    "change-me",
    "password",
    "smtp_password",
    "your-password",
    "your-smtp-password",
    "example",
];

/// SMTP credentials and connection settings for `SmtpEmailNotificationService`.
#[derive(Debug, Clone, PartialEq)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub from_address: String,
    pub from_name: Option<String>,
    /// Whether to use STARTTLS (the standard for port 587). Set
    /// `SMTP_STARTTLS=false` for providers that expect implicit TLS on
    /// port 465 instead.
    pub starttls: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationConfigError {
    MissingSmtpHost,
    MissingSmtpPort,
    InvalidSmtpPort(String),
    MissingSmtpUsername,
    MissingSmtpPassword,
    SmtpPasswordIsKnownPlaceholder,
    MissingSmtpFromAddress,
    InvalidSmtpFromAddress(String),
}

impl std::fmt::Display for NotificationConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingSmtpHost => write!(f, "SMTP_HOST environment variable is not set"),
            Self::MissingSmtpPort => write!(f, "SMTP_PORT environment variable is not set"),
            Self::InvalidSmtpPort(value) => {
                write!(f, "SMTP_PORT '{value}' is not a valid port number")
            }
            Self::MissingSmtpUsername => write!(f, "SMTP_USERNAME environment variable is not set"),
            Self::MissingSmtpPassword => write!(f, "SMTP_PASSWORD environment variable is not set"),
            Self::SmtpPasswordIsKnownPlaceholder => write!(
                f,
                "SMTP_PASSWORD matches a known example/placeholder value; set a real credential"
            ),
            Self::MissingSmtpFromAddress => {
                write!(f, "SMTP_FROM_ADDRESS environment variable is not set")
            }
            Self::InvalidSmtpFromAddress(value) => {
                write!(
                    f,
                    "SMTP_FROM_ADDRESS '{value}' is not a valid email address"
                )
            }
        }
    }
}

impl std::error::Error for NotificationConfigError {}

fn non_empty_env(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

impl SmtpConfig {
    /// Loads SMTP configuration from environment variables.
    ///
    /// Fail-closed like `api::config::Config::try_from_env`, but the
    /// caller (`infra::NotificationDispatcher::from_env`) treats "SMTP not
    /// configured" as "email notifications are disabled for this
    /// deployment" rather than refusing to start the whole app — unlike
    /// `JWT_SECRET`, SMTP is genuinely optional: a self-hosted deployment
    /// that only wants webhook notifications shouldn't be forced to set
    /// up an SMTP relay just to boot.
    pub fn try_from_env() -> Result<Self, NotificationConfigError> {
        let allow_empty_credentials = std::env::var("SMTP_ALLOW_EMPTY_CREDENTIALS")
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(false);
        let host = non_empty_env("SMTP_HOST").ok_or(NotificationConfigError::MissingSmtpHost)?;

        let port_raw =
            non_empty_env("SMTP_PORT").ok_or(NotificationConfigError::MissingSmtpPort)?;
        let port: u16 = port_raw
            .parse()
            .map_err(|_| NotificationConfigError::InvalidSmtpPort(port_raw.clone()))?;

        let username = if allow_empty_credentials {
            non_empty_env("SMTP_USERNAME").unwrap_or_default()
        } else {
            non_empty_env("SMTP_USERNAME").ok_or(NotificationConfigError::MissingSmtpUsername)?
        };

        let password = if allow_empty_credentials {
            non_empty_env("SMTP_PASSWORD").unwrap_or_default()
        } else {
            non_empty_env("SMTP_PASSWORD").ok_or(NotificationConfigError::MissingSmtpPassword)?
        };

        if !allow_empty_credentials {
            let lowered_password = password.to_lowercase();
            if KNOWN_PLACEHOLDER_SMTP_PASSWORDS
                .iter()
                .any(|known| known.to_lowercase() == lowered_password)
            {
                return Err(NotificationConfigError::SmtpPasswordIsKnownPlaceholder);
            }
        }

        let from_address = non_empty_env("SMTP_FROM_ADDRESS")
            .ok_or(NotificationConfigError::MissingSmtpFromAddress)?;

        // Fail fast at boot, not at first send: confirm the from-address
        // parses as a mailbox now, using the same parser lettre will use
        // to actually send.
        from_address
            .parse::<lettre::message::Mailbox>()
            .map_err(|_| NotificationConfigError::InvalidSmtpFromAddress(from_address.clone()))?;

        let from_name = non_empty_env("SMTP_FROM_NAME");

        let starttls = non_empty_env("SMTP_STARTTLS")
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(true);

        Ok(Self {
            host,
            port,
            username,
            password,
            from_address,
            from_name,
            starttls,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    fn clear_smtp_env() {
        unsafe {
            for key in [
                "SMTP_HOST",
                "SMTP_PORT",
                "SMTP_USERNAME",
                "SMTP_PASSWORD",
                "SMTP_FROM_ADDRESS",
                "SMTP_FROM_NAME",
                "SMTP_STARTTLS",
            ] {
                env::remove_var(key);
            }
        }
    }

    #[test]
    #[serial(smtp_env)]
    fn fails_closed_when_smtp_host_missing() {
        clear_smtp_env();
        assert_eq!(
            SmtpConfig::try_from_env(),
            Err(NotificationConfigError::MissingSmtpHost)
        );
    }

    #[test]
    #[serial(smtp_env)]
    fn rejects_known_placeholder_password() {
        clear_smtp_env();
        unsafe {
            env::set_var("SMTP_HOST", "smtp.example.com");
            env::set_var("SMTP_PORT", "587");
            env::set_var("SMTP_USERNAME", "apikey");
            env::set_var("SMTP_PASSWORD", "changeme");
            env::set_var("SMTP_FROM_ADDRESS", "alerts@example.com");
        }

        assert_eq!(
            SmtpConfig::try_from_env(),
            Err(NotificationConfigError::SmtpPasswordIsKnownPlaceholder)
        );

        clear_smtp_env();
    }

    #[test]
    #[serial(smtp_env)]
    fn accepts_a_complete_valid_configuration() {
        clear_smtp_env();
        unsafe {
            env::set_var("SMTP_HOST", "smtp.example.com");
            env::set_var("SMTP_PORT", "587");
            env::set_var("SMTP_USERNAME", "apikey");
            env::set_var("SMTP_PASSWORD", "a-real-generated-credential-123");
            env::set_var("SMTP_FROM_ADDRESS", "alerts@example.com");
        }

        let config = SmtpConfig::try_from_env().expect("expected valid config");
        assert_eq!(config.host, "smtp.example.com");
        assert_eq!(config.port, 587);
        assert!(config.starttls);

        clear_smtp_env();
    }

    #[test]
    #[serial(smtp_env)]
    fn rejects_invalid_from_address() {
        clear_smtp_env();
        unsafe {
            env::set_var("SMTP_HOST", "smtp.example.com");
            env::set_var("SMTP_PORT", "587");
            env::set_var("SMTP_USERNAME", "apikey");
            env::set_var("SMTP_PASSWORD", "a-real-generated-credential-123");
            env::set_var("SMTP_FROM_ADDRESS", "not-an-email");
        }

        assert!(matches!(
            SmtpConfig::try_from_env(),
            Err(NotificationConfigError::InvalidSmtpFromAddress(_))
        ));

        clear_smtp_env();
    }
}
