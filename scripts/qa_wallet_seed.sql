-- ============================================================================
-- QA Wallet Seed — $50 nanoUSD ga kowane QA Tester tenant
-- ============================================================================

BEGIN;

-- ----------------------------------------------------------------------------
-- Step 1: create user_wallets ($50 = 50_000_000_000 nanoUSD)
-- ----------------------------------------------------------------------------
INSERT INTO user_wallets (
    id, tenant_id, currency, balance_units,
    payg_activated_at, created_at, updated_at
)
SELECT
    gen_random_uuid(),
    t.id,
    'USD',
    50000000000,
    now(),
    now(),
    now()
FROM tenants t
WHERE t.name LIKE 'QA Tester % workspace'
ON CONFLICT (tenant_id) DO UPDATE SET
    balance_units = EXCLUDED.balance_units,
    payg_activated_at = COALESCE(user_wallets.payg_activated_at, EXCLUDED.payg_activated_at),
    updated_at = now();

-- ----------------------------------------------------------------------------
-- Step 2: create wallet_settings
-- ----------------------------------------------------------------------------
INSERT INTO wallet_settings (tenant_id)
SELECT t.id
FROM tenants t
WHERE t.name LIKE 'QA Tester % workspace'
ON CONFLICT (tenant_id) DO NOTHING;

-- ----------------------------------------------------------------------------
-- Step 3: write wallet_transactions (tenant_id added)
-- ----------------------------------------------------------------------------
INSERT INTO wallet_transactions (
    id, wallet_id, tenant_id, transaction_type, amount_units, balance_after_units,
    idempotency_key, reference_type, reference_id, description, created_at
)
SELECT
    gen_random_uuid(),
    w.id,
    w.tenant_id,
    'ADJUSTMENT',
    50000000000,
    50000000000,
    'qa-seed-topup-' || w.tenant_id::text,
    'QA_SEED',
    'topup-50usd',
    'QA test seed: $50.00 manual credit',
    now()
FROM user_wallets w
JOIN tenants t ON t.id = w.tenant_id
WHERE t.name LIKE 'QA Tester % workspace'
  AND NOT EXISTS (
      SELECT 1 FROM wallet_transactions wt
      WHERE wt.wallet_id = w.id
        AND wt.idempotency_key = 'qa-seed-topup-' || w.tenant_id::text
  );

COMMIT;

-- ----------------------------------------------------------------------------
-- Tabbatarwa
-- ----------------------------------------------------------------------------
SELECT
    t.name                                    AS tenant,
    u.email                                   AS owner_email,
    w.currency,
    w.balance_units,
    w.balance_units / 1000000000.0            AS balance_usd,
    w.payg_activated_at IS NOT NULL           AS payg_active
FROM user_wallets w
JOIN tenants t ON t.id = w.tenant_id
JOIN tenant_memberships tm ON tm.tenant_id = t.id AND tm.role = 'Owner'
JOIN users u ON u.id = tm.user_id
WHERE t.name LIKE 'QA Tester % workspace'
ORDER BY t.name;
