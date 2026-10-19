-- 0131_checkout_sessions_amount_minor.sql
--
-- Task 9D.1 — adds the column that `PostgresCheckoutSessionRepository::create`
-- has always required, but which Task 6's migration 0129 never actually added.
--
-- Root cause: Task 6's patch that was supposed to append the `amount_minor`
-- ALTER TABLE to 0129 had a string guard `if "amount_minor BIGINT" not in src`
-- that was already true (because the `subscription_billing_records` block
-- earlier in the same file uses `amount_minor BIGINT NOT NULL`). The guard
-- therefore skipped appending the `checkout_sessions.amount_minor` block.
-- The INSERT statement in the repository was written separately, and no
-- integration test ever drove the real Postgres repository for checkout
-- sessions — every Task 6 / Task 8 / Task 9B test used the in-memory impl.
-- Result: the defect only surfaces against real Postgres, first observed by
-- the 9D.1 concurrency test.
--
-- Postgres behavior: `ADD COLUMN IF NOT EXISTS` is idempotent, so this
-- migration is safe to re-run even if some environment manually patched
-- the column. No historical data is touched: existing rows get `NULL`,
-- which the code already handles via `Option<i64>`.

ALTER TABLE checkout_sessions
    ADD COLUMN IF NOT EXISTS amount_minor BIGINT;

ALTER TABLE checkout_sessions
    DROP CONSTRAINT IF EXISTS chk_checkout_sessions_amount_minor;
ALTER TABLE checkout_sessions
    ADD CONSTRAINT chk_checkout_sessions_amount_minor
    CHECK (amount_minor IS NULL OR amount_minor >= 0);
