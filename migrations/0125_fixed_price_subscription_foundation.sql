-- TASK 1 — Fixed-price subscription foundation.
--
-- Introduces the versioned price book (plans, plan_prices,
-- plan_entitlements) and the recurring proxy monitoring tables, and
-- repairs the pre-existing `subscriptions.plan_tier` CHECK constraint
-- which only permitted ('free','tier_1','tier_5','tier_10','tier_20') —
-- incompatible with the domain `PlanTier` enum values
-- ('free','starter','pro','business','enterprise').
--
-- Nothing here removes or rewrites the PAYG tables
-- (pricing_configs, payg_pricing_configs, wallet_*, api_usage_events)
-- or any historical data. Legacy migrations 0001..0124 are untouched.
--
-- All monetary values are stored as INTEGER minor units:
--   * NGN -> kobo (1 NGN = 100 kobo)
--   * USD -> cents (1 USD = 100 cents)

BEGIN;

-- ────────────────────────────────────────────────────────────────
-- A. Repair subscriptions.plan_tier constraint + backfill legacy tiers
-- ────────────────────────────────────────────────────────────────

ALTER TABLE subscriptions
    DROP CONSTRAINT IF EXISTS chk_subscriptions_plan_tier;

-- Legacy tier_* -> new tier mapping (linear, monotonic).
-- Assumption: no production customer is currently on these legacy
-- values; if they are, this mapping is the safest incremental upgrade.
DO $$
DECLARE
    backfilled INT;
BEGIN
    UPDATE subscriptions
    SET plan_tier = CASE plan_tier
        WHEN 'tier_1'  THEN 'starter'
        WHEN 'tier_5'  THEN 'pro'
        WHEN 'tier_10' THEN 'business'
        WHEN 'tier_20' THEN 'enterprise'
        ELSE plan_tier
    END
    WHERE plan_tier IN ('tier_1','tier_5','tier_10','tier_20');
    GET DIAGNOSTICS backfilled = ROW_COUNT;
    IF backfilled > 0 THEN
        RAISE NOTICE
            'migration 0125: backfilled % subscription row(s) from legacy tier_* to PlanTier',
            backfilled;
    END IF;
END $$;

ALTER TABLE subscriptions
    ADD CONSTRAINT chk_subscriptions_plan_tier
    CHECK (plan_tier IN ('free','starter','pro','business','enterprise'));

-- ────────────────────────────────────────────────────────────────
-- B. plans — capability catalog (no monetary data here)
-- ────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS plans (
    id              UUID PRIMARY KEY,
    tier            TEXT NOT NULL UNIQUE,
    display_name    TEXT NOT NULL,
    is_self_service BOOLEAN NOT NULL DEFAULT TRUE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT chk_plans_tier
        CHECK (tier IN ('free','starter','pro','business','enterprise'))
);
COMMENT ON TABLE plans IS
    'Fixed-price plan catalog. Enterprise is not self-service; its pricing is custom-quoted and NOT stored in plan_prices.';
COMMENT ON COLUMN plans.is_self_service IS
    'FALSE for Enterprise: customers must contact sales. TRUE for all self-service tiers.';

-- ────────────────────────────────────────────────────────────────
-- C. plan_prices — versioned price book
-- ────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS plan_prices (
    id                UUID PRIMARY KEY,
    plan_id           UUID NOT NULL REFERENCES plans(id) ON DELETE CASCADE,
    currency          TEXT NOT NULL,
    billing_interval  TEXT NOT NULL,
    amount_minor      BIGINT NOT NULL CHECK (amount_minor >= 0),
    version           BIGINT NOT NULL,
    effective_from    TIMESTAMPTZ NOT NULL DEFAULT now(),
    effective_until   TIMESTAMPTZ,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT chk_plan_prices_currency
        CHECK (currency IN ('NGN','USD')),
    CONSTRAINT chk_plan_prices_interval
        CHECK (billing_interval IN ('monthly','annual')),
    CONSTRAINT chk_plan_prices_window
        CHECK (effective_until IS NULL OR effective_until > effective_from),
    CONSTRAINT uq_plan_prices_version
        UNIQUE (plan_id, currency, billing_interval, version)
);
COMMENT ON TABLE plan_prices IS
    'Versioned fixed prices. A price change MUST insert a new row with a higher version; historical rows are never rewritten.';
