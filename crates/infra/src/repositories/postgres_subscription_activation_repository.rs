//! BE-1 F5 — atomic subscription activation (PostgreSQL).
//!
//! Implements `SubscriptionActivationRepository` as a single
//! `sqlx::Transaction` covering billing-record insert-or-lock,
//! subscription create-or-update, billing-record success marking, and
//! checkout-session completion. Every early-return error drops `tx`
//! without calling `commit()`, so sqlx rolls the transaction back
//! automatically.
//!
//! Errors returned from the SQL calls map to
//! `SubscriptionActivationError::Database(...)` (server-side). Business
//! invariant failures map to specific client-facing variants — see the
//! `subscription_activation_repository` module docs.

use application::ports::subscription_activation_repository::{
    ActivationOutcome, SubscriptionAction, SubscriptionActivationError,
    SubscriptionActivationRepository, SubscriptionActivationResult,
};
use application::ports::subscription_billing_record_repository::SubscriptionBillingRecord;
use async_trait::async_trait;
use domain::shared::ids::CheckoutSessionId;
use sqlx::{FromRow, PgConnection, PgPool};
use std::error::Error;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PostgresSubscriptionActivationRepository {
    pool: PgPool,
}

impl PostgresSubscriptionActivationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Wrap an infrastructure error (sqlx, boxed Error, etc.) in the
/// `Database(...)` server-side variant.
fn db<E: Error + Send + Sync + 'static>(e: E) -> SubscriptionActivationError {
    SubscriptionActivationError::Database(Box::<dyn Error + Send + Sync>::from(e))
}

#[derive(Debug, Clone, FromRow)]
struct ExistingBillingRecord {
    subscription_id: Uuid,
    tenant_id: Uuid,
    plan_tier: String,
    billing_interval: String,
    currency: String,
    amount_minor: i64,
    status: String,
}

#[derive(Debug, Clone, FromRow)]
struct SubscriptionStatusRow {
    tenant_id: Uuid,
    status: String,
}

#[derive(Debug, Clone, FromRow)]
struct ConcurrentWinnerRow {
    id: Uuid,
    tenant_id: Uuid,
    plan_tier: String,
    status: String,
}

