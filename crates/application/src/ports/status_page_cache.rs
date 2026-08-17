use async_trait::async_trait;

pub type CacheResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Abstraction over a cache for the unauthenticated public status page
/// response (`GET /status/:subdomain` and equivalent custom-domain
/// routing).
///
/// This endpoint is unauthenticated and publicly linked, so it is a
/// natural target for high, spiky traffic; a cache is required (not
/// optional) in front of the database lookups it performs. See the task's
/// CACHING requirement.
///
/// Implementations should treat cache misses/failures as non-fatal: a
/// `get` error or a `set` error should never fail the request, only skip
/// the cache for that call — the handler falls back to querying the
/// database directly.
#[async_trait]
pub trait StatusPageCache: Send + Sync {
    /// Returns the cached JSON response body for `key`, if present and
    /// not expired.
    async fn get(&self, key: &str) -> CacheResult<Option<String>>;

    /// Stores `value` (a serialized JSON response body) under `key` with
    /// the given time-to-live.
    async fn set(&self, key: &str, value: &str, ttl_seconds: u64) -> CacheResult<()>;

    /// Removes `key`, e.g. after the underlying status page is updated.
    async fn invalidate(&self, key: &str) -> CacheResult<()>;
}
