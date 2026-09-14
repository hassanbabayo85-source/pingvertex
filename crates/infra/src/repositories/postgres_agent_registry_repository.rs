use std::error::Error;

use application::ports::AgentRegistry;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{Agent, AgentStatus};
use serde_json::Value;
use sqlx::{FromRow, PgPool};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct PostgresAgentRegistryRepository {
    pool: PgPool,
}

impl PostgresAgentRegistryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(FromRow)]
struct Row {
    agent_id: String,
    region: String,
    capabilities: Value,
    status: String,
    certificate_fingerprint: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
}

impl Row {
    fn into_domain(self) -> Agent {
        // Capabilities is JSONB; malformed rows degrade to an empty
        // list rather than poisoning the whole request. The security
        // decision (region match) does NOT depend on capabilities.
        let capabilities: Vec<String> =
            serde_json::from_value(self.capabilities).unwrap_or_default();
        Agent {
            agent_id: self.agent_id,
            region: self.region,
            capabilities,
            // An unknown status string is treated as `Revoked` — fail
            // closed, never silently treat a future status as Active.
            status: AgentStatus::parse(&self.status).unwrap_or(AgentStatus::Revoked),
            certificate_fingerprint: self.certificate_fingerprint,
            created_at: self.created_at,
            updated_at: self.updated_at,
            revoked_at: self.revoked_at,
        }
    }
}

#[async_trait]
impl AgentRegistry for PostgresAgentRegistryRepository {
    async fn find_by_id(&self, agent_id: &str) -> Result<Option<Agent>> {
        let row = sqlx::query_as::<_, Row>(
            r#"
            SELECT agent_id, region, capabilities, status, certificate_fingerprint,
                   created_at, updated_at, revoked_at
            FROM agents
            WHERE agent_id = $1
            "#,
        )
        .bind(agent_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Row::into_domain))
    }

    async fn list_active(&self) -> Result<Vec<Agent>> {
        let rows = sqlx::query_as::<_, Row>(
            r#"
            SELECT agent_id, region, capabilities, status, certificate_fingerprint,
                   created_at, updated_at, revoked_at
            FROM agents
            WHERE status = 'active'
            ORDER BY agent_id
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Row::into_domain).collect())
    }
}