COMMENT ON COLUMN plan_prices.amount_minor IS
    'Minor units: kobo for NGN, cents for USD. Never floating-point.';

CREATE INDEX IF NOT EXISTS idx_plan_prices_lookup
    ON plan_prices (plan_id, currency, billing_interval, effective_from DESC);

-- ────────────────────────────────────────────────────────────────
-- D. plan_entitlements — versioned numeric entitlements
-- ────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS plan_entitlements (
    id              UUID PRIMARY KEY,
    plan_id         UUID NOT NULL REFERENCES plans(id) ON DELETE CASCADE,
    key             TEXT NOT NULL,
    limit_value     BIGINT NOT NULL CHECK (limit_value >= 0),
    version         BIGINT NOT NULL,
    effective_from  TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT chk_plan_entitlements_key
        CHECK (length(trim(key)) > 0),
    CONSTRAINT uq_plan_entitlements_version
        UNIQUE (plan_id, key, version)
);
COMMENT ON TABLE plan_entitlements IS
    'Numeric entitlements (quotas and minimums) attached to a plan. Versioned so a subscription keeps the entitlement_version it was purchased with.';

CREATE INDEX IF NOT EXISTS idx_plan_entitlements_lookup
    ON plan_entitlements (plan_id, key, version DESC);

-- ────────────────────────────────────────────────────────────────
-- E. subscriptions.plan_id — optional FK to the plans catalog
--    (plan_tier TEXT is preserved for backwards compatibility)
-- ────────────────────────────────────────────────────────────────

