-- GAP Add-on Restoration (A2): allow add-on checkout sessions to
-- store `plan_tier = NULL`.
--
-- Migration 0040 made `checkout_sessions.plan_tier` NOT NULL because
-- every checkout session was a subscription-tier purchase. The PAYG
-- runtime adds a second kind of checkout session — an add-on purchase —
-- which has no plan tier. `CheckoutRequest.plan_tier` and
-- `CheckoutSession.plan_tier` are now `Option<PlanTier>`; this migration
-- makes the column accept NULL so the repository can persist those
-- rows.
--
-- Existing rows are unaffected. The CHECK constraint added by 0040 (and
-- relaxed by 0073) already permits NULL because it only tests the
-- non-NULL values against the allowed set.
BEGIN;

ALTER TABLE checkout_sessions
    ALTER COLUMN plan_tier DROP NOT NULL;

COMMIT;
