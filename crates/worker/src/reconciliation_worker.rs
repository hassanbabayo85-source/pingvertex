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
        // Task 8F — PAYG agent billing retired; no metering.
        let agent_billing_repaired: i64 = 0;

        let started = Utc::now();
        let end = started;
        let start = end - Duration::hours(25);
        // Provider-side truth available to this codebase is the durable, signed webhook observation log.
        // A future live-provider adapter can replace/augment this query without changing report shape.
        let missing_payment: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM billing_audit_log b WHERE b.created_at >= $1 AND b.created_at < $2 AND b.event_type IN ('payment_succeeded','subscription_renewed') AND ((b.payload->>'payment_id') IS NULL OR NOT EXISTS (SELECT 1 FROM payments p WHERE p.tenant_id=b.tenant_id AND p.provider_payment_id=b.payload->>'payment_id'))"
        )
        .bind(start).bind(end).fetch_one(&self.pool).await?;
        let duplicate_payment: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM (SELECT provider, provider_payment_id FROM payments WHERE provider_payment_id IS NOT NULL AND created_at >= $1 AND created_at < $2 GROUP BY provider, provider_payment_id HAVING COUNT(*) > 1) d"
        )
        .bind(start).bind(end).fetch_one(&self.pool).await?;
        let incorrect_amount: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM billing_audit_log b JOIN payments p ON p.tenant_id=b.tenant_id AND p.provider_payment_id=b.payload->>'payment_id' WHERE b.created_at >= $1 AND b.created_at < $2 AND b.event_type IN ('payment_succeeded','subscription_renewed') AND ((NOT (b.payload ? 'amount_units')) OR NOT ((b.payload->>'amount_units') ~ '^[0-9]+$') OR (b.payload->>'amount_units')::numeric <> p.amount_units)"
        )
        .bind(start).bind(end).fetch_one(&self.pool).await?;
        let orphan_webhook: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM billing_audit_log b WHERE b.created_at >= $1 AND b.created_at < $2 AND (
                (b.event_type IN ('payment_succeeded','payment_failed','payment_refunded','subscription_renewed')
                 AND (b.payload->>'payment_id') IS NOT NULL
                 AND NOT EXISTS (SELECT 1 FROM payments p WHERE p.tenant_id=b.tenant_id AND p.provider_payment_id=b.payload->>'payment_id'))
                OR
                (b.event_type='subscription_canceled'
                 AND NOT EXISTS (SELECT 1 FROM subscriptions s WHERE s.tenant_id=b.tenant_id))
                OR
                ((b.payload->>'wallet_top_up') = 'true')
                 AND (b.payload->>'provider_checkout_id') IS NOT NULL
                 AND NOT EXISTS (SELECT 1 FROM wallet_top_up_sessions w WHERE w.tenant_id=b.tenant_id AND w.provider_checkout_id=b.payload->>'provider_checkout_id')
            )"
        )
        .bind(start).bind(end).fetch_one(&self.pool).await?;
        let wallet_mismatch: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_wallets w WHERE w.updated_at >= $1 AND w.updated_at < $2 AND (w.balance_units <> COALESCE((SELECT SUM(wt.amount_units) FROM wallet_transactions wt WHERE wt.wallet_id=w.id),0) OR EXISTS (SELECT 1 FROM wallet_top_up_sessions s WHERE s.tenant_id=w.tenant_id AND s.created_at >= $1 AND s.created_at < $2 AND s.status='COMPLETED' AND s.provider_payment_id IS NULL))"
        )
        .bind(start).bind(end).fetch_one(&self.pool).await?;
        let invoice_mismatch: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM invoices i WHERE i.created_at >= $1 AND i.created_at < $2 AND ((i.status='paid' AND i.paid_at IS NULL) OR (i.status <> 'paid' AND i.paid_at IS NOT NULL) OR i.total < 0 OR i.subtotal < 0 OR EXISTS (SELECT 1 FROM invoice_lines l WHERE l.invoice_id=i.id AND l.amount <> l.quantity*l.unit_price))"
        )
        .bind(start).bind(end).fetch_one(&self.pool).await?;
        // PAYG financial reconciliation. A crash in an older deployment could
        // leave a committed debit without its immutable usage/audit records.
        // New captures are atomic; this pass repairs legacy gaps using the
        // historical price effective at the debit timestamp.
        let payg_usage_repaired: i64 = sqlx::query_scalar::<sqlx::Postgres, i64>(
            r#"
            WITH candidates AS (
                SELECT r.id reservation_id, r.request_id, r.operation,
                       w.tenant_id, w.currency, -wt.amount_units amount_units,
                       wt.created_at
                FROM credit_reservations r
                JOIN user_wallets w ON w.id=r.wallet_id
                JOIN wallet_transactions wt
                  ON wt.reference_type='credit_reservation'
                 AND wt.reference_id=r.id::text
                 AND wt.transaction_type='DEBIT'
                LEFT JOIN api_usage_events e
                  ON e.tenant_id=w.tenant_id AND e.request_id=r.request_id
                WHERE r.status='CAPTURED' AND e.id IS NULL
                ORDER BY wt.created_at
                LIMIT 200
            ), priced AS (
                SELECT c.*, p.unit_price, p.version pricing_version
                FROM candidates c
                JOIN LATERAL (
                    SELECT unit_price, version
                    FROM payg_pricing_configs
                    WHERE operation=c.operation
                      AND effective_from <= c.created_at
                      AND (effective_until IS NULL OR effective_until > c.created_at)
                      AND unit_price > 0
                    ORDER BY effective_from DESC, version DESC
                    LIMIT 1
                ) p ON TRUE
            )
            INSERT INTO api_usage_events
                (id,tenant_id,operation,units_requested,units_billed,unit_price,
                 charge_units,currency,pricing_version,request_id,status,created_at)
            SELECT gen_random_uuid(), tenant_id, operation,
                   amount_units / unit_price, amount_units / unit_price,
                   unit_price, amount_units, currency, pricing_version,
                   request_id, 'CAPTURED', created_at
            FROM priced
            WHERE amount_units % unit_price = 0
            ON CONFLICT (tenant_id,request_id) DO NOTHING
            RETURNING 1
            "#,
        )
        .fetch_all(&self.pool)
        .await?
        .len() as i64;

        let payg_audit_repaired: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM (
                SELECT wt.id
                FROM credit_reservations r
                JOIN user_wallets w ON w.id=r.wallet_id
                JOIN wallet_transactions wt
                  ON wt.reference_type='credit_reservation'
                 AND wt.reference_id=r.id::text
                 AND wt.transaction_type='DEBIT'
                JOIN api_usage_events e
                  ON e.tenant_id=w.tenant_id AND e.request_id=r.request_id
                CROSS JOIN LATERAL (VALUES ('payg_charged'),('wallet_debited')) a(action)
                WHERE r.status='CAPTURED'
                  AND NOT EXISTS (
                      SELECT 1 FROM financial_audit_log f
                      WHERE f.action=a.action
                        AND f.resource='payg_request'
                        AND f.resource_id=r.request_id
                  )
            ) missing
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        // Insert missing audit rows after counting them. The reservation row is
        // immutable at this point, so the request id is the stable dedupe key.
        sqlx::query(
            r#"
            INSERT INTO financial_audit_log
                (id,actor,tenant_id,action,resource,resource_id,created_at,metadata)
            SELECT gen_random_uuid(), 'system:payg-reconciliation', w.tenant_id,
                   a.action, 'payg_request', r.request_id, now(),
                   jsonb_build_object('operation',r.operation,
                                      'charge_units',-wt.amount_units,
                                      'reconciled',true)
            FROM credit_reservations r
            JOIN user_wallets w ON w.id=r.wallet_id
            JOIN wallet_transactions wt
              ON wt.reference_type='credit_reservation'
             AND wt.reference_id=r.id::text
             AND wt.transaction_type='DEBIT'
            CROSS JOIN LATERAL (VALUES ('payg_charged'),('wallet_debited')) a(action)
            WHERE r.status='CAPTURED'
              AND EXISTS (
                  SELECT 1 FROM api_usage_events e
                  WHERE e.tenant_id=w.tenant_id AND e.request_id=r.request_id
              )
              AND NOT EXISTS (
                  SELECT 1 FROM financial_audit_log f
                  WHERE f.action=a.action
                    AND f.resource='payg_request'
                    AND f.resource_id=r.request_id
              )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // PAYG consistency checks. These are intentionally tenant-scoped and
        // keyed by the durable request id, which is the logical execution
        // identity shared by reservations, usage events and debit ledger rows.
        let execution_without_usage: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM proxy_check_jobs j
            WHERE EXISTS (
                SELECT 1 FROM proxy_entries pe
                WHERE pe.job_id=j.id AND pe.result_json IS NOT NULL
              )
              AND (
                    j.status IN ('completed','failed')
                    OR (j.status='running' AND j.heartbeat_at < now() - interval '5 minutes')
                  )
              AND NOT EXISTS (
                  SELECT 1 FROM api_usage_events e
                  WHERE e.tenant_id=j.tenant_id
                    AND e.request_id='proxy-check:' || j.id::text
              )
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let captured_reservation_without_usage: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM credit_reservations r
            JOIN user_wallets w ON w.id=r.wallet_id
            JOIN wallet_transactions wt
              ON wt.reference_type='credit_reservation'
             AND wt.reference_id=r.id::text
             AND wt.transaction_type='DEBIT'
            LEFT JOIN api_usage_events e
              ON e.tenant_id=w.tenant_id AND e.request_id=r.request_id
            WHERE r.status='CAPTURED' AND e.id IS NULL
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let chargeable_usage_without_debit: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM api_usage_events e
            LEFT JOIN credit_reservations r
              ON r.request_id=e.request_id
             AND EXISTS (
                 SELECT 1 FROM user_wallets w
                 WHERE w.id=r.wallet_id AND w.tenant_id=e.tenant_id
             )
            LEFT JOIN wallet_transactions wt
              ON wt.reference_type='credit_reservation'
             AND wt.reference_id=r.id::text
             AND wt.transaction_type='DEBIT'
            WHERE e.status='CAPTURED'
              AND e.charge_units > 0
              AND wt.id IS NULL
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let duplicate_usage_events: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM (
                SELECT tenant_id, request_id
                FROM api_usage_events
                GROUP BY tenant_id, request_id
                HAVING COUNT(*) > 1
            ) d
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let duplicate_payg_charges: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM (
                SELECT w.tenant_id, r.request_id
                FROM credit_reservations r
                JOIN user_wallets w ON w.id=r.wallet_id
                JOIN wallet_transactions wt
                  ON wt.reference_type='credit_reservation'
                 AND wt.reference_id=r.id::text
                 AND wt.transaction_type='DEBIT'
                GROUP BY w.tenant_id, r.request_id
                HAVING COUNT(*) > 1
            ) d
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let pending_reservations_past_expiry: i64 = sqlx::query_scalar::<sqlx::Postgres, i64>(
            r#"
            WITH expired AS (
                SELECT id
                FROM credit_reservations
                WHERE status='PENDING' AND expires_at <= now()
                ORDER BY expires_at
                LIMIT 1000
                FOR UPDATE SKIP LOCKED
            )
            UPDATE credit_reservations r
            SET status='EXPIRED', updated_at=now()
            FROM expired
            WHERE r.id=expired.id
            RETURNING 1
            "#,
        )
        .fetch_all(&self.pool)
        .await?
        .len() as i64;

        // Compare the dashboard's charge source of truth (usage events) with
        // PAYG debit ledger totals per tenant. Non-PAYG debit transactions are
        // deliberately excluded.
        let ledger_report_mismatch: i64 = sqlx::query_scalar(
            r#"
            WITH usage AS (
                SELECT tenant_id, COALESCE(SUM(charge_units),0)::BIGINT total
                FROM api_usage_events
                WHERE status='CAPTURED' AND charge_units > 0
                GROUP BY tenant_id
            ),
            ledger AS (
                SELECT w.tenant_id,
                       COALESCE(SUM(-wt.amount_units),0)::BIGINT total
                FROM wallet_transactions wt
                JOIN user_wallets w ON w.id=wt.wallet_id
                JOIN credit_reservations r
                  ON r.id::text=wt.reference_id
                 AND r.wallet_id=w.id
                WHERE wt.transaction_type='DEBIT'
                  AND wt.reference_type='credit_reservation'
                GROUP BY w.tenant_id
            ),
            tenants_seen AS (
                SELECT tenant_id FROM usage
                UNION
                SELECT tenant_id FROM ledger
            )
            SELECT COUNT(*)
            FROM tenants_seen t
            LEFT JOIN usage u ON u.tenant_id=t.tenant_id
            LEFT JOIN ledger l ON l.tenant_id=t.tenant_id
            WHERE COALESCE(u.total,0) <> COALESCE(l.total,0)
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        // Keep the existing legacy repair, but only repair immutable financial
        // gaps where a captured debit already proves the charge happened.
        let report = serde_json::json!({
            "provider_source":"signed_billing_webhook_observations",
            "window_start":start,
            "window_end":end,
            "missing_payment":missing_payment,
            "duplicate_payment":duplicate_payment,
            "incorrect_amount":incorrect_amount,
            "orphan_webhook":orphan_webhook,
            "wallet_mismatch":wallet_mismatch,
            "invoice_mismatch":invoice_mismatch,
            "payg_execution_without_usage":execution_without_usage,
            "payg_captured_reservation_without_usage":captured_reservation_without_usage,
            "payg_chargeable_usage_without_debit":chargeable_usage_without_debit,
            "payg_duplicate_usage_events":duplicate_usage_events,
            "payg_duplicate_charges":duplicate_payg_charges,
            "payg_pending_reservations_expired":pending_reservations_past_expiry,
            "payg_ledger_report_mismatch":ledger_report_mismatch,
            "payg_usage_repaired":payg_usage_repaired,
            "payg_agent_billing_repaired":agent_billing_repaired,
            "payg_audit_missing_before_repair":payg_audit_repaired
        });
        let total = missing_payment
            + duplicate_payment
            + incorrect_amount
            + orphan_webhook
            + wallet_mismatch
            + invoice_mismatch
            + execution_without_usage
            + captured_reservation_without_usage
            + chargeable_usage_without_debit
            + duplicate_usage_events
            + duplicate_payg_charges
            + ledger_report_mismatch;
        for (kind, count) in [
            ("missing_payment", missing_payment),
            ("duplicate_payment", duplicate_payment),
            ("incorrect_amount", incorrect_amount),
            ("orphan_webhook", orphan_webhook),
            ("wallet_mismatch", wallet_mismatch),
            ("invoice_mismatch", invoice_mismatch),
            ("payg_execution_without_usage", execution_without_usage),
            (
                "payg_captured_reservation_without_usage",
                captured_reservation_without_usage,
            ),
            (
                "payg_chargeable_usage_without_debit",
                chargeable_usage_without_debit,
            ),
            ("payg_duplicate_usage_events", duplicate_usage_events),
            ("payg_duplicate_charges", duplicate_payg_charges),
            (
                "payg_pending_reservations_expired",
                pending_reservations_past_expiry,
            ),
            ("payg_ledger_report_mismatch", ledger_report_mismatch),
        ] {
            if count > 0 {
                metrics::counter!("billing_reconciliation_mismatches_total", "kind" => kind)
                    .increment(count as u64);
            }
        }
        metrics::counter!("payg_reconciliation_runs_total").increment(1);
        metrics::counter!("payg_reconciliation_anomalies_total").increment(total.max(0) as u64);
        metrics::gauge!("billing_reconciliation_last_status").set(if total == 0 {
            0.0
        } else {
            1.0
        });
        let status = if total == 0 { "ok" } else { "mismatch" };
        sqlx::query("INSERT INTO billing_reconciliation_reports(id,run_started_at,run_finished_at,window_start,window_end,status,mismatch_counts,report) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
   .bind(Uuid::new_v4()).bind(started).bind(Utc::now()).bind(start).bind(end).bind(status)
   .bind(serde_json::json!({
       "missing_payment":missing_payment,
       "duplicate_payment":duplicate_payment,
       "incorrect_amount":incorrect_amount,
       "orphan_webhook":orphan_webhook,
       "wallet_mismatch":wallet_mismatch,
       "invoice_mismatch":invoice_mismatch,
       "payg_execution_without_usage":execution_without_usage,
       "payg_captured_reservation_without_usage":captured_reservation_without_usage,
       "payg_chargeable_usage_without_debit":chargeable_usage_without_debit,
       "payg_duplicate_usage_events":duplicate_usage_events,
       "payg_duplicate_charges":duplicate_payg_charges,
       "payg_pending_reservations_expired":pending_reservations_past_expiry,
       "payg_ledger_report_mismatch":ledger_report_mismatch
   }))
   .bind(&report).execute(&self.pool).await?;
        if total > 0 {
            let _ = sqlx::query(
                "INSERT INTO financial_audit_log(id,actor,tenant_id,action,resource,resource_id,created_at,metadata) VALUES($1,'system:reconciliation',NULL,'reconciliation_anomaly','reconciliation_report',$2,now(),$3)"
            )
            .bind(Uuid::new_v4())
            .bind(report.get("window_end").and_then(|v| v.as_str()).unwrap_or("unknown"))
            .bind(report.clone())
            .execute(&self.pool)
            .await;
            tracing::warn!(
                missing_payment,
                duplicate_payment,
                incorrect_amount,
                orphan_webhook,
                wallet_mismatch,
                invoice_mismatch,
                execution_without_usage,
                captured_reservation_without_usage,
                chargeable_usage_without_debit,
                duplicate_usage_events,
                duplicate_payg_charges,
                pending_reservations_past_expiry,
                ledger_report_mismatch,
                "billing reconciliation mismatch detected"
            );
        } else {
            info!("billing reconciliation OK");
        }
        Ok(report)
    }
}
