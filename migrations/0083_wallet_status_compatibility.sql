BEGIN;

-- ============================================================
-- WORKER 7 STATUS COMPATIBILITY
--
-- 0082 introduced durable auto-top-up attempts and refund
-- requests. The actual current application code uses:
--
-- Auto top-up attempts:
--   PENDING
--   SUCCEEDED
--   FAILED
--
-- Refund requests:
--   PENDING
--   SUBMITTED
--   COMPLETED
--   FAILED
--
-- Keep the database constraints aligned with the source code.
-- ============================================================


-- ------------------------------------------------------------
-- 1. AUTO TOP-UP ATTEMPT STATUS
-- ------------------------------------------------------------

ALTER TABLE wallet_auto_top_up_attempts
    DROP CONSTRAINT IF EXISTS wallet_auto_top_up_attempts_status_check;

ALTER TABLE wallet_auto_top_up_attempts
    DROP CONSTRAINT IF EXISTS chk_wallet_auto_top_up_attempts_status;

ALTER TABLE wallet_auto_top_up_attempts
    ADD CONSTRAINT chk_wallet_auto_top_up_attempts_status
    CHECK (
        status IN (
            'PENDING',
            'SUCCEEDED',
            'FAILED'
        )
    );


-- ------------------------------------------------------------
-- 2. REFUND REQUEST STATUS
-- ------------------------------------------------------------

ALTER TABLE wallet_refund_requests
    DROP CONSTRAINT IF EXISTS wallet_refund_requests_status_check;

ALTER TABLE wallet_refund_requests
    DROP CONSTRAINT IF EXISTS chk_wallet_refund_requests_status;

ALTER TABLE wallet_refund_requests
    ADD CONSTRAINT chk_wallet_refund_requests_status
    CHECK (
        status IN (
            'PENDING',
            'SUBMITTED',
            'COMPLETED',
            'FAILED'
        )
    );


COMMIT;
