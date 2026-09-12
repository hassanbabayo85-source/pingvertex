use application::ports::LogoStorage;
use async_trait::async_trait;
use domain::value_objects::LogoImage;
use std::error::Error;
use uuid::Uuid;

/// In-memory logo storage for application/API tests.
///
/// It does not retain uploaded bytes; it only returns a deterministic,
/// unique test CDN URL shaped like the one `S3LogoStorage` will return.
#[derive(Debug, Clone, Default)]
pub struct InMemoryLogoStorage;

impl InMemoryLogoStorage {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl LogoStorage for InMemoryLogoStorage {
    async fn store_logo(&self, logo: &LogoImage) -> Result<String, Box<dyn Error + Send + Sync>> {
        let key = format!(
            "logos/inmemory/{}.{}",
            Uuid::new_v4().simple(),
            logo.file_extension()
        );
        Ok(format!("https://cdn.test/{key}"))
    }
}
