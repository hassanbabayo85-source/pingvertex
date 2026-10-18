-- PAYG remediation Part 4: durable billing-block state and reconciliation indexes.
BEGIN;

ALTER TABLE user_wallets
    ADD COLUMN IF NOT EXISTS billing_status TEXT NOT NULL DEFAULT 'ACTIVE';

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'chk_user_wallets_billing_status'
          AND conrelid = 'user_wallets'::regclass
    ) THEN
        ALTER TABLE user_wallets
            ADD CONSTRAINT chk_user_wallets_billing_status
            CHECK (billing_status IN ('ACTIVE','PAYG_BALANCE_EXHAUSTED'));
    END IF;
END;
$$;

-- Normalize legacy wallets before runtime enforcement starts.
UPDATE user_wallets
SET billing_status = CASE
    WHEN balance_units <= 0 THEN 'PAYG_BALANCE_EXHAUSTED'
    ELSE 'ACTIVE'
END;

CREATE INDEX IF NOT EXISTS idx_credit_reservations_request_status
    ON credit_reservations(wallet_id, request_id, status);

CREATE INDEX IF NOT EXISTS idx_wallet_transactions_payg_reference
    ON wallet_transactions(reference_type, reference_id, transaction_type)
    WHERE reference_type = 'credit_reservation';

CREATE INDEX IF NOT EXISTS idx_proxy_check_jobs_payg_reconciliation
    ON proxy_check_jobs(tenant_id, status, checked_count, heartbeat_at);

COMMIT;
