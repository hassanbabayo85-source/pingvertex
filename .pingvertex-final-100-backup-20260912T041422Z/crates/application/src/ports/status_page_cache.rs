use async_trait::async_trait;

pub type CacheResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Cache key for the unauthenticated "list all public monitors" endpoint
/// (`GET /api/v1/public/status`). A single global key — this endpoint has
/// no per-tenant/per-page scoping (see `MonitorRepository::find_public`),
/// unlike the per-status-page cache keys (`status_page:id:{uuid}`) used by
/// `render_public_status_page`.
///
/// Shared, rather than a private literal duplicated in each crate that
/// needs it, because BOTH the API (which reads/populates it in
/// `get_public_status`) and the worker (which invalidates it from
/// `IncidentManager` whenever an incident opens/resolves, since that
/// changes a monitor's public status) must agree on the exact string.
pub const PUBLIC_STATUS_LIST_CACHE_KEY: &str = "public_status:list";

/// Abstraction over a cache for the unauthenticated public status page
/// response (`GET /status/:subdomain` and equivalent custom-domain
/// routing), and for the public status list (`GET /public/status`).
///
/// These endpoints are unauthenticated and publicly linked, so they are a
/// natural target for high, spiky traffic; a cache is required (not
/// optional) in front of the database lookups they perform. See the
/// task's CACHING requirement.
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

    /// Removes `key`, e.g. after the underlying status page is updated,
    /// or after an incident open/resolve changes a monitor's public
    /// status.
    async fn invalidate(&self, key: &str) -> CacheResult<()>;
}