#[async_trait]
impl SubscriptionActivationRepository for PostgresSubscriptionActivationRepository {
    async fn activate_atomically(
        &self,
        record: SubscriptionBillingRecord,
        action: SubscriptionAction,
        session_id: CheckoutSessionId,
    ) -> SubscriptionActivationResult<ActivationOutcome> {
        let mut tx = self.pool.begin().await.map_err(db)?;

        // ── Step 1 — insert billing record with status='pending' (idempotent) ──
        let inserted_row = sqlx::query(
            "INSERT INTO subscription_billing_records \
             (id, subscription_id, tenant_id, plan_tier, billing_interval, currency, \
              amount_minor, status, provider, provider_reference, provider_payment_id, \
              provider_subscription_code, period_start, period_end, created_at, updated_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16) \
             ON CONFLICT (provider, provider_reference) DO NOTHING \
             RETURNING id",
        )
        .bind(record.id)
        .bind(record.subscription_id)
        .bind(record.tenant_id)
        .bind(record.plan_tier.as_str())
        .bind(record.billing_interval.as_str())
        .bind(record.currency.as_str())
        .bind(record.amount_minor)
        .bind("pending")
        .bind(&record.provider)
        .bind(&record.provider_reference)
        .bind(&record.provider_payment_id)
        .bind(&record.provider_subscription_code)
        .bind(record.period_start)
        .bind(record.period_end)
        .bind(record.created_at)
        .bind(record.updated_at)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;

        let record_was_inserted = inserted_row.is_some();

        // ── Step 2 — if not inserted, lock and load the existing row ──
        let existing: Option<ExistingBillingRecord> = if record_was_inserted {
            None
        } else {
            let row = sqlx::query_as::<_, ExistingBillingRecord>(
                "SELECT subscription_id, tenant_id, plan_tier, billing_interval, \
                        currency, amount_minor, status \
                 FROM subscription_billing_records \
                 WHERE provider = $1 AND provider_reference = $2 \
                 FOR UPDATE",
            )
            .bind(&record.provider)
            .bind(&record.provider_reference)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or(SubscriptionActivationError::BillingRecordMissingAfterConflict)?;
            Some(row)
        };

        // ── Step 3 — validate identity invariants against the request ──
        if let Some(existing) = &existing {
            if existing.tenant_id != record.tenant_id {
                return Err(SubscriptionActivationError::BillingRecordTenantMismatch);
            }
            if existing.plan_tier != record.plan_tier.as_str() {
                return Err(SubscriptionActivationError::BillingRecordPlanTierMismatch);
            }
            if existing.billing_interval != record.billing_interval.as_str() {
                return Err(SubscriptionActivationError::BillingRecordIntervalMismatch);
            }
            if existing.currency != record.currency.as_str() {
                return Err(SubscriptionActivationError::BillingRecordCurrencyMismatch);
            }
            if existing.amount_minor != record.amount_minor {
                return Err(SubscriptionActivationError::BillingRecordAmountMismatch);
            }
        }

        let existing_status = existing.as_ref().map(|e| e.status.as_str());

        // ── Step 3a — Failed → do not reactivate ──
        if existing_status == Some("failed") {
            return Err(SubscriptionActivationError::BillingRecordFailed);
        }

        // ── Step 3b — Succeeded → strengthen validation, then complete idempotently ──
        if existing_status == Some("succeeded") {
            let existing = existing.as_ref().expect("checked above");
            let subscription_id = existing.subscription_id;

            let sub = sqlx::query_as::<_, SubscriptionStatusRow>(
                "SELECT tenant_id, status FROM subscriptions WHERE id = $1",
            )
            .bind(subscription_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;

            let sub = sub.ok_or(SubscriptionActivationError::SucceededSubscriptionMissing)?;

            if sub.tenant_id != record.tenant_id {
                return Err(SubscriptionActivationError::SucceededSubscriptionTenantMismatch);
            }
            if sub.status == "canceled" || sub.status == "expired" {
                return Err(SubscriptionActivationError::SucceededSubscriptionTerminal);
            }

            complete_session(
                &mut *tx,
                session_id,
                record.tenant_id,
                record.provider_payment_id.as_deref(),
            )
            .await?;

            tx.commit().await.map_err(db)?;
            return Ok(ActivationOutcome::AlreadySucceeded { subscription_id });
        }

        if let Some(status) = existing_status
            && status != "pending"
        {
            return Err(SubscriptionActivationError::BillingRecordStatusUnknown(
                status.to_owned(),
            ));
        }

        // ── Step 4 — persist subscription ──
        let (final_subscription_id, outcome) = match action {
            SubscriptionAction::Update(sub) => {
                let result = sqlx::query(
                    "UPDATE subscriptions SET \
                     plan_tier = $2, status = $3, trial_ends_at = $4, grace_period_ends_at = $5, \
                     current_period_ends_at = $6, canceled_at = $7, cancel_at_period_end = $8, \
                     monitors = $9, interval_seconds = $10, retention_days = $11, \
                     pricing_version = $12, updated_at = $13 \
                     WHERE id = $1 AND tenant_id = $14 AND status <> 'canceled'",
                )
                .bind(sub.id.0)
                .bind(sub.plan_tier.as_str())
                .bind(sub.status.as_str())
                .bind(sub.trial_ends_at)
                .bind(sub.grace_period_ends_at)
                .bind(sub.current_period_ends_at)
                .bind(sub.canceled_at)
                .bind(sub.cancel_at_period_end)
                .bind(sub.monitors.map(|m| m as i64))
                .bind(sub.interval_seconds.map(|v| v as i32))
                .bind(sub.retention_days.map(|v| v as i32))
                .bind(sub.pricing_version.map(|v| v as i32))
                .bind(sub.updated_at)
                .bind(sub.tenant_id.0)
                .execute(&mut *tx)
                .await
                .map_err(db)?;

                if result.rows_affected() != 1 {
                    return Err(SubscriptionActivationError::SubscriptionUpdateMatchedNoRows);
                }

                (
                    sub.id.0,
                    ActivationOutcome::Updated {
                        subscription_id: sub.id.0,
                    },
                )
            }
            SubscriptionAction::Create(sub) => {
                let inserted = sqlx::query(
                    "INSERT INTO subscriptions \
                     (id, tenant_id, plan_tier, status, trial_ends_at, grace_period_ends_at, \
                      current_period_ends_at, canceled_at, cancel_at_period_end, monitors, \
                      interval_seconds, retention_days, pricing_version, created_at, updated_at) \
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15) \
                     ON CONFLICT (tenant_id) DO NOTHING \
                     RETURNING id",
                )
                .bind(sub.id.0)
                .bind(sub.tenant_id.0)
                .bind(sub.plan_tier.as_str())
                .bind(sub.status.as_str())
                .bind(sub.trial_ends_at)
                .bind(sub.grace_period_ends_at)
                .bind(sub.current_period_ends_at)
                .bind(sub.canceled_at)
                .bind(sub.cancel_at_period_end)
                .bind(sub.monitors.map(|m| m as i64))
                .bind(sub.interval_seconds.map(|v| v as i32))
                .bind(sub.retention_days.map(|v| v as i32))
                .bind(sub.pricing_version.map(|v| v as i32))
                .bind(sub.created_at)
                .bind(sub.updated_at)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;

                match inserted {
                    Some(row) => {
                        use sqlx::Row;
                        let new_id: Uuid = row.get("id");
                        (
                            new_id,
                            ActivationOutcome::Created {
                                subscription_id: new_id,
                            },
                        )
                    }
                    None => {
                        let winner = sqlx::query_as::<_, ConcurrentWinnerRow>(
                            "SELECT id, tenant_id, plan_tier, status \
                             FROM subscriptions WHERE tenant_id = $1 FOR UPDATE",
                        )
                        .bind(sub.tenant_id.0)
                        .fetch_one(&mut *tx)
                        .await
                        .map_err(db)?;

                        if winner.tenant_id != sub.tenant_id.0 {
                            return Err(
                                SubscriptionActivationError::ConcurrentWinnerTenantMismatch,
                            );
                        }
                        if winner.plan_tier != sub.plan_tier.as_str() {
                            return Err(SubscriptionActivationError::ConcurrentWinnerPlanMismatch);
                        }
                        if winner.status == "canceled" || winner.status == "expired" {
                            return Err(SubscriptionActivationError::ConcurrentWinnerTerminal);
                        }

                        (
                            winner.id,
                            ActivationOutcome::ConcurrentWinnerAccepted {
                                subscription_id: winner.id,
                            },
                        )
                    }
                }
            }
        };

        // ── Step 4b — sync record's subscription_id to the actual persisted row ──
        if !record_was_inserted || final_subscription_id != record.subscription_id {
            let sync = sqlx::query(
                "UPDATE subscription_billing_records \
                 SET subscription_id = $1, updated_at = now() \
                 WHERE provider = $2 AND provider_reference = $3",
            )
            .bind(final_subscription_id)
            .bind(&record.provider)
            .bind(&record.provider_reference)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
            if sync.rows_affected() != 1 {
                return Err(SubscriptionActivationError::BillingRecordLinkFailed);
            }
        }

        // ── Step 5 — mark billing record succeeded ──
        let mark = sqlx::query(
            "UPDATE subscription_billing_records \
             SET status = 'succeeded', provider_payment_id = $1, updated_at = now() \
             WHERE provider = $2 AND provider_reference = $3",
        )
        .bind(record.provider_payment_id.as_deref())
        .bind(&record.provider)
        .bind(&record.provider_reference)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        if mark.rows_affected() != 1 {
            return Err(SubscriptionActivationError::BillingRecordMarkSucceededFailed);
        }

        // ── Step 6 — complete checkout session ──
        complete_session(
            &mut *tx,
            session_id,
            record.tenant_id,
            record.provider_payment_id.as_deref(),
        )
        .await?;

        tx.commit().await.map_err(db)?;

        Ok(outcome)
    }
}

/// Step 6 helper — atomically mark a checkout session completed.
async fn complete_session(
    conn: &mut PgConnection,
    session_id: CheckoutSessionId,
    tenant_id: Uuid,
    provider_payment_id: Option<&str>,
) -> SubscriptionActivationResult<()> {
    let result = sqlx::query(
        "UPDATE checkout_sessions \
         SET status = 'completed', provider_payment_id = $1, updated_at = now() \
         WHERE id = $2 AND tenant_id = $3 AND status <> 'failed'",
    )
    .bind(provider_payment_id)
    .bind(session_id.0)
    .bind(tenant_id)
    .execute(conn)
    .await
    .map_err(db)?;

    if result.rows_affected() != 1 {
        return Err(SubscriptionActivationError::CheckoutSessionUpdateRejected);
    }

    Ok(())
}
