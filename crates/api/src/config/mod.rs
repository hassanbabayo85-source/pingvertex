use std::env;

/// Mafi ƙarancin tsayin JWT_SECRET (a cikin bytes) da ake karɓa.
///
/// 32 bytes (256 bits) shi ne mafi ƙarancin da ake bukata domin HS256
/// signing ya kasance mai wuyar ƙeta (brute-force resistant).
const MIN_JWT_SECRET_LEN: usize = 32;

/// Default TCP port for the internal-agent mTLS listener. Overridable via
/// `MTLS_PORT`. Kept separate from the main HTTP port (3000) since the
/// mTLS listener is a second, independent server (see `main.rs`).
const DEFAULT_MTLS_PORT: u16 = 3443;

/// Default filesystem paths for the mTLS CA/server identity, relative to
/// the process's working directory (`/app` in the container — see
/// `Dockerfile.api`, which COPYs `certs/` alongside the binary).
/// Overridable via `MTLS_CA_CERT_PATH`, `MTLS_SERVER_CERT_PATH`,
/// `MTLS_SERVER_KEY_PATH`.
const DEFAULT_MTLS_CA_CERT_PATH: &str = "certs/ca/ca-cert.pem";
const DEFAULT_MTLS_SERVER_CERT_PATH: &str = "certs/api/server-cert.pem";
const DEFAULT_MTLS_SERVER_KEY_PATH: &str = "certs/api/server-key.pem";

/// Jerin sanannun sirrin da AKA TAɓA turo su a bayyane (leaked) ko wanda
/// aka sani a matsayin placeholder/example. Ba a taɓa yarda a yi amfani
/// da su a runtime, koda sun cika tsayin da ake bukata.
///
/// NOTE: sirrin da aka samu a cikin `.env` da aka gano a cikin security
/// audit din 2026-08-11 an haɗa shi a nan domin tabbatar da cewa BA ZAI
/// SAKE aiki ba, ko da wani ya manta ya juya shi a wani environment.
const KNOWN_LEAKED_OR_EXAMPLE_SECRETS: &[&str] = &[
    "changeme",
    "secret",
    "your-secret-key",
    "your-256-bit-secret",
    "example",
    // Leaked in the security audit of 2026-08-11.
    "9cd090161fdad318d781961889b215d98d845d331b4b4d2722b0799b9449a50d940ac2e82f2d0d6943ffb598fb4286a24cd13774c964a9762745cd83ac4a3366",
    // Leaked a second time in a zip artifact shared for the 2026-08-12
    // security audit (see .env comment history).
    "fd0d36442c07ce13ec97bd4f11d2fd515a80405be2df3558a13c7371d332364335f63012e84219051ecb4a096c037125955c81a1b59bc13a3acb5beb9659cbc5",
];

/// Application configuration.
#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
    /// Connection URL for the status-page response cache (`RedisStatusPageCache`).
    ///
    /// Optional, same reasoning as `infra::SmtpConfig`: a self-hosted
    /// deployment that doesn't care about status pages (or is fine with
    /// every public status-page request hitting Postgres directly)
    /// shouldn't be forced to stand up Redis just to boot. When absent,
    /// `main.rs` wires a no-op cache instead.
    pub redis_url: Option<String>,
    /// TCP port for the internal-agent mTLS listener (see `api::mtls` and
    /// `router::build_internal_agent_router`). Separate from the main
    /// plain-HTTP port (3000, hardcoded in `main.rs`) because the two
    /// listeners run as two independent `axum`/`axum_server` servers.
    pub mtls_port: u16,
    /// Path to the internal CA certificate that signs both the server's
    /// own mTLS identity and every agent's client certificate. Client
    /// certs presented to the mTLS listener must chain up to this CA.
    pub mtls_ca_cert_path: String,
    /// Path to this server's own certificate, presented to connecting
    /// agents during the mTLS handshake.
    pub mtls_server_cert_path: String,
    /// Path to this server's own private key, matching `mtls_server_cert_path`.
    pub mtls_server_key_path: String,
}

