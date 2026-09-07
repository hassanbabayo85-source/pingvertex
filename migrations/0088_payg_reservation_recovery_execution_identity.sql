-- PAYG remediation: crash-safe reservation expiry and explicit scheduler execution identity.
BEGIN;
ALTER TABLE monitors ADD COLUMN IF NOT EXISTS lease_execution_id UUID;
CREATE INDEX IF NOT EXISTS idx_monitors_lease_execution_id ON monitors (lease_execution_id) WHERE lease_execution_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_credit_reservations_pending_expiry ON credit_reservations(expires_at, wallet_id) WHERE status='PENDING';
COMMIT;