ALTER TABLE subscriptions
    ADD COLUMN IF NOT EXISTS plan_id UUID REFERENCES plans(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_subscriptions_plan_id
    ON subscriptions (plan_id) WHERE plan_id IS NOT NULL;

-- ────────────────────────────────────────────────────────────────
-- F. recurring_proxy_monitors — long-lived, tenant-owned monitors
--    (Deliberately distinct from proxy_check_jobs one-time batches)
-- ────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS recurring_proxy_monitors (
    id                     UUID PRIMARY KEY,
    tenant_id              UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id                UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name                   TEXT NOT NULL,
    host                   TEXT NOT NULL,
    port                   INT  NOT NULL CHECK (port > 0 AND port <= 65535),
    scheme                 TEXT NOT NULL,
    encrypted_credentials  BYTEA,
    interval_seconds       INT  NOT NULL CHECK (interval_seconds >= 300),
    user_enabled           BOOLEAN NOT NULL DEFAULT TRUE,
    entitlement_suspended  BOOLEAN NOT NULL DEFAULT FALSE,
    next_run_at            TIMESTAMPTZ,
    last_run_at            TIMESTAMPTZ,
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT chk_recurring_proxy_scheme
        CHECK (scheme IN ('http','https','socks4','socks5')),
    CONSTRAINT chk_recurring_proxy_name_nonempty
        CHECK (length(trim(name)) > 0)
);
COMMENT ON TABLE recurring_proxy_monitors IS
    'Long-lived recurring proxy monitors. Separate from proxy_check_jobs (one-time batches). Minimum interval is 300s, enforced both here and in domain code.';
COMMENT ON COLUMN recurring_proxy_monitors.user_enabled IS
    'Customer''s own enable/disable choice. Never overwritten by entitlement changes.';
COMMENT ON COLUMN recurring_proxy_monitors.entitlement_suspended IS
    'Set by backend when the current subscription no longer permits this monitor. Does NOT change user_enabled.';

-- Worker claim index: only enabled, non-suspended monitors that are due.
CREATE INDEX IF NOT EXISTS idx_recurring_proxy_monitors_due
    ON recurring_proxy_monitors (next_run_at)
    WHERE user_enabled = TRUE AND entitlement_suspended = FALSE;

CREATE INDEX IF NOT EXISTS idx_recurring_proxy_monitors_tenant
    ON recurring_proxy_monitors (tenant_id, created_at DESC);

-- ────────────────────────────────────────────────────────────────
-- G. recurring_proxy_checks — individual execution history
-- ────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS recurring_proxy_checks (
    id             UUID PRIMARY KEY,
    monitor_id     UUID NOT NULL REFERENCES recurring_proxy_monitors(id) ON DELETE CASCADE,
    tenant_id      UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    status         TEXT NOT NULL,
    started_at     TIMESTAMPTZ NOT NULL,
    finished_at    TIMESTAMPTZ,
    result_json    JSONB,
    error_message  TEXT,
    CONSTRAINT chk_recurring_proxy_checks_status
        CHECK (status IN ('running','succeeded','failed','timed_out'))
);
COMMENT ON TABLE recurring_proxy_checks IS
    'Per-execution history for recurring proxy monitors. Kept distinct from proxy_entries.result_json (one-time jobs).';

CREATE INDEX IF NOT EXISTS idx_recurring_proxy_checks_monitor
    ON recurring_proxy_checks (monitor_id, started_at DESC);

CREATE INDEX IF NOT EXISTS idx_recurring_proxy_checks_tenant
    ON recurring_proxy_checks (tenant_id, started_at DESC);

-- ────────────────────────────────────────────────────────────────
-- H. Seed plans
-- ────────────────────────────────────────────────────────────────

INSERT INTO plans (id, tier, display_name, is_self_service) VALUES
    ('00000000-0000-0000-0000-0000000000a1','free','Free',TRUE),
    ('00000000-0000-0000-0000-0000000000a2','starter','Starter',TRUE),
    ('00000000-0000-0000-0000-0000000000a3','pro','Pro',TRUE),
    ('00000000-0000-0000-0000-0000000000a4','business','Business',TRUE),
    ('00000000-0000-0000-0000-0000000000a5','enterprise','Enterprise',FALSE)
ON CONFLICT DO NOTHING;

-- ────────────────────────────────────────────────────────────────
-- I. Seed plan_prices (Free, Starter, Pro, Business only; Enterprise is custom)
--    USD: cents. NGN: kobo. Annual = 10 * monthly per product rules.
-- ────────────────────────────────────────────────────────────────

INSERT INTO plan_prices (id, plan_id, currency, billing_interval, amount_minor, version) VALUES
    -- Free
    ('10000000-0000-0000-0000-000000000f01','00000000-0000-0000-0000-0000000000a1','USD','monthly',0,1),
    ('10000000-0000-0000-0000-000000000f02','00000000-0000-0000-0000-0000000000a1','USD','annual',0,1),
    ('10000000-0000-0000-0000-000000000f03','00000000-0000-0000-0000-0000000000a1','NGN','monthly',0,1),
    ('10000000-0000-0000-0000-000000000f04','00000000-0000-0000-0000-0000000000a1','NGN','annual',0,1),
    -- Starter: $5/mo = 500 cents; $50/yr = 5000 cents; ₦8,000/mo = 800,000 kobo; ₦80,000/yr = 8,000,000 kobo
    ('10000000-0000-0000-0000-000000000101','00000000-0000-0000-0000-0000000000a2','USD','monthly',500,1),
    ('10000000-0000-0000-0000-000000000102','00000000-0000-0000-0000-0000000000a2','USD','annual',5000,1),
    ('10000000-0000-0000-0000-000000000103','00000000-0000-0000-0000-0000000000a2','NGN','monthly',800000,1),
    ('10000000-0000-0000-0000-000000000104','00000000-0000-0000-0000-0000000000a2','NGN','annual',8000000,1),
    -- Pro: $19/mo = 1900 cents; $190/yr = 19000 cents; ₦30,000/mo = 3,000,000 kobo; ₦300,000/yr = 30,000,000 kobo
    ('10000000-0000-0000-0000-000000000201','00000000-0000-0000-0000-0000000000a3','USD','monthly',1900,1),
    ('10000000-0000-0000-0000-000000000202','00000000-0000-0000-0000-0000000000a3','USD','annual',19000,1),
    ('10000000-0000-0000-0000-000000000203','00000000-0000-0000-0000-0000000000a3','NGN','monthly',3000000,1),
    ('10000000-0000-0000-0000-000000000204','00000000-0000-0000-0000-0000000000a3','NGN','annual',30000000,1),
    -- Business: $49/mo = 4900 cents; $490/yr = 49000 cents; ₦77,000/mo = 7,700,000 kobo; ₦770,000/yr = 77,000,000 kobo
    ('10000000-0000-0000-0000-000000000301','00000000-0000-0000-0000-0000000000a4','USD','monthly',4900,1),
    ('10000000-0000-0000-0000-000000000302','00000000-0000-0000-0000-0000000000a4','USD','annual',49000,1),
    ('10000000-0000-0000-0000-000000000303','00000000-0000-0000-0000-0000000000a4','NGN','monthly',7700000,1),
    ('10000000-0000-0000-0000-000000000304','00000000-0000-0000-0000-0000000000a4','NGN','annual',77000000,1)
ON CONFLICT DO NOTHING;

-- ────────────────────────────────────────────────────────────────
-- J. Seed plan_entitlements v1
--
-- Source: PingVertex Pricing & Entitlements v1 (Task 0 audit). The
-- recurring-proxy-monitor entries are a *starting point* only; TASK 2
-- is expected to recalculate them from actual execution cost.
--
-- 8,640 checks/mo = one monitor at 300s interval for 30 days.
-- So 5 Pro monitors x 8,640 = 43,200 checks/mo; 10 Business x 8,640 = 86,400.
-- ────────────────────────────────────────────────────────────────

WITH plan_by_tier AS (
    SELECT tier, id FROM plans
),
seed(plan_tier, key, limit_value) AS (
    VALUES
    -- HTTP/TCP/DNS monitors (shared quota)
    ('free','max_uptime_monitors',20),
    ('starter','max_uptime_monitors',30),
    ('pro','max_uptime_monitors',80),
    ('business','max_uptime_monitors',100),
    ('enterprise','max_uptime_monitors',1000),

    -- Heartbeat monitors (separate quota)
    ('free','max_heartbeat_monitors',5),
    ('starter','max_heartbeat_monitors',10),
    ('pro','max_heartbeat_monitors',30),
    ('business','max_heartbeat_monitors',50),
    ('enterprise','max_heartbeat_monitors',500),

    -- Minimum uptime check interval (seconds)
    ('free','min_uptime_interval_seconds',300),
    ('starter','min_uptime_interval_seconds',60),
    ('pro','min_uptime_interval_seconds',60),
    ('business','min_uptime_interval_seconds',30),
    ('enterprise','min_uptime_interval_seconds',30),

    -- Regions per monitor
    ('free','max_regions_per_monitor',1),
    ('starter','max_regions_per_monitor',2),
    ('pro','max_regions_per_monitor',3),
    ('business','max_regions_per_monitor',3),
    ('enterprise','max_regions_per_monitor',10),

    -- API Flow monitors (separate quota)
    ('free','max_api_flow_monitors',0),
    ('starter','max_api_flow_monitors',2),
    ('pro','max_api_flow_monitors',10),
    ('business','max_api_flow_monitors',30),
    ('enterprise','max_api_flow_monitors',100),

    -- API Flow steps per flow
    ('free','max_api_flow_steps',0),
    ('starter','max_api_flow_steps',5),
    ('pro','max_api_flow_steps',10),
    ('business','max_api_flow_steps',15),
    ('enterprise','max_api_flow_steps',50),

    -- Proxy entries per one-time job
    ('free','max_proxy_entries_per_job',0),
    ('starter','max_proxy_entries_per_job',500),
    ('pro','max_proxy_entries_per_job',2000),
    ('business','max_proxy_entries_per_job',5000),
    ('enterprise','max_proxy_entries_per_job',20000),

    -- Concurrent one-time proxy jobs
    ('free','max_concurrent_proxy_jobs',0),
    ('starter','max_concurrent_proxy_jobs',1),
    ('pro','max_concurrent_proxy_jobs',2),
    ('business','max_concurrent_proxy_jobs',3),
    ('enterprise','max_concurrent_proxy_jobs',10),

    -- Recurring proxy monitors (v1 starting point; TASK 2 finalizes)
    ('free','max_recurring_proxy_monitors',0),
    ('starter','max_recurring_proxy_monitors',0),
    ('pro','max_recurring_proxy_monitors',5),
    ('business','max_recurring_proxy_monitors',10),
    ('enterprise','max_recurring_proxy_monitors',100),

    -- Minimum interval is 300s for every tier (system-wide floor)
    ('free','min_recurring_proxy_interval_seconds',300),
    ('starter','min_recurring_proxy_interval_seconds',300),
    ('pro','min_recurring_proxy_interval_seconds',300),
    ('business','min_recurring_proxy_interval_seconds',300),
    ('enterprise','min_recurring_proxy_interval_seconds',300),

    -- Monthly execution allowance (5 monitors x 8640, 10 x 8640)
    ('free','max_recurring_proxy_executions_per_month',0),
    ('starter','max_recurring_proxy_executions_per_month',0),
    ('pro','max_recurring_proxy_executions_per_month',43200),
    ('business','max_recurring_proxy_executions_per_month',86400),
    ('enterprise','max_recurring_proxy_executions_per_month',500000),

    -- RUM
    ('free','max_rum_sites',0),
    ('starter','max_rum_sites',1),
    ('pro','max_rum_sites',3),
    ('business','max_rum_sites',10),
    ('enterprise','max_rum_sites',50),

    ('free','max_rum_events_per_month',0),
    ('starter','max_rum_events_per_month',25000),
    ('pro','max_rum_events_per_month',100000),
    ('business','max_rum_events_per_month',500000),
    ('enterprise','max_rum_events_per_month',5000000)
)
INSERT INTO plan_entitlements (id, plan_id, key, limit_value, version)
SELECT gen_random_uuid(), p.id, s.key, s.limit_value, 1
FROM seed s
JOIN plan_by_tier p ON p.tier = s.plan_tier
ON CONFLICT (plan_id, key, version) DO NOTHING;

-- ────────────────────────────────────────────────────────────────
-- K. Sanity assertion (rolls back the whole migration on failure)
-- ────────────────────────────────────────────────────────────────

DO $$
DECLARE
    plan_count INT;
    price_count INT;
    ent_count INT;
BEGIN
    SELECT COUNT(*) INTO plan_count FROM plans;
    IF plan_count < 5 THEN
        RAISE EXCEPTION 'migration 0125: expected >=5 plans after seed, got %', plan_count;
    END IF;

    SELECT COUNT(*) INTO price_count FROM plan_prices
        WHERE plan_id IN (SELECT id FROM plans WHERE tier IN ('starter','pro','business'));
    IF price_count < 12 THEN
        RAISE EXCEPTION
            'migration 0125: expected 12 paid prices (3 tiers x 2 currencies x 2 intervals), got %',
            price_count;
    END IF;

    SELECT COUNT(*) INTO ent_count FROM plan_entitlements;
    IF ent_count < 40 THEN
        RAISE EXCEPTION 'migration 0125: expected >=40 entitlements after seed, got %', ent_count;
    END IF;
END $$;

COMMIT;
