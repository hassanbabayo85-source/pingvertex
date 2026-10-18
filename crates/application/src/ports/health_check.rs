use async_trait::async_trait;

pub type HealthCheckResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Abstraction over a cheap, read-only check that the API's critical
/// dependency (the database) is actually reachable — used by the
/// readiness endpoint (`GET /ready`) to distinguish "process is up"
/// (liveness, `GET /health`) from "process can actually serve requests"
/// (readiness).
#[async_trait]
pub trait HealthCheckPort: Send + Sync {
    /// Returns `Ok(())` if the dependency is reachable, `Err` otherwise.
    /// Implementations should keep this fast and side-effect-free (e.g. a
    /// bare `SELECT 1`), since it may be polled frequently by an
    /// orchestrator.
    async fn check(&self) -> HealthCheckResult<()>;
}
