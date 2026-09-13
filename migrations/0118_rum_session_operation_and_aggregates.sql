-- GAP 3 Kashi E: RUM session-based pricing + daily aggregation.
--
-- Replaces per-beacon billing (`rum_event`) with per-session billing
-- (`rum_session`). One RUM session = 30 minutes of user activity on a
-- site; all pageviews/errors within that window are grouped under one
-- session row and charged once.
--
-- Raw RUM events (sessions, page views, errors) are kept for 7 days,
-- after which a worker rolls them up into `rum_daily_aggregates` and
-- deletes the raw rows. The dashboard reads raw data for the last 7
-- days and aggregates for older days.
BEGIN;

-- 1. New pricing operation: `rum_session`, priced at the same rate as
--    the retired `rum_event` (1000 nano-USD per session).
DO $$
DECLARE
  v BIGINT;
BEGIN
  SELECT COALESCE(MAX(version), 0) + 1 INTO v FROM payg_pricing_configs;
  INSERT INTO payg_pricing_configs
    (id, version, operation, unit_name, unit_price, currency,
     effective_from, effective_until, active)
  VALUES
    (gen_random_uuid(), v, 'rum_session', 'session', 1000, 'USD',
     now(), NULL, TRUE)
  ON CONFLICT (version, operation) DO NOTHING;
END $$;

-- 2. Retire `rum_event` so `list_active()` stops advertising it. The
--    historical pricing rows remain queryable (append-only price book).
INSERT INTO payg_retired_operations (operation, reason)
VALUES (
    'rum_event',
    'Replaced by rum_session in GAP 3 Kashi E: per-session billing, not per-beacon'
)
ON CONFLICT (operation) DO NOTHING;

-- 3. Daily aggregates, kept indefinitely (unlike the 7-day raw data).
CREATE TABLE IF NOT EXISTS rum_daily_aggregates (
    site_id UUID NOT NULL REFERENCES rum_sites(id) ON DELETE CASCADE,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    day DATE NOT NULL,
    total_sessions BIGINT NOT NULL DEFAULT 0,
    page_view_count BIGINT NOT NULL DEFAULT 0,
    js_error_count BIGINT NOT NULL DEFAULT 0,
    resource_error_count BIGINT NOT NULL DEFAULT 0,
    avg_load_time_ms INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (site_id, day),
    CONSTRAINT chk_rum_daily_aggregates_nonneg CHECK (
        total_sessions >= 0
        AND page_view_count >= 0
        AND js_error_count >= 0
        AND resource_error_count >= 0
        AND (avg_load_time_ms IS NULL OR avg_load_time_ms >= 0)
    )
);

CREATE INDEX IF NOT EXISTS idx_rum_daily_aggregates_tenant_day
    ON rum_daily_aggregates (tenant_id, day DESC);

COMMIT;
