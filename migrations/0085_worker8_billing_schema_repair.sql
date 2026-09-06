BEGIN;
ALTER TABLE checkout_sessions DROP CONSTRAINT IF EXISTS chk_checkout_sessions_plan_tier;
ALTER TABLE checkout_sessions ADD CONSTRAINT chk_checkout_sessions_plan_tier CHECK (plan_tier IN ('free','starter','pro','business','enterprise'));
COMMIT;

