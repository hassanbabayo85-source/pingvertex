BEGIN;

-- Explicit Individual/Company account model. Billing logic remains shared;
-- account type is descriptive/organizational metadata, never a pricing tier.
ALTER TABLE tenants
    ADD COLUMN IF NOT EXISTS account_type TEXT NOT NULL DEFAULT 'individual';

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'chk_tenants_account_type'
          AND conrelid = 'tenants'::regclass
    ) THEN
        ALTER TABLE tenants
            ADD CONSTRAINT chk_tenants_account_type
            CHECK (account_type IN ('individual','company'));
    END IF;
END $$;


-- Worker 1 PAYG completion:
-- distinguish a never-funded Free wallet from an exhausted PAYG wallet.
-- This prevents a tenant with zero balance from silently falling back to
-- Free after it has entered PAYG.
ALTER TABLE user_wallets
    ADD COLUMN IF NOT EXISTS payg_activated_at TIMESTAMPTZ;

-- Pin the server price snapshot to every new reservation so a price change
-- cannot alter the amount/price recorded after an operation has started.
ALTER TABLE credit_reservations
    ADD COLUMN IF NOT EXISTS unit_price BIGINT,
    ADD COLUMN IF NOT EXISTS pricing_version BIGINT,
    ADD COLUMN IF NOT EXISTS currency TEXT;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'chk_credit_reservation_price_snapshot'
          AND conrelid = 'credit_reservations'::regclass
    ) THEN
        ALTER TABLE credit_reservations
            ADD CONSTRAINT chk_credit_reservation_price_snapshot
            CHECK (
                (unit_price IS NULL OR unit_price > 0)
                AND (pricing_version IS NULL OR pricing_version > 0)
            );
    END IF;
END $$;

ALTER TABLE agent_check_leases
    ADD COLUMN IF NOT EXISTS billing_reserved BOOLEAN NOT NULL DEFAULT FALSE;

CREATE INDEX IF NOT EXISTS idx_agent_check_leases_billing_reserved
    ON agent_check_leases(check_id, billing_reserved);


-- Backfill activation from trusted historical credits. A positive balance
-- without a historical credit is treated as already-funded for safety.
UPDATE user_wallets w
SET payg_activated_at = COALESCE(
    (
        SELECT MIN(wt.created_at)
        FROM wallet_transactions wt
        WHERE wt.tenant_id = w.tenant_id
          AND wt.transaction_type IN ('TOP_UP', 'PROMOTIONAL_CREDIT')
          AND wt.amount_units > 0
    ),
    CASE WHEN w.balance_units > 0 THEN w.created_at ELSE NULL END
)
WHERE w.payg_activated_at IS NULL;

-- Keep the legacy billing_status column coherent for existing consumers.
UPDATE user_wallets
SET billing_status = CASE
    WHEN payg_activated_at IS NULL THEN 'ACTIVE'
    WHEN balance_units <= 0 THEN 'PAYG_BALANCE_EXHAUSTED'
    ELSE 'ACTIVE'
END;

CREATE INDEX IF NOT EXISTS idx_user_wallets_payg_activated
    ON user_wallets (tenant_id)
    WHERE payg_activated_at IS NOT NULL;

-- Ensure the active pricing-config source of truth has the Part-1 Free
-- entitlement. Historical versions remain untouched.
DO $$
DECLARE
    latest RECORD;
    next_version INTEGER;
BEGIN
    SELECT version, currency, cost_per_check, effective_storage_bytes_per_check,
           cost_per_gb_hot, cost_per_gb_warm, cost_per_gb_cold,
           backup_multiplier, markup_multiplier, min_interval_seconds,
           max_monitors, max_retention_days, min_paid_monthly_price,
           free_monitors, free_interval_seconds, free_retention_days,
           avg_billable_network_bytes_per_check, cost_per_gb_egress,
           cost_safety_buffer, sms_markup_multiplier,
           enterprise_sla_monthly_price, created_by
    INTO latest
    FROM pricing_configs
    ORDER BY version DESC
    LIMIT 1;

    IF FOUND
       AND (latest.free_monitors <> 10 OR latest.free_interval_seconds <> 300)
    THEN
        next_version := latest.version + 1;
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

-- Repair any partially-migrated PAYG price book without deleting or mutating
-- historical rows. All prices are micro-USD per unit.
DO $$
DECLARE
    v BIGINT;
BEGIN
    SELECT COALESCE(MAX(version), 0) + 1 INTO v FROM payg_pricing_configs;

    INSERT INTO payg_pricing_configs
        (id, version, operation, unit_name, unit_price, currency, effective_from, effective_until, active)
    SELECT gen_random_uuid(), v, x.operation, x.unit_name, x.unit_price, 'USD', now(), NULL, TRUE
    FROM (VALUES
        ('http_check.one_time','check',2),
        ('tcp_check.one_time','check',3),
        ('dns_check.one_time','check',3),
        ('api_flow_check.one_time','request/check',4),
        ('proxy_check.one_time','check',2),
        ('http_check.monitoring','check',2),
        ('tcp_check.monitoring','check',3),
        ('dns_check.monitoring','check',3),
        ('heartbeat_processing','check',1),
        ('api_flow_check.monitoring','request/check',4),
        ('proxy_check.monitoring','check',2),
        ('rum_event','event',1),
        ('sms','message',1000),
        ('storage','MB/month',20),
        ('network_egress','MB',100)
    ) AS x(operation, unit_name, unit_price)
    WHERE NOT EXISTS (
        SELECT 1
        FROM payg_pricing_configs p
        WHERE p.operation = x.operation
    );
END $$;

COMMIT;
