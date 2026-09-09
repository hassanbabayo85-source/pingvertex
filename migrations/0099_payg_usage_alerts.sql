BEGIN;

ALTER TABLE wallet_settings
    ADD COLUMN IF NOT EXISTS daily_usage_alert_threshold_units BIGINT,
    ADD COLUMN IF NOT EXISTS monthly_usage_alert_threshold_units BIGINT;

ALTER TABLE wallet_settings DROP CONSTRAINT IF EXISTS chk_wallet_daily_usage_alert_threshold;
ALTER TABLE wallet_settings ADD CONSTRAINT chk_wallet_daily_usage_alert_threshold
    CHECK (daily_usage_alert_threshold_units IS NULL OR daily_usage_alert_threshold_units > 0);

ALTER TABLE wallet_settings DROP CONSTRAINT IF EXISTS chk_wallet_monthly_usage_alert_threshold;
ALTER TABLE wallet_settings ADD CONSTRAINT chk_wallet_monthly_usage_alert_threshold
    CHECK (monthly_usage_alert_threshold_units IS NULL OR monthly_usage_alert_threshold_units > 0);

ALTER TABLE wallet_settings DROP CONSTRAINT IF EXISTS chk_wallet_usage_alert_thresholds;
ALTER TABLE wallet_settings ADD CONSTRAINT chk_wallet_usage_alert_thresholds
    CHECK (
        (daily_usage_alert_threshold_units IS NULL OR monthly_usage_alert_threshold_units IS NULL)
        OR monthly_usage_alert_threshold_units >= daily_usage_alert_threshold_units
    );

CREATE INDEX IF NOT EXISTS idx_api_usage_events_tenant_status_created
    ON api_usage_events(tenant_id, status, created_at DESC);

COMMIT;
