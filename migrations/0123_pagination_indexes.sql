-- ============================================================================
-- PART 8 — Pagination indexes.
-- ============================================================================
--
-- Every tenant-scoped list endpoint now fetches a bounded page ordered
-- by `(created_at DESC, id DESC)`. Without a supporting index, each of
-- those queries is a full table scan + sort, so a large tenant could
-- still make the API slow even though the row count is capped.
--
-- This migration adds `(tenant_id, created_at DESC, id DESC)` indexes
-- to every table that gets cursor pagination and does not already have
-- an equivalent index. Indexes are idempotent (`IF NOT EXISTS`) so
-- re-running the migration on an existing database is safe.
--
-- Tables that ALREADY have a `(tenant_id, created_at, id)` index and
-- therefore do NOT appear here:
--   * audit_log                (migration 0103, idx_audit_log_tenant_created_id)
--   * financial_audit_log      (migration 0103, idx_financial_audit_log_tenant_created_id)
--   * invoices                 (migration 0080, idx_invoices_tenant_created)
--   * api_usage_events         (migration 0081, idx_api_usage_tenant_created)
--   * wallet_transactions      (migration 0078, idx_wallet_transactions_tenant_created)
--   * subscription_usage_periods (migration 0080)
-- ============================================================================

-- ----------------------------------------------------------------------------
-- 1) monitors — the biggest tenant collection on PAYG (no upper bound)
-- ----------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_monitors_tenant_created_id
    ON monitors (tenant_id, created_at DESC, id DESC);

-- User-scoped variant, because `find_all_for_user` is still used on the
-- legacy single-owner path.
CREATE INDEX IF NOT EXISTS idx_monitors_user_created_id
    ON monitors (user_id, created_at DESC, id DESC);

-- ----------------------------------------------------------------------------
-- 2) status_pages
-- ----------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_status_pages_tenant_created_id
    ON status_pages (tenant_id, created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_status_pages_user_created_id
    ON status_pages (user_id, created_at DESC, id DESC);

-- ----------------------------------------------------------------------------
-- 3) api_keys
-- ----------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_api_keys_tenant_created_id
    ON api_keys (tenant_id, created_at DESC, id DESC);

-- ----------------------------------------------------------------------------
-- 4) rum_sites
-- ----------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_rum_sites_tenant_created_id
    ON rum_sites (tenant_id, created_at DESC, id DESC);

-- ----------------------------------------------------------------------------
-- 5) support_tickets
-- ----------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_support_tickets_tenant_created_id
    ON support_tickets (tenant_id, created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_support_tickets_tenant_user_created_id
    ON support_tickets (tenant_id, created_by, created_at DESC, id DESC);

-- ----------------------------------------------------------------------------
-- 6) notification_channels
-- ----------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_notification_channels_tenant_created_id
    ON notification_channels (tenant_id, created_at DESC, id DESC);

-- The primary list path filters by monitor.
CREATE INDEX IF NOT EXISTS idx_notification_channels_monitor_created_id
    ON notification_channels (monitor_id, created_at DESC, id DESC);

-- ----------------------------------------------------------------------------
-- 7) maintenance_windows
-- ----------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_maintenance_windows_tenant_created_id
    ON maintenance_windows (tenant_id, created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_maintenance_windows_monitor_created_id
    ON maintenance_windows (monitor_id, created_at DESC, id DESC);

-- ----------------------------------------------------------------------------
-- 8) tenant_invitations
-- ----------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_tenant_invitations_tenant_created_id
    ON tenant_invitations (tenant_id, created_at DESC, id DESC);

-- ----------------------------------------------------------------------------
-- 9) escalation_policies
-- ----------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_escalation_policies_tenant_created_id
    ON escalation_policies (tenant_id, created_at DESC, id DESC);

-- ----------------------------------------------------------------------------
-- 10) proxy_check_jobs
-- ----------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_proxy_check_jobs_tenant_created_id
    ON proxy_check_jobs (tenant_id, created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_proxy_check_jobs_tenant_user_created_id
    ON proxy_check_jobs (tenant_id, user_id, created_at DESC, id DESC);

-- ----------------------------------------------------------------------------
-- 11) addon_subscriptions
-- ----------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_addon_subscriptions_tenant_created_id
    ON addon_subscriptions (tenant_id, created_at DESC, id DESC);

-- ----------------------------------------------------------------------------
-- 12) api_usage_events — already has `idx_api_usage_tenant_created`
--     (tenant_id, created_at DESC) but NOT the `id` tiebreaker required
--     for stable cursor pagination; add a matching extended index.
-- ----------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_api_usage_events_tenant_created_id
    ON api_usage_events (tenant_id, created_at DESC, id DESC);
