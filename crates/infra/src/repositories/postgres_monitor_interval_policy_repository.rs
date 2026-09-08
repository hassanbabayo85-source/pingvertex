use application::ports::MonitorIntervalPolicyRepository;
use async_trait::async_trait;
use domain::MonitorKind;
use sqlx::PgPool;
use std::error::Error;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Clone)]
pub struct PostgresMonitorIntervalPolicyRepository {
    pool: PgPool,
}

impl PostgresMonitorIntervalPolicyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MonitorIntervalPolicyRepository for PostgresMonitorIntervalPolicyRepository {
    async fn minimum_interval_seconds(&self, kind: MonitorKind) -> RepoResult<u32> {
        let key = match kind {
            MonitorKind::Http => "http",
            MonitorKind::Tcp => "tcp",
            MonitorKind::Dns => "dns",
            MonitorKind::Heartbeat => "heartbeat",
            MonitorKind::Flow => "flow",
        };
        let value: Option<i32> = sqlx::query_scalar(
            "SELECT min_interval_seconds FROM monitor_interval_policies WHERE kind=$1",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;
        value
            .ok_or_else(|| format!("missing interval policy for monitor kind {key}").into())
            .and_then(|v| u32::try_from(v).map_err(|_| "invalid monitor interval policy".into()))
    }
}

