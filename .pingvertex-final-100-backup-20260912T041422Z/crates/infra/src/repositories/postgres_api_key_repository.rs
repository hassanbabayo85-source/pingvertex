use std::error::Error;
use std::str::FromStr;

use application::ports::ApiKeyRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{
    entities::ApiKey,
    enums::ApiKeyScope,
    shared::ids::{ApiKeyId, TenantId, UserId},
};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct PostgresApiKeyRepository {
    pool: PgPool,
}

impl PostgresApiKeyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn scopes_to_db(scopes: &[ApiKeyScope]) -> Vec<String> {
    scopes.iter().map(|s| s.to_string()).collect()
}

fn scopes_from_db(values: Vec<String>) -> RepoResult<Vec<ApiKeyScope>> {
    values
        .into_iter()
        .map(|value| {
            ApiKeyScope::from_str(&value)
                .map_err(|_| format!("unknown api_keys.scopes entry '{value}' in database").into())
        })
        .collect()
}

#[derive(Debug, Clone, FromRow)]
struct ApiKeyRow {
    id: Uuid,
    tenant_id: Uuid,
    user_id: Uuid,
    name: String,
    key_prefix: String,
    hashed_key: String,
    scopes: Vec<String>,
    created_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    last_used_at: Option<DateTime<Utc>>,
}

impl ApiKeyRow {
    fn from_domain(api_key: &ApiKey) -> Self {
        Self {
            id: api_key.id.0,
            tenant_id: api_key.tenant_id.0,
            user_id: api_key.user_id.0,
            name: api_key.name.clone(),
            key_prefix: api_key.key_prefix.clone(),
            hashed_key: api_key.hashed_key.clone(),
            scopes: scopes_to_db(&api_key.scopes),
            created_at: api_key.created_at,
            expires_at: api_key.expires_at,
            last_used_at: api_key.last_used_at,
        }
    }

    fn into_domain(self) -> RepoResult<ApiKey> {
        Ok(ApiKey {
            id: ApiKeyId(self.id),
            tenant_id: TenantId(self.tenant_id),
            user_id: UserId(self.user_id),
            name: self.name,
            key_prefix: self.key_prefix,
            hashed_key: self.hashed_key,
            scopes: scopes_from_db(self.scopes)?,
            created_at: self.created_at,
            expires_at: self.expires_at,
            last_used_at: self.last_used_at,
        })
    }
}

const SELECT_COLUMNS: &str = "id, tenant_id, user_id, name, key_prefix, hashed_key, scopes, \
     created_at, expires_at, last_used_at";

#[async_trait]
impl ApiKeyRepository for PostgresApiKeyRepository {
    async fn create(&self, api_key: &ApiKey) -> RepoResult<()> {
        let row = ApiKeyRow::from_domain(api_key);
        sqlx::query(
            "INSERT INTO api_keys \
             (id, tenant_id, user_id, name, key_prefix, hashed_key, scopes, created_at, expires_at, last_used_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(row.id)
        .bind(row.tenant_id)
        .bind(row.user_id)
        .bind(row.name)
        .bind(row.key_prefix)
        .bind(row.hashed_key)
        .bind(row.scopes)
        .bind(row.created_at)
        .bind(row.expires_at)
        .bind(row.last_used_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_by_id(&self, id: ApiKeyId) -> RepoResult<Option<ApiKey>> {
        let row = sqlx::query_as::<_, ApiKeyRow>(&format!(
            "SELECT {SELECT_COLUMNS} FROM api_keys WHERE id = $1"
        ))
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await?;
        row.map(ApiKeyRow::into_domain).transpose()
    }

    async fn find_by_prefix(&self, key_prefix: &str) -> RepoResult<Option<ApiKey>> {
        let row = sqlx::query_as::<_, ApiKeyRow>(&format!(
            "SELECT {SELECT_COLUMNS} FROM api_keys WHERE key_prefix = $1"
        ))
        .bind(key_prefix)
        .fetch_optional(&self.pool)
        .await?;
        row.map(ApiKeyRow::into_domain).transpose()
    }

    async fn list_by_tenant(&self, tenant_id: TenantId) -> RepoResult<Vec<ApiKey>> {
        let rows = sqlx::query_as::<_, ApiKeyRow>(&format!(
            "SELECT {SELECT_COLUMNS} FROM api_keys WHERE tenant_id = $1 ORDER BY created_at DESC"
        ))
        .bind(tenant_id.0)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(ApiKeyRow::into_domain).collect()
    }

    async fn delete(&self, id: ApiKeyId) -> RepoResult<bool> {
        let result = sqlx::query("DELETE FROM api_keys WHERE id = $1")
            .bind(id.0)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn delete_for_tenant(&self, id: ApiKeyId, tenant_id: TenantId) -> RepoResult<bool> {
        let result = sqlx::query("DELETE FROM api_keys WHERE id = $1 AND tenant_id = $2")
            .bind(id.0)
            .bind(tenant_id.0)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn touch_last_used(&self, id: ApiKeyId, at: DateTime<Utc>) -> RepoResult<()> {
        sqlx::query("UPDATE api_keys SET last_used_at = $2 WHERE id = $1")
            .bind(id.0)
            .bind(at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_round_trip_through_db_string_form() {
        let scopes = vec![
            ApiKeyScope::MonitorRead,
            ApiKeyScope::MonitorWrite,
            ApiKeyScope::StatusPageRead,
        ];
        let db = scopes_to_db(&scopes);
        let back = scopes_from_db(db).unwrap();
        assert_eq!(back, scopes);
    }

    #[test]
    fn unknown_scope_string_is_rejected() {
        let err = scopes_from_db(vec!["monitor:delete".to_string()]);
        assert!(err.is_err());
    }
}
