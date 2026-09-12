BEGIN;

-- Provider identity must be globally unique: one successful provider payment
-- can fulfill wallet value only once.
CREATE UNIQUE INDEX IF NOT EXISTS uq_wallet_top_up_provider_payment
    ON wallet_top_up_sessions(provider_payment_id)
    WHERE provider_payment_id IS NOT NULL;

ALTER TABLE wallet_auto_top_up_attempts
    ADD COLUMN IF NOT EXISTS provider_payment_id TEXT;

ALTER TABLE wallet_auto_top_up_attempts
    DROP CONSTRAINT IF EXISTS wallet_auto_top_up_attempts_status_check;

ALTER TABLE wallet_auto_top_up_attempts
    ADD CONSTRAINT chk_wallet_auto_top_up_attempt_status
    CHECK (status IN (
        'PENDING', 'PROCESSING', 'SUCCEEDED', 'FAILED',
        'REQUIRES_RECONCILIATION'
    ));

CREATE UNIQUE INDEX IF NOT EXISTS uq_wallet_auto_top_up_provider_payment
    ON wallet_auto_top_up_attempts(provider_payment_id)
    WHERE provider_payment_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_wallet_auto_top_up_reconciliation
    ON wallet_auto_top_up_attempts(attempted_at ASC)
    WHERE status IN ('PENDING','PROCESSING','REQUIRES_RECONCILIATION')
      AND provider_reference IS NOT NULL;

-- Exact contextual PAYG rules cannot have two rows with the same start time.
-- Historical rows remain immutable; the runtime resolver uses effective_from.
CREATE UNIQUE INDEX IF NOT EXISTS uq_payg_pricing_same_context_start
ON payg_pricing_configs (
    operation,
    COALESCE(account_type, ''),
    COALESCE(region, ''),
    COALESCE(monitor_kind, ''),
    COALESCE(min_quantity, -1),
    COALESCE(max_quantity, -1),
    effective_from
);

CREATE INDEX IF NOT EXISTS idx_payg_pricing_runtime_context
ON payg_pricing_configs(operation, active, effective_from DESC, version DESC);

COMMIT;
