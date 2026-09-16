-- Allow the `usage_event_id` NULL -> non-NULL transition on
-- wallet_transactions, so that `PostgresWalletRepository::capture()`
-- can link a freshly-INSERTed debit row to the usage event that
-- produced it (see crates/infra/src/repositories/postgres_wallet_repository.rs
-- around the `if let Some(event) = usage_event` block in `capture`).
--
-- The previous trigger unconditionally rejected EVERY UPDATE on the
-- table. That correctly protected amount_units / balance_after_units /
-- idempotency_key from mutation, but it also blocked this single
-- legitimate linking step, which silently broke every PAYG capture
-- and every proxy check (and every other metered operation).
--
-- The relaxed function:
--   * still rejects every DELETE,
--   * still rejects every UPDATE that changes ANY business column,
--   * allows ONLY the specific transition usage_event_id: NULL -> value
--     on an otherwise byte-identical row.
--
-- This preserves the financial-ledger immutability invariant: the
-- amount, balance, type, wallet, tenant, reference and idempotency
-- key of a booked transaction can never change. Only the "which usage
-- event caused this debit" link is allowed to be filled in once.
BEGIN;

CREATE OR REPLACE FUNCTION prevent_wallet_transaction_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'wallet_transactions are immutable';
    END IF;

    IF TG_OP = 'UPDATE' THEN
        IF OLD.usage_event_id IS NULL
           AND NEW.usage_event_id IS NOT NULL
           AND OLD.id = NEW.id
           AND OLD.wallet_id = NEW.wallet_id
           AND OLD.tenant_id = NEW.tenant_id
           AND OLD.transaction_type = NEW.transaction_type
           AND OLD.amount_units = NEW.amount_units
           AND OLD.balance_after_units = NEW.balance_after_units
           AND OLD.idempotency_key = NEW.idempotency_key
           AND COALESCE(OLD.reference_type, '') = COALESCE(NEW.reference_type, '')
           AND COALESCE(OLD.reference_id, '')   = COALESCE(NEW.reference_id, '')
           AND COALESCE(OLD.description, '')    = COALESCE(NEW.description, '')
        THEN
            RETURN NEW;
        END IF;
        RAISE EXCEPTION 'wallet_transactions are immutable';
    END IF;

    RETURN NEW;
END;
$$;

COMMIT;