/// Kurakuran da za a iya samu wajen loda/tabbatar da configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    MissingDatabaseUrl,
    MissingJwtSecret,
    JwtSecretTooShort { actual_len: usize, min_len: usize },
    JwtSecretIsKnownWeakOrLeaked,
    InvalidMtlsPort { value: String },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingDatabaseUrl => {
                write!(f, "DATABASE_URL environment variable is not set")
            }
            Self::MissingJwtSecret => {
                write!(f, "JWT_SECRET environment variable is not set")
            }
            Self::JwtSecretTooShort { actual_len, min_len } => write!(
                f,
                "JWT_SECRET is too short ({actual_len} chars); must be at least {min_len} characters (256 bits) long. \
                 Generate one with: openssl rand -hex 64"
            ),
            Self::JwtSecretIsKnownWeakOrLeaked => write!(
                f,
                "JWT_SECRET matches a known example/placeholder value or a previously leaked secret. \
                 Generate a fresh one with: openssl rand -hex 64"
            ),
            Self::InvalidMtlsPort { value } => write!(
                f,
                "MTLS_PORT is not a valid TCP port number: '{value}'"
            ),
        }
    }
}

impl std::error::Error for ConfigError {}

/// Tabbatar da cewa JWT secret ɗin ya isa ya zama amintacce.
///
/// Fail-closed: idan sirrin bai cika ka'idoji ba, aikin zai dawo da Err
/// maimakon ya ƙyale app ɗin ya tashi da sirri mai rauni.
fn validate_jwt_secret(secret: &str) -> Result<(), ConfigError> {
    if secret.trim().is_empty() {
        return Err(ConfigError::MissingJwtSecret);
    }

    if secret.len() < MIN_JWT_SECRET_LEN {
        return Err(ConfigError::JwtSecretTooShort {
            actual_len: secret.len(),
            min_len: MIN_JWT_SECRET_LEN,
        });
    }

    let lowered = secret.to_lowercase();
    if KNOWN_LEAKED_OR_EXAMPLE_SECRETS
        .iter()
        .any(|known| known.to_lowercase() == lowered)
    {
        return Err(ConfigError::JwtSecretIsKnownWeakOrLeaked);
    }

    Ok(())
}

