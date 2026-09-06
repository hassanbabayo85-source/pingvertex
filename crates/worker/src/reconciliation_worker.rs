use chrono::{Duration, Utc};
use sqlx::PgPool;
use std::time::Duration as StdDuration;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use uuid::Uuid;

#[derive(Clone)]
pub struct ReconciliationWorker {
    pool: PgPool,
    interval: StdDuration,
}
impl ReconciliationWorker {
    pub fn new(pool: PgPool, interval: StdDuration) -> Self {
        Self { pool, interval }
    }
    pub async fn run_forever(
        self,
        cancel: CancellationToken,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.run_once().await?;
        let mut tick = tokio::time::interval(self.interval);
        loop {
            tokio::select! {_=cancel.cancelled()=>break,_=tick.tick()=>{if let Err(e)=self.run_once().await{error!(error=%e,"billing reconciliation failed");}}}
        }
        Ok(())
    }
    pub async fn run_once(
        &self,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        let started = Utc::now();
        let end = started;
        let start = end - Duration::hours(25);
        // Provider-side truth available to this codebase is the durable, signed webhook observation log.
        // A future live-provider adapter can replace/augment this query without changing report shape.
        let missing_payment: i64=sqlx::query_scalar("SELECT COUNT(*) FROM billing_audit_log b WHERE b.created_at >= $1 AND b.created_at < $2 AND b.event_type='payment_succeeded' AND NOT EXISTS (SELECT 1 FROM payments p WHERE p.tenant_id=b.tenant_id AND p.created_at BETWEEN $1 AND $2 AND p.status IN ('succeeded','paid','completed'))").bind(start).bind(end).fetch_one(&self.pool).await?;
        let duplicate_payment:i64=sqlx::query_scalar("SELECT COUNT(*) FROM (SELECT provider,provider_payment_id FROM payments WHERE provider_payment_id IS NOT NULL AND created_at >= $1 AND created_at < $2 GROUP BY provider,provider_payment_id HAVING COUNT(*)>1) d").bind(start).bind(end).fetch_one(&self.pool).await?;
        let incorrect_amount:i64=sqlx::query_scalar("SELECT COUNT(*) FROM billing_audit_log b JOIN payments p ON p.tenant_id=b.tenant_id AND p.provider_payment_id=(b.payload->>'payment_id') WHERE b.created_at >= $1 AND b.created_at < $2 AND b.event_type='payment_succeeded' AND b.payload ? 'amount_units' AND (b.payload->>'amount_units')::bigint <> p.amount_units").bind(start).bind(end).fetch_one(&self.pool).await?;
        let orphan_webhook:i64=sqlx::query_scalar("SELECT COUNT(*) FROM billing_audit_log b WHERE b.created_at >= $1 AND b.created_at < $2 AND NOT EXISTS (SELECT 1 FROM subscriptions s WHERE s.tenant_id=b.tenant_id) AND NOT EXISTS (SELECT 1 FROM wallet_top_up_sessions w WHERE w.tenant_id=b.tenant_id AND w.provider_checkout_id=(b.payload->>'provider_checkout_id'))").bind(start).bind(end).fetch_one(&self.pool).await?;
        let wallet_mismatch:i64=sqlx::query_scalar("SELECT COUNT(*) FROM user_wallets w WHERE w.updated_at >= $1 AND w.updated_at < $2 AND (w.balance_units <> COALESCE((SELECT SUM(wt.amount_units) FROM wallet_transactions wt WHERE wt.wallet_id=w.id),0) OR EXISTS (SELECT 1 FROM wallet_top_up_sessions s WHERE s.tenant_id=w.tenant_id AND s.created_at >= $1 AND s.created_at < $2 AND s.status='COMPLETED' AND s.provider_payment_id IS NULL))").bind(start).bind(end).fetch_one(&self.pool).await?;
        let report = serde_json::json!({"provider_source":"signed_billing_webhook_observations","window_start":start,"window_end":end,"missing_payment":missing_payment,"duplicate_payment":duplicate_payment,"incorrect_amount":incorrect_amount,"orphan_webhook":orphan_webhook,"wallet_mismatch":wallet_mismatch});
        let total = missing_payment
            + duplicate_payment
            + incorrect_amount
            + orphan_webhook
            + wallet_mismatch;
        for (kind, count) in [
            ("missing_payment", missing_payment),
            ("duplicate_payment", duplicate_payment),
            ("incorrect_amount", incorrect_amount),
            ("orphan_webhook", orphan_webhook),
            ("wallet_mismatch", wallet_mismatch),
        ] {
            if count > 0 {
                metrics::counter!("billing_reconciliation_mismatches_total", "kind" => kind)
                    .increment(count as u64);
            }
        }
        metrics::gauge!("billing_reconciliation_last_status").set(if total == 0 {
            0.0
        } else {
            1.0
        });
        let status = if total == 0 { "ok" } else { "mismatch" };
        sqlx::query("INSERT INTO billing_reconciliation_reports(id,run_started_at,run_finished_at,window_start,window_end,status,mismatch_counts,report) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
   .bind(Uuid::new_v4()).bind(started).bind(Utc::now()).bind(start).bind(end).bind(status)
   .bind(serde_json::json!({"missing_payment":missing_payment,"duplicate_payment":duplicate_payment,"incorrect_amount":incorrect_amount,"orphan_webhook":orphan_webhook,"wallet_mismatch":wallet_mismatch}))
   .bind(&report).execute(&self.pool).await?;
        if total > 0 {
            tracing::warn!(
                missing_payment,
                duplicate_payment,
                incorrect_amount,
                orphan_webhook,
                wallet_mismatch,
                "billing reconciliation mismatch detected"
            );
        } else {
            info!("billing reconciliation OK");
        }
        Ok(report)
    }
}
