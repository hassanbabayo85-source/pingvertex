-- ============================================================================
-- GAP 3 — Kashi A1: Switch PAYG unit from micro-USD to nano-USD (×1000)
--
-- The old PAYG price book stored prices in micro-USD (1 unit = $0.000001).
-- That granularity cannot represent the newly-approved per-kind pricing
-- (HTTP=0.0000010, TCP=0.0000007, DNS=0.0000005) without fractions.
--
-- This migration switches the runtime unit to nano-USD (1 unit = $10^-9 USD)
-- and simultaneously inserts a new price version reflecting the per-kind
-- rates. Wallet balances, historical transactions, reservations, and usage
-- events are scaled ×1000 in the same transaction so no runtime path can
-- observe a mixed-unit state.
--
-- Invariants preserved
-- --------------------
-- * `payg_pricing_configs` is append-only (immutability trigger forbids
--   UPDATE/DELETE). New versions are INSERTed; historical rows stay.
-- * `payg_retired_operations` is untouched here — storage retirement is
--   handled in a later migration alongside its code removal.
-- * All arithmetic is checked for i64 overflow in a pre-flight block before
--   any row is mutated.
--
-- Prices after this migration (nano-USD)
-- --------------------------------------
--   http_check.one_time         1000     ($0.0000010)
--   tcp_check.one_time           700     ($0.0000007)
--   dns_check.one_time           500     ($0.0000005)
--   api_flow_check.one_time     1000     (base rate)
--   proxy_check.one_time        2000
--   http_check.monitoring       1000
--   tcp_check.monitoring         700
--   dns_check.monitoring         500
--   api_flow_check.monitoring   1000     (base rate)
--   proxy_check.monitoring      2000
--   heartbeat_processing        1000
--   rum_event                   1000
--   sms                      1000000
--   storage                    20000     (retired in a later migration)
--   network_egress            100000
--
-- Backup
-- ------
-- Before running in production:
--
--   pg_dump -t user_wallets -t wallet_transactions \
--           -t credit_reservations -t api_usage_events \
--           -t payg_pricing_configs -t payg_retired_operations \
--       > backup_before_gap3_a1.sql
--
-- ============================================================================

BEGIN;

-- ----------------------------------------------------------------------------
-- Pre-flight: i64 overflow guard.
-- i64 max is 9,223,372,036,854,775,807. We refuse the migration if any
-- existing value exceeds 9,000,000,000,000,000 (9e15).
-- ----------------------------------------------------------------------------
DO $$
DECLARE
  overflow_threshold CONSTANT BIGINT := 9000000000000000;
  v BIGINT;
BEGIN
  SELECT COALESCE(MAX(balance_units), 0) INTO v FROM user_wallets;
  IF v > overflow_threshold THEN
    RAISE EXCEPTION 'user_wallets.balance_units=% exceeds threshold', v;
  END IF;

  SELECT COALESCE(MAX(amount_units), 0) INTO v FROM wallet_transactions;
  IF v > overflow_threshold THEN
    RAISE EXCEPTION 'wallet_transactions.amount_units=% exceeds threshold', v;
  END IF;

  SELECT COALESCE(MAX(balance_after_units), 0) INTO v FROM wallet_transactions;
  IF v > overflow_threshold THEN
    RAISE EXCEPTION 'wallet_transactions.balance_after_units=% exceeds threshold', v;
  END IF;

  SELECT COALESCE(MAX(amount_units), 0) INTO v FROM credit_reservations;
  IF v > overflow_threshold THEN
    RAISE EXCEPTION 'credit_reservations.amount_units=% exceeds threshold', v;
  END IF;

  SELECT COALESCE(MAX(unit_price), 0) INTO v FROM credit_reservations WHERE unit_price IS NOT NULL;
  IF v > overflow_threshold THEN
    RAISE EXCEPTION 'credit_reservations.unit_price=% exceeds threshold', v;
  END IF;

  SELECT COALESCE(MAX(unit_price), 0) INTO v FROM api_usage_events;
  IF v > overflow_threshold THEN
    RAISE EXCEPTION 'api_usage_events.unit_price=% exceeds threshold', v;
  END IF;

  SELECT COALESCE(MAX(charge_units), 0) INTO v FROM api_usage_events;
  IF v > overflow_threshold THEN
    RAISE EXCEPTION 'api_usage_events.charge_units=% exceeds threshold', v;
  END IF;
END $$;

-- 1. Scale wallet balances
UPDATE user_wallets SET balance_units = balance_units * 1000;

-- 2. Scale historical wallet transactions.
--
-- `wallet_transactions` carries an immutability trigger
-- (`prevent_wallet_transaction_mutation`) that forbids UPDATEs on
-- financial columns. That trigger is a BEFORE UPDATE trigger, so it fires
-- even on an empty table's UPDATE statement. Temporarily disable it for
-- this one-shot backfill; the ALTERs are transactional, so the trigger is
-- restored automatically on COMMIT or ROLLBACK.
ALTER TABLE wallet_transactions DISABLE TRIGGER USER;
UPDATE wallet_transactions
SET amount_units = amount_units * 1000,
    balance_after_units = balance_after_units * 1000;
ALTER TABLE wallet_transactions ENABLE TRIGGER USER;

-- 3. Scale credit reservations
UPDATE credit_reservations
SET amount_units = amount_units * 1000,
    unit_price   = unit_price * 1000;

-- 4. Scale usage events
UPDATE api_usage_events
SET unit_price   = unit_price * 1000,
    charge_units = charge_units * 1000;

-- 5. Insert the new per-kind price version in nano-USD.
DO $$
DECLARE
  next_version BIGINT;
BEGIN
  SELECT COALESCE(MAX(version), 0) + 1 INTO next_version
  FROM payg_pricing_configs;

  INSERT INTO payg_pricing_configs
    (id, version, operation, unit_name, unit_price, currency,
     effective_from, effective_until, active)
  VALUES
    (gen_random_uuid(), next_version, 'http_check.one_time',       'check',          1000,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), next_version, 'tcp_check.one_time',        'check',           700,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), next_version, 'dns_check.one_time',        'check',           500,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), next_version, 'api_flow_check.one_time',   'request/check',  1000,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), next_version, 'proxy_check.one_time',      'check',          2000,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), next_version, 'http_check.monitoring',     'check',          1000,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), next_version, 'tcp_check.monitoring',      'check',           700,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), next_version, 'dns_check.monitoring',      'check',           500,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), next_version, 'api_flow_check.monitoring', 'request/check',  1000,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), next_version, 'proxy_check.monitoring',    'check',          2000,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), next_version, 'heartbeat_processing',      'check',          1000,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), next_version, 'rum_event',                 'event',          1000,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), next_version, 'sms',                       'message',     1000000,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), next_version, 'storage',                   'MB/month',      20000,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), next_version, 'network_egress',            'MB',           100000,    'USD', now(), NULL, TRUE)
  ON CONFLICT (version, operation) DO NOTHING;
END $$;

COMMIT;
