-- B15 — Billing Audit Log (append-only).
--
-- Records billing-relevant webhook events. Unlike the general audit_log
-- table, actor_user_id is NOT required here: events originate from the
-- billing provider, not from an authenticated user action.
CREATE TABLE billing_audit_log (
    id UUID PRIMARY KEY,
    provider_event_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    subscription_id UUID REFERENCES subscriptions(id) ON DELETE SET NULL,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT chk_billing_audit_log_event_type
        CHECK (event_type IN (
            'payment_succeeded',
            'payment_failed',
            'subscription_renewed',
            'subscription_canceled'
        ))
);

CREATE INDEX idx_billing_audit_log_tenant_id
    ON billing_audit_log (tenant_id);

CREATE INDEX idx_billing_audit_log_provider_event_id
    ON billing_audit_log (provider_event_id);
