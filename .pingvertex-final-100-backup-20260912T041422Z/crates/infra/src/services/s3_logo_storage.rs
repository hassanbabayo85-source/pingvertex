use application::ports::LogoStorage;
use async_trait::async_trait;
use domain::value_objects::LogoImage;
use s3::creds::Credentials;
use s3::{Bucket, Region};
use std::error::Error;
use uuid::Uuid;

use crate::s3_config::{S3Config, S3ConfigError};

/// S3-compatible object storage implementation of [`LogoStorage`].
///
/// Uses `rust-s3` with a rustls-only HTTP client. Object keys are random
/// UUIDs, so an attacker cannot predict/overwrite another status page's
/// logo; file extension is chosen from the magic-byte detection performed
/// in `LogoImage`, never from client-supplied content.
#[derive(Clone)]
pub struct S3LogoStorage {
    bucket: Box<Bucket>,
    public_base_url: String,
}

impl S3LogoStorage {
    pub fn try_from_env() -> Result<Self, S3ConfigError> {
        let config = S3Config::try_from_env()?;
        Self::new(config)
    }

    pub fn new(config: S3Config) -> Result<Self, S3ConfigError> {
        let region = match config.endpoint {
            Some(endpoint) => Region::Custom {
                region: config.region.clone(),
                endpoint,
            },
            None => config
                .region
                .parse::<Region>()
                .map_err(|_| S3ConfigError::InvalidS3Region(config.region.clone()))?,
        };

        let credentials = Credentials::new(
            Some(&config.access_key_id),
            Some(&config.secret_access_key),
            None,
            None,
            None,
        )
        .map_err(|err| S3ConfigError::S3ClientError(err.to_string()))?;

        let bucket = Bucket::new(&config.bucket, region, credentials)
            .map_err(|err| S3ConfigError::S3ClientError(err.to_string()))?;

        let bucket = if config.path_style {
            bucket.with_path_style()
        } else {
            bucket
        };

        Ok(Self {
            bucket,
            public_base_url: config.public_base_url,
        })
    }

    fn object_key(logo: &LogoImage) -> String {
        format!(
            "logos/{}.{}",
            Uuid::new_v4().simple(),
            logo.file_extension()
        )
    }
}

#[async_trait]
impl LogoStorage for S3LogoStorage {
    async fn store_logo(&self, logo: &LogoImage) -> Result<String, Box<dyn Error + Send + Sync>> {
        let key = Self::object_key(logo);

        self.bucket
            .put_object_with_content_type(key.as_str(), logo.data(), logo.content_type())
            .await?;

        Ok(format!("{}/{}", self.public_base_url, key))
    }
}
