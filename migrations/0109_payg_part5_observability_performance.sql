BEGIN;

-- PAYG Blueprint Part 5: observability/performance release hardening.
-- Forward-only: never mutate historical wallet/usage/audit rows.

-- Stable tenant-scoped monitor ordering for large monitor lists.
CREATE INDEX IF NOT EXISTS idx_monitors_tenant_created_id
    ON monitors (tenant_id, created_at DESC, id DESC);

-- Bulk job lookup/claim paths.
CREATE INDEX IF NOT EXISTS idx_monitor_bulk_action_jobs_status_created
    ON monitor_bulk_action_jobs (status, created_at, id);

CREATE INDEX IF NOT EXISTS idx_monitor_bulk_import_jobs_status_updated
    ON monitor_bulk_import_jobs (status, updated_at, id);

CREATE INDEX IF NOT EXISTS idx_monitor_bulk_import_rows_job_status_row
    ON monitor_bulk_import_rows (job_id, validation_status, row_number);

-- Usage reporting/reconciliation remains tenant scoped and time ordered.
CREATE INDEX IF NOT EXISTS idx_api_usage_events_tenant_created_id
    ON api_usage_events (tenant_id, created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_wallet_transactions_wallet_created_id
    ON wallet_transactions (wallet_id, created_at DESC, id DESC);

-- Enforce the product-authoritative Free tier for the latest active pricing
-- configuration without rewriting historical pricing versions.
DO $$
DECLARE
    latest RECORD;
    next_version INTEGER;
BEGIN
    SELECT *
      INTO latest
      FROM pricing_configs
     WHERE effective_at <= now()
     ORDER BY version DESC
     LIMIT 1;

    IF FOUND
       AND (latest.free_monitors <> 10 OR latest.free_interval_seconds <> 300)
    THEN
        SELECT COALESCE(MAX(version), 0) + 1
          INTO next_version
          FROM pricing_configs;

        INSERT INTO pricing_configs (
            id, version, currency, cost_per_check, effective_storage_bytes_per_check,
            cost_per_gb_hot, cost_per_gb_warm, cost_per_gb_cold, backup_multiplier,
            markup_multiplier, min_interval_seconds, max_monitors, max_retention_days,
            min_paid_monthly_price, free_monitors, free_interval_seconds,
            free_retention_days, avg_billable_network_bytes_per_check,
            cost_per_gb_egress, cost_safety_buffer, sms_markup_multiplier,
            enterprise_sla_monthly_price, effective_at, created_by
        )
        VALUES (
            gen_random_uuid(), next_version, latest.currency, latest.cost_per_check,
            latest.effective_storage_bytes_per_check, latest.cost_per_gb_hot,
            latest.cost_per_gb_warm, latest.cost_per_gb_cold, latest.backup_multiplier,
            latest.markup_multiplier, latest.min_interval_seconds, latest.max_monitors,
            latest.max_retention_days, latest.min_paid_monthly_price, 10, 300,
            latest.free_retention_days, latest.avg_billable_network_bytes_per_check,
            latest.cost_per_gb_egress, latest.cost_safety_buffer,
            latest.sms_markup_multiplier, latest.enterprise_sla_monthly_price,
            now(), latest.created_by
        );
    END IF;
END $$;

COMMIT;