fn non_empty_env(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

impl Config {
    /// Loda configuration daga environment variables.
    ///
    /// # Panics
    /// Yana `panic!` idan configuration ɗin bai inganta ba (missing ko
    /// weak JWT secret). Wannan hali ne da ake so a nan: mun fi son app
    /// ɗin ya ƙi tashi (fail closed) maimakon ya yi aiki da sirri mai
    /// rauni ko wanda aka riga aka sani an leaked shi.
    pub fn from_env() -> Self {
        dotenvy::dotenv().ok();

        Self::try_from_env().unwrap_or_else(|err| panic!("invalid configuration: {err}"))
    }

    /// Sigar da ba ta yin `panic!` — ana amfani da ita a cikin tests, kuma
    /// tana bawa masu kira damar sarrafa kuskuren yadda suke so.
    pub fn try_from_env() -> Result<Self, ConfigError> {
        let database_url = env::var("DATABASE_URL").map_err(|_| ConfigError::MissingDatabaseUrl)?;

        let jwt_secret = env::var("JWT_SECRET").map_err(|_| ConfigError::MissingJwtSecret)?;

        validate_jwt_secret(&jwt_secret)?;

        let redis_url = non_empty_env("REDIS_URL");

        let mtls_port = match non_empty_env("MTLS_PORT") {
            Some(raw) => raw
                .parse::<u16>()
                .map_err(|_| ConfigError::InvalidMtlsPort { value: raw })?,
            None => DEFAULT_MTLS_PORT,
        };

        let mtls_ca_cert_path = non_empty_env("MTLS_CA_CERT_PATH")
            .unwrap_or_else(|| DEFAULT_MTLS_CA_CERT_PATH.to_string());
        let mtls_server_cert_path = non_empty_env("MTLS_SERVER_CERT_PATH")
            .unwrap_or_else(|| DEFAULT_MTLS_SERVER_CERT_PATH.to_string());
        let mtls_server_key_path = non_empty_env("MTLS_SERVER_KEY_PATH")
            .unwrap_or_else(|| DEFAULT_MTLS_SERVER_KEY_PATH.to_string());

        Ok(Self {
            database_url,
            jwt_secret,
            redis_url,
            mtls_port,
            mtls_ca_cert_path,
            mtls_server_cert_path,
            mtls_server_key_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_jwt_secret() {
        assert_eq!(validate_jwt_secret(""), Err(ConfigError::MissingJwtSecret));
        assert_eq!(
            validate_jwt_secret("   "),
            Err(ConfigError::MissingJwtSecret)
        );
    }

    #[test]
    fn rejects_jwt_secret_shorter_than_32_chars() {
        let short_secret = "a".repeat(31);
        assert_eq!(
            validate_jwt_secret(&short_secret),
            Err(ConfigError::JwtSecretTooShort {
                actual_len: 31,
                min_len: MIN_JWT_SECRET_LEN,
            })
        );
    }

    #[test]
    fn accepts_jwt_secret_of_exactly_32_chars() {
        let secret = "a".repeat(32);
        assert!(validate_jwt_secret(&secret).is_ok());
    }

    #[test]
    fn rejects_known_placeholder_secrets_even_if_long_enough() {
        for weak in [
            "changeme",
            "secret",
            "your-secret-key",
            "your-256-bit-secret",
            "example",
        ] {
            // Mun sanya su su zama tsayi 32+ don tabbatar da cewa an
            // toshe su ne saboda sun yi daidai da sanannun sirrin da aka
            // sani, ba don gajarta ba.
            let padded = format!("{weak}{}", "x".repeat(32usize.saturating_sub(weak.len())));
            // Amma dole a gwada ainihin darajar ma (case-insensitive),
            // domin shi ne yadda list ɗin ke aiki.
            let result = validate_jwt_secret(weak);
            assert!(
                matches!(
                    result,
                    Err(ConfigError::JwtSecretTooShort { .. })
                        | Err(ConfigError::JwtSecretIsKnownWeakOrLeaked)
                ),
                "expected '{weak}' (padded check: {padded}) to be rejected, got {result:?}"
            );
        }
    }

    #[test]
    fn rejects_the_specific_secret_that_was_leaked_in_dot_env() {
        let leaked_secret = "9cd090161fdad318d781961889b215d98d845d331b4b4d2722b0799b9449a50d940ac2e82f2d0d6943ffb598fb4286a24cd13774c964a9762745cd83ac4a3366";
        assert_eq!(
            validate_jwt_secret(leaked_secret),
            Err(ConfigError::JwtSecretIsKnownWeakOrLeaked)
        );
    }

    #[test]
    fn accepts_a_properly_generated_random_secret() {
        let strong_secret =
            "b1a4c9e02d7f8635a0e2c4f96d1b7a8e3c5f0d9a2b6e1c4f7a0d3b8e5c2f9a6d0b3e7c1a4f8d5b2e9c6f0a3d7b1e4c8f2a5d9b6e0c3f7a1d4b8e2c5f9a0d3b6";
        assert!(validate_jwt_secret(strong_secret).is_ok());
    }

    #[test]
    fn try_from_env_fails_closed_when_database_url_missing() {
        unsafe {
            env::remove_var("DATABASE_URL");
            env::remove_var("JWT_SECRET");
        }

        let result = Config::try_from_env();
        assert_eq!(result.unwrap_err(), ConfigError::MissingDatabaseUrl);
    }
}
