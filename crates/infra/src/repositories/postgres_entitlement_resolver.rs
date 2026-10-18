//! PostgreSQL-backed `EntitlementResolver`.
//!
//! Resolves a tenant's current `Entitlements` snapshot in two queries:
//!
//!   1. Look up the tenant's subscription row. If none exists, or if
//!      the row's status is `expired` / `canceled`, treat the tenant as
//!      `Free`. Otherwise use the row's `plan_tier`.
//!   2. Load the latest `plan_entitlements` version for that plan,
//!      and hand the rows to `Entitlements::from_rows`.
//!
//! This is the **only** production implementation of the resolver.
//! No caching: any cache would risk serving a stale snapshot to a
//! race-sensitive quota check. If that ever becomes a measured
//! bottleneck, the cache must invalidate on every subscription and
//! plan_entitlements write, not just expire on time.

use std::error::Error;

use application::ports::{EntitlementResolver, RepoResult};
use async_trait::async_trait;
use domain::{
    EntitlementResolutionError, Entitlements, PlanTier, entities::PlanEntitlement,
    shared::ids::TenantId,
};
use sqlx::{FromRow, PgPool};

/// PostgreSQL-backed `EntitlementResolver`.
#[derive(Debug, Clone)]
pub struct PostgresEntitlementResolver {
    pool: PgPool,
}

impl PostgresEntitlementResolver {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, FromRow)]
struct SubscriptionRow {
    plan_tier: String,
    status: String,
}

#[derive(Debug, FromRow)]
struct EntitlementRow {
    id: uuid::Uuid,
    plan_id: uuid::Uuid,
    key: String,
    limit_value: i64,
    version: i64,
    effective_from: chrono::DateTime<chrono::Utc>,
    created_at: chrono::DateTime<chrono::Utc>,
}

/// Tiny helper so the explicit `.map_err(boxed)` calls read cleanly.
fn boxed<E: Error + Send + Sync + 'static>(e: E) -> Box<dyn Error + Send + Sync> {
    Box::new(e)
}

#[async_trait]
impl EntitlementResolver for PostgresEntitlementResolver {
    async fn resolve(&self, tenant_id: TenantId) -> RepoResult<Entitlements> {
        // Step 1: determine the effective tier.
        let effective_tier: PlanTier = match sqlx::query_as::<_, SubscriptionRow>(
            "SELECT plan_tier, status FROM subscriptions WHERE tenant_id = $1",
        )
        .bind(tenant_id.0)
        .fetch_optional(&self.pool)
        .await?
        {
            None => PlanTier::Free,
            Some(row) => match row.status.as_str() {
                "expired" | "canceled" => PlanTier::Free,
                _ => row.plan_tier.parse::<PlanTier>().map_err(boxed)?,
            },
        };

        // Step 2: find the plan id for that tier. Migration 0125 seeds
        // every tier (Free..=Enterprise), so a missing row here means
        // the database is not what the migration left it as; fail loudly
        // rather than fall back to some silent default.
        let plan_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM plans WHERE tier = $1")
            .bind(effective_tier.as_str())
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| boxed(EntitlementResolutionError::Empty))?;

        // Step 3: load the latest version's entitlement rows.
        let rows: Vec<EntitlementRow> = sqlx::query_as::<_, EntitlementRow>(
            r#"
            SELECT id, plan_id, key, limit_value, version, effective_from, created_at
            FROM plan_entitlements
            WHERE plan_id = $1
              AND version = (SELECT MAX(version) FROM plan_entitlements WHERE plan_id = $1)
            "#,
        )
        .bind(plan_id)
        .fetch_all(&self.pool)
        .await?;

        // Rows come from our own migration, so they already satisfy
        // `PlanEntitlement`'s field-level invariants. Rebuild the domain
        // type with a struct literal (rather than `new()`, which would
        // re-validate each row and reject negative limits before
        // `Entitlements::from_rows` gets a chance to report them with
        // quota-specific context).
        let domain_rows: Vec<PlanEntitlement> = rows
            .into_iter()
            .map(|r| PlanEntitlement {
                id: r.id,
                plan_id: r.plan_id,
                key: r.key,
                limit_value: r.limit_value,
                version: r.version,
                effective_from: r.effective_from,
                created_at: r.created_at,
            })
            .collect();

        Entitlements::from_rows(effective_tier, &domain_rows).map_err(boxed)
    }
}
