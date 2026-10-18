use std::env;

/// Known placeholder/example billing secrets that must never be accepted,
/// even if the operator happens to set an env var to one of them.
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

#[derive(Clone, PartialEq, Eq)]
pub struct BillingConfig {
    pub provider: String,
    pub secret_key: Option<String>,
    /// HMAC secret used to verify inbound billing webhook signatures.
    ///
    /// * For `paystack`, this is ALWAYS `BILLING_SECRET_KEY`. Paystack
    ///   signs its webhook payloads with the provider secret key, never
    ///   with a separately-configured webhook secret. Allowing
    ///   `BILLING_WEBHOOK_SECRET` to override this would let a
    ///   misconfigured deployment accept unverified Paystack webhooks
    ///   with a non-Paystack HMAC key.
    /// * For other providers, this is `BILLING_WEBHOOK_SECRET` if set,
    ///   otherwise it falls back to `BILLING_SECRET_KEY` so single-secret
    ///   deployments keep working.
    pub webhook_secret: Option<String>,
}

/// Custom Debug that redacts secrets.
impl std::fmt::Debug for BillingConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BillingConfig")
            .field("provider", &self.provider)
            .field(
                "secret_key",
                &self.secret_key.as_ref().map(|_| "<redacted>"),
            )
            .field(
                "webhook_secret",
                &self.webhook_secret.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BillingConfigError {
    MissingSecretKey { provider: String },
    SecretKeyIsKnownPlaceholder,
    WebhookSecretIsKnownPlaceholder,
    ProductionProviderRequired { provider: String },
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
            Self::WebhookSecretIsKnownPlaceholder => write!(
                f,
                "BILLING_WEBHOOK_SECRET matches a known example/placeholder value; set a real credential"
            ),
            Self::ProductionProviderRequired { provider } => write!(
                f,
                "production billing requires the real Paystack provider; got '{provider}'"
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

fn is_known_placeholder(value: &str) -> bool {
    let lowered = value.trim().to_lowercase();
    KNOWN_PLACEHOLDER_BILLING_SECRETS
        .iter()
        .any(|known| known.to_lowercase() == lowered)
}

impl BillingConfig {
    /// Loads billing configuration from environment variables.
    ///
    /// Provider-specific secret keys are only read through this config
    /// function — never hardcoded in any Rust source file. Unknown
    /// non-dev providers require `BILLING_SECRET_KEY` and reject known
    /// placeholder values. Supported providers for this worker are:
    /// `disabled`, `fake`, and `paystack`.
    ///
    /// Production (`ENVIRONMENT=production`) additionally requires:
    ///   * `BILLING_PROVIDER=paystack`
    ///   * a non-placeholder webhook secret resolved (see
    ///     `webhook_secret` field docs for Paystack vs. other providers).
    pub fn try_from_env() -> Result<Self, BillingConfigError> {
        let provider = non_empty_env("BILLING_PROVIDER")
            .unwrap_or_else(|| "disabled".to_owned())
            .to_lowercase();

        let secret_key = non_empty_env("BILLING_SECRET_KEY");
        if let Some(secret) = &secret_key
            && is_known_placeholder(secret)
        {
            return Err(BillingConfigError::SecretKeyIsKnownPlaceholder);
        }

        let explicit_webhook_secret = non_empty_env("BILLING_WEBHOOK_SECRET");
        if let Some(secret) = &explicit_webhook_secret
            && is_known_placeholder(secret)
        {
            return Err(BillingConfigError::WebhookSecretIsKnownPlaceholder);
        }

        // Paystack signs its webhook payloads (X-Paystack-Signature,
        // HMAC-SHA512) with the provider secret key. The provider's own
        // contract does not expose a separate webhook secret, so the
        // secret key MUST be the verification key for the Paystack path.
        // Other providers may use a distinct `BILLING_WEBHOOK_SECRET`.
        let webhook_secret = match provider.as_str() {
            "paystack" => secret_key.clone(),
            _ => explicit_webhook_secret.or_else(|| secret_key.clone()),
        };

        let production = env::var("ENVIRONMENT")
            .ok()
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "production" | "prod"
                )
            })
            .unwrap_or(false);

        if production && !matches!(provider.as_str(), "paystack") {
            return Err(BillingConfigError::ProductionProviderRequired { provider });
        }

        if production && webhook_secret.is_none() {
            return Err(BillingConfigError::MissingSecretKey {
                provider: "billing webhook".to_owned(),
            });
        }

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Env-var manipulation is process-global, so tests that touch the
    // billing env vars must be serialized.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    const ENV_KEYS: &[&str] = &[
        "BILLING_PROVIDER",
        "BILLING_SECRET_KEY",
        "BILLING_WEBHOOK_SECRET",
        "ENVIRONMENT",
    ];

    fn with_env<F: FnOnce()>(vars: &[(&str, &str)], f: F) {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        for k in ENV_KEYS {
            // SAFETY: serialized by ENV_LOCK above.
            unsafe { env::remove_var(k) };
        }
        for (k, v) in vars {
            // SAFETY: serialized by ENV_LOCK above.
            unsafe { env::set_var(k, v) };
        }
        f();
    }

    #[test]
    fn paystack_uses_secret_key_for_webhook_verification() {
        with_env(
            &[
                ("BILLING_PROVIDER", "paystack"),
                ("BILLING_SECRET_KEY", "sk_live_realsecret"),
                ("BILLING_WEBHOOK_SECRET", "some_other_value"),
            ],
            || {
                let cfg = BillingConfig::try_from_env().unwrap();
                assert_eq!(
                    cfg.webhook_secret.as_deref(),
                    Some("sk_live_realsecret"),
                    "Paystack must verify webhook HMAC with BILLING_SECRET_KEY, \
                     not BILLING_WEBHOOK_SECRET"
                );
            },
        );
    }

    #[test]
    fn paystack_falls_back_to_secret_key_when_no_webhook_secret() {
        with_env(
            &[
                ("BILLING_PROVIDER", "paystack"),
                ("BILLING_SECRET_KEY", "sk_live_realsecret"),
            ],
            || {
                let cfg = BillingConfig::try_from_env().unwrap();
                assert_eq!(cfg.webhook_secret.as_deref(), Some("sk_live_realsecret"));
            },
        );
    }

    #[test]
    fn non_paystack_provider_can_use_distinct_webhook_secret() {
        with_env(
            &[
                ("BILLING_PROVIDER", "fake"),
                ("BILLING_SECRET_KEY", "some_secret_key"),
                ("BILLING_WEBHOOK_SECRET", "distinct_webhook_secret"),
            ],
            || {
                let cfg = BillingConfig::try_from_env().unwrap();
                assert_eq!(
                    cfg.webhook_secret.as_deref(),
                    Some("distinct_webhook_secret")
                );
            },
        );
    }

    #[test]
    fn placeholder_secret_key_is_rejected() {
        with_env(
            &[
                ("BILLING_PROVIDER", "paystack"),
                ("BILLING_SECRET_KEY", "changeme"),
            ],
            || {
                assert!(matches!(
                    BillingConfig::try_from_env(),
                    Err(BillingConfigError::SecretKeyIsKnownPlaceholder)
                ));
            },
        );
    }

    #[test]
    fn placeholder_webhook_secret_is_rejected() {
        with_env(
            &[
                ("BILLING_PROVIDER", "disabled"),
                ("BILLING_WEBHOOK_SECRET", "change-me"),
            ],
            || {
                assert!(matches!(
                    BillingConfig::try_from_env(),
                    Err(BillingConfigError::WebhookSecretIsKnownPlaceholder)
                ));
            },
        );
    }

    #[test]
    fn empty_or_whitespace_webhook_secret_is_treated_as_absent() {
        with_env(
            &[
                ("BILLING_PROVIDER", "disabled"),
                ("BILLING_WEBHOOK_SECRET", "   "),
            ],
            || {
                let cfg = BillingConfig::try_from_env().unwrap();
                assert_eq!(cfg.webhook_secret, None);
            },
        );
    }

    #[test]
    fn production_requires_paystack_provider() {
        with_env(
            &[("ENVIRONMENT", "production"), ("BILLING_PROVIDER", "fake")],
            || {
                assert!(matches!(
                    BillingConfig::try_from_env(),
                    Err(BillingConfigError::ProductionProviderRequired { .. })
                ));
            },
        );
    }

    #[test]
    fn production_requires_a_resolved_webhook_secret() {
        with_env(
            &[
                ("ENVIRONMENT", "production"),
                ("BILLING_PROVIDER", "paystack"),
            ],
            || {
                assert!(matches!(
                    BillingConfig::try_from_env(),
                    Err(BillingConfigError::MissingSecretKey { .. })
                ));
            },
        );
    }
}
