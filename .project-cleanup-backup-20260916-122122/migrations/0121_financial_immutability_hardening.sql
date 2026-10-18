-- ============================================================================
-- PART 4 — FINANCIAL DATABASE SECURITY: immutability hardening
-- ============================================================================
--
-- NOTE: this migration is intentionally written WITHOUT nested
-- dollar-quoting. sqlx's statement parser cannot reliably split a
-- migration that contains `DO $$ ... EXECUTE $tag$ ... $tag$ ... $$`,
-- and silently skips the inner DDL. Every function is therefore
-- created at the top level with `$$`, and every trigger at the top
-- level with a plain `ON <table>`.
--
-- Also no `BEGIN;`/`COMMIT;`: sqlx wraps each migration in its own
-- transaction, and a nested BEGIN would just emit a warning.
--
-- Invariant preserved:
--   NO attacker or application bug can rewrite financial history.
-- ============================================================================

-- ----------------------------------------------------------------------------
-- 1) billing_audit_log — append-only at the DB boundary.
-- ----------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION prevent_billing_audit_log_mutation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'billing_audit_log is append-only';
END;
$$;

DROP TRIGGER IF EXISTS trg_billing_audit_log_immutable ON billing_audit_log;
CREATE TRIGGER trg_billing_audit_log_immutable
BEFORE UPDATE OR DELETE ON billing_audit_log
FOR EACH ROW EXECUTE FUNCTION prevent_billing_audit_log_mutation();

-- ----------------------------------------------------------------------------
-- 2) pricing_configs — append-only.
-- ----------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION prevent_pricing_config_mutation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'pricing_configs are append-only; insert a new version instead';
END;
$$;

DROP TRIGGER IF EXISTS trg_pricing_configs_immutable ON pricing_configs;
CREATE TRIGGER trg_pricing_configs_immutable
BEFORE UPDATE OR DELETE ON pricing_configs
FOR EACH ROW EXECUTE FUNCTION prevent_pricing_config_mutation();

-- ----------------------------------------------------------------------------
-- 3) credit_reservations — state machine + no-delete.
-- ----------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION enforce_credit_reservation_transition()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.status = NEW.status THEN
        RETURN NEW;
    END IF;
    IF OLD.status IN ('CAPTURED','RELEASED') THEN
        RAISE EXCEPTION 'credit_reservations: status % is terminal', OLD.status
            USING ERRCODE = 'check_violation';
    END IF;
    IF NEW.status IN ('CAPTURED','RELEASED') AND OLD.status <> 'PENDING' THEN
        RAISE EXCEPTION 'credit_reservations: % can only be reached from PENDING (was %)',
            NEW.status, OLD.status
            USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_credit_reservations_transition ON credit_reservations;
CREATE TRIGGER trg_credit_reservations_transition
BEFORE UPDATE ON credit_reservations
FOR EACH ROW EXECUTE FUNCTION enforce_credit_reservation_transition();

CREATE OR REPLACE FUNCTION prevent_credit_reservation_delete()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'credit_reservations cannot be deleted';
END;
$$;

DROP TRIGGER IF EXISTS trg_credit_reservations_no_delete ON credit_reservations;
CREATE TRIGGER trg_credit_reservations_no_delete
BEFORE DELETE ON credit_reservations
FOR EACH ROW EXECUTE FUNCTION prevent_credit_reservation_delete();

-- ----------------------------------------------------------------------------
-- 4) wallet_top_up_sessions — state machine.
-- ----------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION enforce_top_up_session_transition()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.status = NEW.status THEN
        RETURN NEW;
    END IF;
    IF OLD.status <> 'PENDING' THEN
        RAISE EXCEPTION 'wallet_top_up_sessions: status % is terminal', OLD.status
            USING ERRCODE = 'check_violation';
    END IF;
    IF NEW.status NOT IN ('COMPLETED','FAILED') THEN
        RAISE EXCEPTION 'wallet_top_up_sessions: invalid transition PENDING -> %', NEW.status
            USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_wallet_top_up_sessions_transition ON wallet_top_up_sessions;
CREATE TRIGGER trg_wallet_top_up_sessions_transition
BEFORE UPDATE ON wallet_top_up_sessions
FOR EACH ROW EXECUTE FUNCTION enforce_top_up_session_transition();

