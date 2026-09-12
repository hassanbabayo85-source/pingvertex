BEGIN;

-- P1/P0 release gate: bounded audit search indexes. Historical rows remain immutable.
CREATE INDEX IF NOT EXISTS idx_audit_log_tenant_created_id
    ON audit_log(tenant_id, created_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_audit_log_actor_created_id
    ON audit_log(actor_user_id, created_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_audit_log_action_created_id
    ON audit_log(action, created_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_audit_log_target_created_id
    ON audit_log(target_id, created_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_financial_audit_log_tenant_created_id
    ON financial_audit_log(tenant_id, created_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_financial_audit_log_resource_created_id
    ON financial_audit_log(resource, created_at DESC, id DESC);

-- Keep all audit history append-only, including all immutable identity fields.
CREATE OR REPLACE FUNCTION prevent_audit_log_mutation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'audit logs are append-only';
END;
$$;
DROP TRIGGER IF EXISTS trg_audit_log_append_only ON audit_log;
CREATE TRIGGER trg_audit_log_append_only BEFORE UPDATE OR DELETE ON audit_log
FOR EACH ROW EXECUTE FUNCTION prevent_audit_log_mutation();
DROP TRIGGER IF EXISTS trg_financial_audit_log_append_only ON financial_audit_log;
CREATE TRIGGER trg_financial_audit_log_append_only BEFORE UPDATE OR DELETE ON financial_audit_log
FOR EACH ROW EXECUTE FUNCTION prevent_audit_log_mutation();
DROP TRIGGER IF EXISTS trg_pricing_audit_log_append_only ON pricing_audit_logs;
CREATE TRIGGER trg_pricing_audit_log_append_only BEFORE UPDATE OR DELETE ON pricing_audit_logs
FOR EACH ROW EXECUTE FUNCTION prevent_audit_log_mutation();

-- Explicit production billing contract: provider identity is never optional on
-- a settled payment/top-up attempt. Legacy rows remain readable; new rows are
-- enforced by the existing unique provider-payment indexes and runtime checks.
-- Auto-top-up reconciliation state is represented by the existing immutable
-- attempt status (PENDING/PROCESSING/SUCCEEDED/FAILED/REQUIRES_RECONCILIATION).
-- Keep the index aligned with the worker's actual query path.
CREATE INDEX IF NOT EXISTS idx_wallet_auto_top_up_reconciliation_status
    ON wallet_auto_top_up_attempts(status, attempted_at ASC)
    WHERE status IN ('PENDING','PROCESSING','REQUIRES_RECONCILIATION');

-- Fast wallet/usage reconciliation joins.
CREATE INDEX IF NOT EXISTS idx_wallet_transactions_tenant_usage_event
    ON wallet_transactions(tenant_id, usage_event_id)
    WHERE usage_event_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_api_usage_events_tenant_execution
    ON api_usage_events(tenant_id, execution_id)
    WHERE execution_id IS NOT NULL;


-- Read-only reconciliation contract. This gives operators one stable shape for
-- external payments, wallet movements and usage charges without duplicating
-- mutable financial state.
CREATE OR REPLACE VIEW payg_financial_event_contract AS
SELECT
    p.id AS event_id,
    p.idempotency_key AS request_id,
    p.tenant_id,
    w.id AS wallet_id,
    p.amount_units AS amount,
    p.currency,
    p.provider AS provider_id,
    p.provider_payment_id AS provider_reference,
    p.status,
    p.created_at,
    'payment'::TEXT AS event_type
FROM payments p
LEFT JOIN user_wallets w ON w.tenant_id = p.tenant_id
UNION ALL
SELECT
    wt.id AS event_id,
    COALESCE(r.request_id, wt.idempotency_key),
    w.tenant_id,
    wt.wallet_id,
    ABS(wt.amount_units) AS amount,
    w.currency,
    NULL::TEXT AS provider_id,
    CASE WHEN wt.reference_type = 'payment' THEN wt.reference_id ELSE NULL END AS provider_reference,
    'SETTLED'::TEXT AS status,
    wt.created_at,
    CASE WHEN wt.transaction_type = 'DEBIT' THEN 'wallet_debit' ELSE 'wallet_credit' END AS event_type
FROM wallet_transactions wt
JOIN user_wallets w ON w.id = wt.wallet_id
LEFT JOIN credit_reservations r
  ON r.id::TEXT = wt.reference_id
 AND wt.reference_type = 'credit_reservation'
UNION ALL
SELECT
    e.id AS event_id,
    e.request_id,
    e.tenant_id,
    w.id AS wallet_id,
    e.charge_units AS amount,
    e.currency,
    NULL::TEXT AS provider_id,
    NULL::TEXT AS provider_reference,
    e.status,
    e.created_at,
    'usage'::TEXT AS event_type
FROM api_usage_events e
LEFT JOIN user_wallets w ON w.tenant_id = e.tenant_id;

COMMIT;
