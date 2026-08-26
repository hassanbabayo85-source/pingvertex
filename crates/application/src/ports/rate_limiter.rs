use async_trait::async_trait;

pub type RateLimiterResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Distributed rate limiter used for security-sensitive endpoints.
///
/// Implementations MUST fail closed when the backing store is unavailable.
/// `check_and_record` atomically checks the current counter and records the
/// request when the limit has not yet been reached.
#[async_trait]
pub trait RateLimiter: Send + Sync {
    async fn check_and_record(
        &self,
        namespace: &str,
        key: &str,
        max_requests: u32,
        window_seconds: u64,
    ) -> RateLimiterResult<bool>;
}

#[async_trait]
impl<T> RateLimiter for std::sync::Arc<T>
where
    T: RateLimiter + ?Sized,
{
    async fn check_and_record(
        &self,
        namespace: &str,
        key: &str,
        max_requests: u32,
        window_seconds: u64,
    ) -> RateLimiterResult<bool> {
        (**self)
            .check_and_record(namespace, key, max_requests, window_seconds)
            .await
    }
}
