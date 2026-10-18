use application::ports::{HealthCheckPort, HealthCheckResult};
use async_trait::async_trait;
use sqlx::PgPool;

/// Postgres-backed `HealthCheckPort` used by the readiness endpoint
/// (`GET /ready`) to confirm the API can actually reach its database,
/// as opposed to `GET /health` (liveness), which intentionally has zero
/// dependencies and only proves the process itself is up.
///
/// Deliberately a bare `SELECT 1` against the existing pool — no new
/// connection, no schema assumptions, cheap enough to poll frequently
/// from an orchestrator's readiness probe.
#[derive(Debug, Clone)]
pub struct PostgresHealthCheck {
    pool: PgPool,
}

impl PostgresHealthCheck {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheckPort for PostgresHealthCheck {
    async fn check(&self) -> HealthCheckResult<()> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }
}
