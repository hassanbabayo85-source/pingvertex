-- GAP 3 Kashi D: tenant alert fair-use allowances.
--
-- Fair-use monthly allowance for SMS (and, in the future, voice minutes
-- and phone numbers). The allowance is proportional to the tenant's
-- active monitor count at the start of the cycle:
--
--   sms_limit = ceil(enabled_monitors * 250 / 1000)
--
-- Baseline (from the GAP 3 blueprint): 1,000 monitors = 250 SMS.
-- 10 monitors = 3 SMS; 1,000 monitors = 250; 1,001 = 251.
--
-- The allowance is consumed BEFORE the wallet. When exhausted, SMS
-- falls back to (a) paid SMS credits in the wallet, or (b) the email
-- channel (free, unlimited). Voice and phone-number columns are present
-- for forward compatibility; they are always 0 today (the Voice
-- integration is not implemented yet).

CREATE TABLE IF NOT EXISTS tenant_alert_allowances (
    tenant_id UUID PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    cycle_started_at TIMESTAMPTZ NOT NULL,
    cycle_ends_at TIMESTAMPTZ NOT NULL,
    sms_limit INTEGER NOT NULL DEFAULT 0 CHECK (sms_limit >= 0),
    sms_used INTEGER NOT NULL DEFAULT 0 CHECK (sms_used >= 0),
    voice_limit INTEGER NOT NULL DEFAULT 0 CHECK (voice_limit >= 0),
    voice_used INTEGER NOT NULL DEFAULT 0 CHECK (voice_used >= 0),
    phone_numbers_limit INTEGER NOT NULL DEFAULT 0 CHECK (phone_numbers_limit >= 0),
    phone_numbers_used INTEGER NOT NULL DEFAULT 0 CHECK (phone_numbers_used >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT chk_tenant_alert_allowance_cycle
        CHECK (cycle_ends_at > cycle_started_at)
);

CREATE INDEX IF NOT EXISTS idx_tenant_alert_allowances_cycle_end
    ON tenant_alert_allowances (cycle_ends_at);

-- Seed: give every existing tenant an empty row for the current cycle.
-- The reset worker (or the first SMS attempt) will populate sms_limit
-- with the correct value; this seed just ensures the row exists so the
-- dashboard can render something on the next page load.
INSERT INTO tenant_alert_allowances
    (tenant_id, cycle_started_at, cycle_ends_at, sms_limit, sms_used)
SELECT
    id,
    date_trunc('month', now()),
    date_trunc('month', now()) + interval '1 month',
    0,
    0
FROM tenants
ON CONFLICT (tenant_id) DO NOTHING;

COMMIT;
