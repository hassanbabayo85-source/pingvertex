-- B36 — Billing Webhook Idempotency hardening.
--
-- Prior to this migration `billing_webhook_idempotency` only recorded
-- that an event id had been seen. If subscription/audit processing
-- failed AFTER the row was inserted, the row stayed forever, and a
-- provider retry with the same event id was silently ignored — losing
-- the event permanently.
--
-- This adds a small state machine so retries after a failed processing
-- attempt can be reclaimed:
--   pending    -> processing -> processed
--   processing -> pending     (on application-level failure)
--   processing -> processing  (reclaimed only after a stale timeout)
--
-- `processed` is terminal. `processing_started_at` is the stale lease
-- timestamp used by `try_claim`.
ALTER TABLE billing_webhook_idempotency
    ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'processing', 'processed')),
    ADD COLUMN IF NOT EXISTS processing_started_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS processed_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_billing_webhook_idempotency_processing
    ON billing_webhook_idempotency (processing_started_at)
    WHERE status = 'processing';
