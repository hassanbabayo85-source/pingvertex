use std::env;

/// S3-compatible object-storage settings for status-page logo uploads.
///
/// Unlike SMTP/Redis (which are optional in self-hosted deployments),
/// `S3LogoStorage` is the production storage backend selected for logo
/// uploads, so missing S3 configuration fails closed at boot.
#[derive(Clone, PartialEq)]
pub struct S3Config {
    pub bucket: String,
    pub region: String,
    pub endpoint: Option<String>,
    pub access_key_id: String,
    pub secret_access_key: String,
    /// Base URL that publicly serves the logo bucket, e.g.
    /// `https://cdn.example.com` or `https://s3.example.com/my-bucket`.
    /// Generated logo URLs are `"{public_base_url}/{key}"`.
    pub public_base_url: String,
    /// Force path-style addressing (`https://endpoint/bucket/key`) instead
    /// of virtual-hosted style. Defaults to `true` for self-hosted
    /// S3-compatible providers (MinIO, Garage, etc.).
    pub path_style: bool,
}

/// PART 6 — custom Debug that redacts both AWS credentials.
impl std::fmt::Debug for S3Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3Config")
            .field("bucket", &self.bucket)
            .field("region", &self.region)
            .field("endpoint", &self.endpoint)
            .field("access_key_id", &"<redacted>")
            .field("secret_access_key", &"<redacted>")
            .field("public_base_url", &self.public_base_url)
            .field("path_style", &self.path_style)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum S3ConfigError {
    MissingS3Bucket,
    MissingS3Region,
    MissingS3AccessKeyId,
    MissingS3SecretAccessKey,
    MissingS3PublicBaseUrl,
    InvalidS3PublicBaseUrl(String),
    InvalidS3Region(String),
    S3ClientError(String),
}

impl std::fmt::Display for S3ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingS3Bucket => write!(f, "S3_BUCKET environment variable is not set"),
            Self::MissingS3Region => write!(f, "S3_REGION environment variable is not set"),
            Self::MissingS3AccessKeyId => {
                write!(f, "S3_ACCESS_KEY_ID environment variable is not set")
            }
            Self::MissingS3SecretAccessKey => {
                write!(f, "S3_SECRET_ACCESS_KEY environment variable is not set")
            }
            Self::MissingS3PublicBaseUrl => {
                write!(f, "S3_PUBLIC_BASE_URL environment variable is not set")
            }
            Self::InvalidS3PublicBaseUrl(value) => {
                write!(
                    f,
                    "S3_PUBLIC_BASE_URL '{value}' must start with http:// or https://"
                )
            }
            Self::InvalidS3Region(value) => {
                write!(f, "S3_REGION '{value}' is not a valid AWS region")
            }
            Self::S3ClientError(value) => {
                write!(f, "failed to initialize S3 client: {value}")
            }
        }
    }
}

impl std::error::Error for S3ConfigError {}

fn non_empty_env(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn bool_env(key: &str, default: bool) -> bool {
    env::var(key)
        .ok()
        .map(|v| v.trim().eq_ignore_ascii_case("true") || v.trim() == "1")
        .unwrap_or(default)
}

impl S3Config {
    pub fn try_from_env() -> Result<Self, S3ConfigError> {
        let bucket = non_empty_env("S3_BUCKET").ok_or(S3ConfigError::MissingS3Bucket)?;
        let region = non_empty_env("S3_REGION").ok_or(S3ConfigError::MissingS3Region)?;
        let endpoint = non_empty_env("S3_ENDPOINT");
        let access_key_id =
            non_empty_env("S3_ACCESS_KEY_ID").ok_or(S3ConfigError::MissingS3AccessKeyId)?;
        let secret_access_key =
            non_empty_env("S3_SECRET_ACCESS_KEY").ok_or(S3ConfigError::MissingS3SecretAccessKey)?;

        let public_base_url = non_empty_env("S3_PUBLIC_BASE_URL")
            .ok_or(S3ConfigError::MissingS3PublicBaseUrl)?
            .trim_end_matches('/')
            .to_string();

        if public_base_url.is_empty()
            || (!public_base_url.starts_with("https://") && !public_base_url.starts_with("http://"))
        {
            return Err(S3ConfigError::InvalidS3PublicBaseUrl(public_base_url));
        }

        Ok(Self {
            bucket,
            region,
            endpoint,
            access_key_id,
            secret_access_key,
            public_base_url,
            path_style: bool_env("S3_PATH_STYLE", true),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    const ALL_VARS: &[&str] = &[
        "S3_BUCKET",
        "S3_REGION",
        "S3_ENDPOINT",
        "S3_ACCESS_KEY_ID",
        "S3_SECRET_ACCESS_KEY",
        "S3_PUBLIC_BASE_URL",
        "S3_PATH_STYLE",
    ];

    fn clear_s3_env() {
        unsafe {
            for key in ALL_VARS {
                env::remove_var(key);
            }
        }
    }

    #[test]
    #[serial(s3_env)]
    fn fails_closed_when_bucket_missing() {
        clear_s3_env();
        assert_eq!(
            S3Config::try_from_env(),
            Err(S3ConfigError::MissingS3Bucket)
        );
    }

    #[test]
    #[serial(s3_env)]
    fn accepts_complete_valid_configuration() {
        clear_s3_env();
        unsafe {
            env::set_var("S3_BUCKET", "pingvertex-logos");
            env::set_var("S3_REGION", "us-east-1");
            env::set_var("S3_ENDPOINT", "https://s3.example.com");
            env::set_var("S3_ACCESS_KEY_ID", "access-key");
            env::set_var("S3_SECRET_ACCESS_KEY", "secret-key");
            env::set_var("S3_PUBLIC_BASE_URL", "https://cdn.example.com/");
        }

        let config = S3Config::try_from_env().expect("expected valid config");
        assert_eq!(config.bucket, "pingvertex-logos");
        assert_eq!(config.region, "us-east-1");
        assert_eq!(config.endpoint.as_deref(), Some("https://s3.example.com"));
        assert_eq!(config.public_base_url, "https://cdn.example.com");
        assert!(config.path_style);

        clear_s3_env();
    }

    #[test]
    #[serial(s3_env)]
    fn rejects_non_http_public_base_url() {
        clear_s3_env();
        unsafe {
            env::set_var("S3_BUCKET", "pingvertex-logos");
            env::set_var("S3_REGION", "us-east-1");
            env::set_var("S3_ENDPOINT", "https://s3.example.com");
            env::set_var("S3_ACCESS_KEY_ID", "access-key");
            env::set_var("S3_SECRET_ACCESS_KEY", "secret-key");
            env::set_var("S3_PUBLIC_BASE_URL", "ftp://cdn.example.com");
        }

        assert!(matches!(
            S3Config::try_from_env(),
            Err(S3ConfigError::InvalidS3PublicBaseUrl(_))
        ));

        clear_s3_env();
    }
}
