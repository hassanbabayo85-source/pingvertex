BEGIN;

-- B15 originally omitted provider-confirmed refund events from the event
-- constraint even though the domain/webhook handler supports them. Repair
-- the constraint so refund webhooks can be durably recorded.
ALTER TABLE billing_audit_log
    DROP CONSTRAINT IF EXISTS chk_billing_audit_log_event_type;
ALTER TABLE billing_audit_log
    ADD CONSTRAINT chk_billing_audit_log_event_type
    CHECK (event_type IN (
        'payment_succeeded',
        'payment_failed',
        'subscription_renewed',
        'subscription_canceled',
        'payment_refunded'
    ));

-- Worker 8: durable financial audit trail and reconciliation reports.
-- Monetary values remain integer smallest-currency units in payments/wallets;
-- report payloads are JSON for diagnostics only.
CREATE TABLE IF NOT EXISTS financial_audit_log (
    id UUID PRIMARY KEY,
    actor TEXT NOT NULL,
    tenant_id UUID REFERENCES tenants(id) ON DELETE SET NULL,
    action TEXT NOT NULL,
    resource TEXT NOT NULL,
    resource_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb
);
CREATE INDEX IF NOT EXISTS idx_financial_audit_log_tenant_time
    ON financial_audit_log(tenant_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_financial_audit_log_action_time
    ON financial_audit_log(action, created_at DESC);

CREATE OR REPLACE FUNCTION prevent_financial_audit_log_mutation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'financial_audit_log is append-only';
END;
$$;
DROP TRIGGER IF EXISTS trg_financial_audit_log_immutable ON financial_audit_log;
CREATE TRIGGER trg_financial_audit_log_immutable
BEFORE UPDATE OR DELETE ON financial_audit_log
FOR EACH ROW EXECUTE FUNCTION prevent_financial_audit_log_mutation();
REVOKE UPDATE, DELETE ON financial_audit_log FROM PUBLIC;
REVOKE ALL ON financial_audit_log FROM PUBLIC;

-- Provider payment references must be unique. NULL remains allowed while a
-- payment is still pending and has no provider-side payment id.
CREATE UNIQUE INDEX IF NOT EXISTS uq_payments_provider_payment_id
    ON payments(provider, provider_payment_id)
    WHERE provider_payment_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_payments_provider_payment
    ON payments(provider_payment_id)
    WHERE provider_payment_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS billing_reconciliation_reports (
    id UUID PRIMARY KEY,
    run_started_at TIMESTAMPTZ NOT NULL,
    run_finished_at TIMESTAMPTZ NOT NULL,
    window_start TIMESTAMPTZ NOT NULL,
    window_end TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('ok','mismatch','error')),
    mismatch_counts JSONB NOT NULL DEFAULT '{}'::jsonb,
    report JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_billing_reconciliation_reports_created
    ON billing_reconciliation_reports(created_at DESC);

COMMIT;