-- ----------------------------------------------------------------------------
-- 5) payments — core fields immutable once inserted.
-- ----------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION enforce_payment_core_immutability()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.amount_units    <> OLD.amount_units
       OR NEW.currency         <> OLD.currency
       OR NEW.provider         <> OLD.provider
       OR NEW.idempotency_key  <> OLD.idempotency_key
       OR NEW.tenant_id        <> OLD.tenant_id
       OR NEW.created_at       <> OLD.created_at
    THEN
        RAISE EXCEPTION 'payments: core fields are immutable (amount/currency/provider/idempotency_key/tenant/created_at)'
            USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_payments_core_immutable ON payments;
CREATE TRIGGER trg_payments_core_immutable
BEFORE UPDATE ON payments
FOR EACH ROW EXECUTE FUNCTION enforce_payment_core_immutability();

-- ----------------------------------------------------------------------------
-- 6) Role-level REVOKE (idempotent).
-- ----------------------------------------------------------------------------
REVOKE UPDATE, DELETE, TRUNCATE ON billing_audit_log FROM pingvertex_app, pingvertex_worker;
REVOKE UPDATE, DELETE, TRUNCATE ON financial_audit_log FROM pingvertex_app, pingvertex_worker;
REVOKE UPDATE, DELETE, TRUNCATE ON audit_log FROM pingvertex_app, pingvertex_worker;
REVOKE UPDATE, DELETE, TRUNCATE ON pricing_audit_logs FROM pingvertex_app, pingvertex_worker;
REVOKE UPDATE, DELETE, TRUNCATE ON payg_retired_operations FROM pingvertex_app, pingvertex_worker;
REVOKE UPDATE, DELETE, TRUNCATE ON billing_webhook_idempotency FROM pingvertex_app, pingvertex_worker;

REVOKE INSERT, UPDATE, DELETE, TRUNCATE ON pricing_configs FROM pingvertex_app, pingvertex_worker;
GRANT SELECT ON pricing_configs TO pingvertex_app, pingvertex_worker;

REVOKE INSERT, UPDATE, DELETE, TRUNCATE ON payg_pricing_configs FROM pingvertex_app, pingvertex_worker;
GRANT SELECT ON payg_pricing_configs TO pingvertex_app, pingvertex_worker;

REVOKE DELETE, TRUNCATE ON credit_reservations FROM pingvertex_app, pingvertex_worker;

-- ----------------------------------------------------------------------------
-- 7) promotional_credits — conditional. Uses a plain string inside
--    `EXECUTE` (no nested dollar-quote) so sqlx can parse it.
-- ----------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION enforce_promotional_credit_immutability()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.original_amount_units <> OLD.original_amount_units
       OR NEW.tenant_id    <> OLD.tenant_id
       OR NEW.wallet_id    <> OLD.wallet_id
       OR NEW.expires_at   <> OLD.expires_at
       OR NEW.idempotency_key <> OLD.idempotency_key
    THEN
        RAISE EXCEPTION 'promotional_credits: core fields are immutable'
            USING ERRCODE = 'check_violation';
    END IF;
    IF NEW.remaining_amount_units > OLD.remaining_amount_units THEN
        RAISE EXCEPTION 'promotional_credits: remaining_amount_units cannot increase'
            USING ERRCODE = 'check_violation';
    END IF;
    IF NEW.remaining_amount_units < 0 THEN
        RAISE EXCEPTION 'promotional_credits: remaining_amount_units cannot be negative'
            USING ERRCODE = 'check_violation';
    END IF;
    IF NEW.remaining_amount_units > NEW.original_amount_units THEN
        RAISE EXCEPTION 'promotional_credits: remaining exceeds original'
            USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
END;
$$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = current_schema() AND table_name = 'promotional_credits'
    ) THEN
        EXECUTE 'DROP TRIGGER IF EXISTS trg_promotional_credits_immutable ON promotional_credits';
        EXECUTE 'CREATE TRIGGER trg_promotional_credits_immutable BEFORE UPDATE ON promotional_credits FOR EACH ROW EXECUTE FUNCTION enforce_promotional_credit_immutability()';
    END IF;
END;
$$;
