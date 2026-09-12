use async_trait::async_trait;
use std::{error::Error, sync::Arc};

use domain::LogoImage;

pub type StorageResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Stores an already-validated logo image and returns a publicly reachable
/// URL. Implementations live in the infra layer (S3-compatible object
/// storage, local disk, etc.). The domain/application layers never perform
/// this I/O directly.
#[async_trait]
pub trait LogoStorage: Send + Sync {
    async fn store_logo(&self, logo: &LogoImage) -> StorageResult<String>;
}

#[async_trait]
impl<T> LogoStorage for Arc<T>
where
    T: LogoStorage + Send + Sync + ?Sized,
{
    async fn store_logo(&self, logo: &LogoImage) -> StorageResult<String> {
        (**self).store_logo(logo).await
    }
}
