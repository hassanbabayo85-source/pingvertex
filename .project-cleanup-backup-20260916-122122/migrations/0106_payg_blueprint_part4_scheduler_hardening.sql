BEGIN;

-- PAYG Blueprint Part 4 (Sections 35-49): scheduler database hardening.
-- The runtime already uses row-level claiming with FOR UPDATE SKIP LOCKED;
-- these indexes keep the claim/recovery paths bounded as tenant/monitor
-- counts grow. No billing semantics or historical rows are rewritten.

CREATE INDEX IF NOT EXISTS idx_monitors_scheduler_due_payg
    ON monitors (next_due_at, tenant_id, id)
    WHERE enabled = TRUE
      AND lease_expires_at IS NULL
      AND kind <> 'heartbeat'
      AND (kind <> 'http' OR cardinality(regions) <= 1);

CREATE INDEX IF NOT EXISTS idx_monitors_scheduler_active_leases_tenant
    ON monitors (tenant_id, lease_expires_at)
    WHERE lease_owner IS NOT NULL
      AND lease_expires_at IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_one_time_check_queued_created
    ON one_time_check_requests (created_at, id)
    WHERE status = 'QUEUED';

CREATE INDEX IF NOT EXISTS idx_one_time_check_running_expiry
    ON one_time_check_requests (lease_expires_at, id)
    WHERE status = 'RUNNING';

CREATE INDEX IF NOT EXISTS idx_agent_check_leases_monitor_region_active
    ON agent_check_leases (monitor_id, region, lease_expires_at)
    WHERE completed_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_credit_reservations_pending_expiry_wallet
    ON credit_reservations (expires_at, wallet_id, id)
    WHERE status = 'PENDING';

CREATE INDEX IF NOT EXISTS idx_monitor_billing_blocks_tenant_monitor
    ON monitor_billing_blocks (tenant_id, monitor_id);

COMMIT;
