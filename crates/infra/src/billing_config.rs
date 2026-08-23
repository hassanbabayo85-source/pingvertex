use std::env;

/// Known placeholder/example billing secrets that must never be accepted,
/// even if `BILLING_SECRET_KEY` happens to be set to one of them.
const KNOWN_PLACEHOLDER_BILLING_SECRETS: &[&str] = &[
    "changeme",
    "change-me",
    "password",
    "secret",
    "your-secret-key",
    "your-billing-secret",
    "example",
    "sk_test_example",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BillingConfig {
    pub provider: String,
    pub secret_key: Option<String>,
    /// HMAC secret for inbound billing webhook signature verification.
    /// Falls back to `BILLING_SECRET_KEY` if `BILLING_WEBHOOK_SECRET`
    /// is not set, so deployments with a single provider secret can use
    /// one environment variable.
    pub webhook_secret: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BillingConfigError {
    MissingSecretKey { provider: String },
    SecretKeyIsKnownPlaceholder,
}

impl std::fmt::Display for BillingConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingSecretKey { provider } => {
                write!(
                    f,
                    "BILLING_SECRET_KEY is not set for billing provider '{provider}'"
                )
            }
            Self::SecretKeyIsKnownPlaceholder => write!(
                f,
                "BILLING_SECRET_KEY matches a known example/placeholder value; set a real credential"
            ),
        }
    }
}

impl std::error::Error for BillingConfigError {}

fn non_empty_env(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

impl BillingConfig {
    /// Loads billing configuration from environment variables.
    ///
    /// Provider-specific secret keys are only read through this config
    /// function — never hardcoded in any Rust source file. Unknown
    /// non-dev providers require `BILLING_SECRET_KEY` and reject known
    /// placeholder values. Supported providers for this worker are:
    /// `disabled` and `fake`.
    pub fn try_from_env() -> Result<Self, BillingConfigError> {
        let provider = non_empty_env("BILLING_PROVIDER")
            .unwrap_or_else(|| "disabled".to_owned())
            .to_lowercase();

        let secret_key = non_empty_env("BILLING_SECRET_KEY");
        if let Some(secret) = &secret_key {
            let lowered = secret.to_lowercase();
            if KNOWN_PLACEHOLDER_BILLING_SECRETS
                .iter()
                .any(|known| known.to_lowercase() == lowered)
            {
                return Err(BillingConfigError::SecretKeyIsKnownPlaceholder);
            }
        }

        let webhook_secret = non_empty_env("BILLING_WEBHOOK_SECRET").or_else(|| secret_key.clone());

        match provider.as_str() {
            "disabled" | "fake" => Ok(Self {
                provider,
                secret_key: None,
                webhook_secret,
            }),
            other => {
                let _secret = secret_key.ok_or_else(|| BillingConfigError::MissingSecretKey {
                    provider: other.to_owned(),
                })?;
                Ok(Self {
                    provider: other.to_owned(),
                    secret_key: Some(_secret),
                    webhook_secret,
                })
            }
        }
    }
}
