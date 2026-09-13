-- GAP 3 Kashi C: per-tenant retention settings + default retention.
--
-- Blueprint:
--   * Per-tenant retention (a cikin billing / tenant settings).
--   * 30-day default for new PAYG tenants.
--   * Grandfathering: existing tenants that already had a subscription
--     retention_days keep it (source = 'legacy') so the retention
--     worker does not start deleting their history on the next sweep.
--   * Hard ceiling = pricing_configs.max_retention_days (3650).
--
-- The pricing_configs.default_retention_days column (added below) is the
-- value used when a tenant has no explicit row in tenant_retention_settings.
-- It is DDL, not row UPDATE — the append-only pricing_configs trigger
-- only blocks row-level UPDATE/DELETE, not schema changes.

-- 1. Add the default retention column.
ALTER TABLE pricing_configs
    ADD COLUMN IF NOT EXISTS default_retention_days INTEGER NOT NULL DEFAULT 30;

-- 2. Per-tenant retention settings table.
CREATE TABLE IF NOT EXISTS tenant_retention_settings (
    tenant_id UUID PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    retention_days INTEGER NOT NULL,
    source TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT chk_tenant_retention_days_range
        CHECK (retention_days > 0 AND retention_days <= 3650),
    CONSTRAINT chk_tenant_retention_source
        CHECK (source IN ('default', 'legacy', 'addon'))
);

CREATE INDEX IF NOT EXISTS idx_tenant_retention_settings_source
    ON tenant_retention_settings (source);

-- 3. Grandfather existing tenants.
--
-- For every tenant, choose the effective retention:
--   * If the tenant has a subscription with `retention_days` set, use
--     that and mark the row source = 'legacy'. This preserves the old
--     subscription-era retention (which could be up to 3650 days) so the
--     retention worker does not start deleting history on the next sweep.
--   * Otherwise, use the active pricing_configs.default_retention_days
--     (30) and mark the row source = 'default'.
--
-- The ON CONFLICT DO NOTHING makes the seed idempotent. It only inserts
-- rows for tenants that do not already have a settings row, so a later
-- re-run cannot clobber a value the user has since set.

INSERT INTO tenant_retention_settings (tenant_id, retention_days, source)
SELECT
    t.id,
    COALESCE(
        s.retention_days,
        (SELECT default_retention_days
         FROM pricing_configs
         ORDER BY version DESC
         LIMIT 1),
        30
    ),
    CASE
        WHEN s.retention_days IS NOT NULL THEN 'legacy'
        ELSE 'default'
    END
FROM tenants t
LEFT JOIN subscriptions s ON s.tenant_id = t.id
WHERE NOT EXISTS (
    SELECT 1 FROM tenant_retention_settings rs WHERE rs.tenant_id = t.id
);

COMMIT;
