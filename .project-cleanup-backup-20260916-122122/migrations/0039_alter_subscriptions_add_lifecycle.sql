-- B12 — Subscription Lifecycle (Trial, Grace Period, Expiry, Cancellation).
--
-- Extends B11's minimal subscriptions table. Existing rows default to
-- status = 'active' with no lifecycle dates set, which matches how they
-- were already being treated (a bare plan_tier row = a fully active
-- subscription) — so no separate backfill is required.
ALTER TABLE subscriptions
    ADD COLUMN status TEXT NOT NULL DEFAULT 'active',
    ADD COLUMN trial_ends_at TIMESTAMPTZ,
    ADD COLUMN grace_period_ends_at TIMESTAMPTZ,
    ADD COLUMN current_period_ends_at TIMESTAMPTZ,
    ADD COLUMN canceled_at TIMESTAMPTZ,
    ADD COLUMN cancel_at_period_end BOOLEAN NOT NULL DEFAULT FALSE,
    ADD CONSTRAINT chk_subscriptions_status
        CHECK (status IN ('trialing', 'active', 'past_due', 'expired', 'canceled'));
