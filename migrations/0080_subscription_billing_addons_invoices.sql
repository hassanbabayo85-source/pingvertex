BEGIN;

ALTER TABLE checkout_sessions
    ADD COLUMN IF NOT EXISTS plan_change_id UUID,
    ADD COLUMN IF NOT EXISTS addon_subscription_id UUID,
    ADD COLUMN IF NOT EXISTS amount NUMERIC(20,6),
    ADD COLUMN IF NOT EXISTS currency TEXT;
CREATE INDEX IF NOT EXISTS idx_checkout_sessions_plan_change
    ON checkout_sessions(plan_change_id) WHERE plan_change_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_checkout_sessions_addon
    ON checkout_sessions(addon_subscription_id) WHERE addon_subscription_id IS NOT NULL;

ALTER TABLE addon_subscriptions
    ADD COLUMN IF NOT EXISTS provider_checkout_id TEXT,
    ADD COLUMN IF NOT EXISTS paid_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS canceled_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT now();
ALTER TABLE addon_subscriptions DROP CONSTRAINT IF EXISTS chk_addon_subscriptions_status;
ALTER TABLE addon_subscriptions ADD CONSTRAINT chk_addon_subscriptions_status
    CHECK (status IN ('pending','active','canceled','failed'));
CREATE INDEX IF NOT EXISTS idx_addon_subscriptions_provider_checkout
    ON addon_subscriptions(provider_checkout_id) WHERE provider_checkout_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS subscription_plan_changes (
    id UUID PRIMARY KEY,
    subscription_id UUID NOT NULL REFERENCES subscriptions(id) ON DELETE CASCADE,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    old_plan TEXT NOT NULL,
    new_plan TEXT NOT NULL,
    period_start TIMESTAMPTZ NOT NULL,
    period_end TIMESTAMPTZ NOT NULL,
    calculated_amount NUMERIC(20,6) NOT NULL,
    currency TEXT NOT NULL,
    pricing_version INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    CONSTRAINT chk_subscription_plan_change_period CHECK (period_end > period_start),
    CONSTRAINT chk_subscription_plan_change_status CHECK (status IN ('pending','completed','failed'))
);
CREATE INDEX IF NOT EXISTS idx_subscription_plan_changes_tenant_created
    ON subscription_plan_changes(tenant_id,created_at DESC);

CREATE TABLE IF NOT EXISTS invoices (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    invoice_number TEXT NOT NULL,
    currency TEXT NOT NULL,
    subtotal NUMERIC(20,6) NOT NULL,
    tax NUMERIC(20,6) NOT NULL DEFAULT 0,
    total NUMERIC(20,6) NOT NULL,
    status TEXT NOT NULL,
    period_start TIMESTAMPTZ NOT NULL,
    period_end TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    paid_at TIMESTAMPTZ,
    CONSTRAINT uq_invoices_tenant_number UNIQUE (tenant_id,invoice_number),
    CONSTRAINT chk_invoices_status CHECK (status IN ('draft','open','paid','void','uncollectible')),
    CONSTRAINT chk_invoices_period CHECK (period_end > period_start),
    CONSTRAINT chk_invoices_tax CHECK (tax >= 0)
);
CREATE INDEX IF NOT EXISTS idx_invoices_tenant_created ON invoices(tenant_id,created_at DESC);

CREATE TABLE IF NOT EXISTS invoice_lines (
    id UUID PRIMARY KEY,
    invoice_id UUID NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
    line_type TEXT NOT NULL,
    description TEXT NOT NULL,
    quantity NUMERIC(20,6) NOT NULL,
    unit_price NUMERIC(20,6) NOT NULL,
    amount NUMERIC(20,6) NOT NULL,
    source_type TEXT NOT NULL,
    source_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT chk_invoice_lines_type CHECK (line_type IN ('subscription','addon','payg_usage','credit','refund','tax')),
    CONSTRAINT chk_invoice_lines_source_type CHECK (source_type IN ('usage_event','subscription_period','wallet_transaction')),
    CONSTRAINT chk_invoice_lines_quantity CHECK (quantity > 0),
    CONSTRAINT chk_invoice_lines_amount CHECK (amount = quantity * unit_price)
);
CREATE INDEX IF NOT EXISTS idx_invoice_lines_invoice ON invoice_lines(invoice_id);
CREATE INDEX IF NOT EXISTS idx_invoice_lines_source ON invoice_lines(source_type,source_id);

-- Strong traceability: the referenced source must exist AND belong to the
-- same tenant as the invoice. This prevents cross-tenant invoice forgery.
CREATE OR REPLACE FUNCTION validate_invoice_line_source()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE invoice_tenant UUID;
BEGIN
    SELECT tenant_id INTO invoice_tenant FROM invoices WHERE id=NEW.invoice_id;
    IF invoice_tenant IS NULL THEN
        RAISE EXCEPTION 'invoice does not exist' USING ERRCODE='foreign_key_violation';
    END IF;
    IF NEW.source_type='usage_event' THEN
        IF NOT EXISTS (SELECT 1 FROM api_usage_events e WHERE e.id=NEW.source_id AND e.tenant_id=invoice_tenant) THEN
            RAISE EXCEPTION 'invoice line usage event is missing or belongs to another tenant' USING ERRCODE='foreign_key_violation';
        END IF;
    ELSIF NEW.source_type='subscription_period' THEN
        IF NOT EXISTS (
            SELECT 1 FROM subscription_usage_periods p JOIN subscriptions s ON s.id=p.subscription_id
            WHERE p.id=NEW.source_id AND s.tenant_id=invoice_tenant
        ) THEN
            RAISE EXCEPTION 'invoice line subscription period is missing or belongs to another tenant' USING ERRCODE='foreign_key_violation';
        END IF;
    ELSIF NEW.source_type='wallet_transaction' THEN
        IF NOT EXISTS (
            SELECT 1 FROM wallet_transactions wt JOIN user_wallets w ON w.id=wt.wallet_id
            WHERE wt.id=NEW.source_id AND w.tenant_id=invoice_tenant
        ) THEN
            RAISE EXCEPTION 'invoice line wallet transaction is missing or belongs to another tenant' USING ERRCODE='foreign_key_violation';
        END IF;
    END IF;
    RETURN NEW;
END; $$;
DROP TRIGGER IF EXISTS trg_validate_invoice_line_source ON invoice_lines;
CREATE TRIGGER trg_validate_invoice_line_source
BEFORE INSERT OR UPDATE OF invoice_id,source_type,source_id ON invoice_lines
FOR EACH ROW EXECUTE FUNCTION validate_invoice_line_source();

COMMIT;
