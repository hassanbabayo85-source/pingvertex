BEGIN;

-- Optional, server-owned pricing dimensions. NULL means "generic fallback".
ALTER TABLE payg_pricing_configs
    ADD COLUMN IF NOT EXISTS account_type TEXT,
    ADD COLUMN IF NOT EXISTS region TEXT,
    ADD COLUMN IF NOT EXISTS monitor_kind TEXT,
    ADD COLUMN IF NOT EXISTS min_quantity BIGINT,
    ADD COLUMN IF NOT EXISTS max_quantity BIGINT;

ALTER TABLE payg_pricing_configs
    DROP CONSTRAINT IF EXISTS chk_payg_pricing_context_account_type;
ALTER TABLE payg_pricing_configs
    ADD CONSTRAINT chk_payg_pricing_context_account_type
    CHECK (account_type IS NULL OR account_type IN ('individual','company'));

ALTER TABLE payg_pricing_configs
    DROP CONSTRAINT IF EXISTS chk_payg_pricing_context_region;
ALTER TABLE payg_pricing_configs
    ADD CONSTRAINT chk_payg_pricing_context_region
    CHECK (region IS NULL OR region IN ('us-east','eu-west','af-west','ap-southeast'));

ALTER TABLE payg_pricing_configs
    DROP CONSTRAINT IF EXISTS chk_payg_pricing_context_monitor_kind;
ALTER TABLE payg_pricing_configs
    ADD CONSTRAINT chk_payg_pricing_context_monitor_kind
    CHECK (monitor_kind IS NULL OR monitor_kind IN ('http','tcp','dns','heartbeat','flow'));

ALTER TABLE payg_pricing_configs
    DROP CONSTRAINT IF EXISTS chk_payg_pricing_context_quantity;
ALTER TABLE payg_pricing_configs
    ADD CONSTRAINT chk_payg_pricing_context_quantity
    CHECK (
        (min_quantity IS NULL OR min_quantity >= 0)
        AND (max_quantity IS NULL OR max_quantity >= 0)
        AND (min_quantity IS NULL OR max_quantity IS NULL OR min_quantity <= max_quantity)
    );

CREATE INDEX IF NOT EXISTS idx_payg_pricing_context_lookup
    ON payg_pricing_configs(
        operation,
        active,
        effective_from,
        account_type,
        region,
        monitor_kind,
        min_quantity,
        max_quantity,
        version DESC
    );

COMMIT;
