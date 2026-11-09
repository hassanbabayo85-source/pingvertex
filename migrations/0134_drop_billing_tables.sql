-- 0134_drop_billing_tables.sql
--
-- Final billing-surface removal. Drops every remaining table that
-- existed only to serve the PAYG / subscription / wallet billing model,
-- removes the last two residue columns from non-billing tables, drops
-- the scheduler's billing-block gate, and cleans up orphaned functions
-- and views from earlier phases.
--
-- SAFETY PRECONDITIONS (verified against a 133-migration test DB):
--   * No FK from any non-billing table into any billing table.
--     (grep over pg_constraint for contype='f' with confrelid in
--     billing set, conrelid NOT IN billing set returned 0 rows.)
--   * No billing column on monitors / tenants / checks / incidents /
--     users / any core monitoring table, except the two residue
--     columns handled below.
--   * No Rust code references any of these tables after phase-3-2
--     (the last Rust-side reference, monitor_billing_blocks, is
--     removed in this same commit).
--   * The one view (payg_financial_event_contract) and the nine
--     trigger-protection functions are only referenced by the tables
--     being dropped.
--
-- NON-BILLING TABLES DROPPED IN PART (columns removed, table kept):
--   * agent_check_leases.billing_reserved    -- residue from phase-1a
--     (Rust struct field removed in 0f80581, but the column was left
--     behind to keep the Postgres INSERT valid; the INSERT is
--     rewritten in this same commit)
--   * test_access_grants.plan_tier           -- residue from phase-1b-2c
--     (the field held the string 'enterprise' for QA test accounts;
--     the resolver no longer reads tier from grants, so this column
--     is inert)
--
-- TABLES DROPPED:
--   Monitor-side scheduler gate:
--     * monitor_billing_blocks       (residue from phase-1b-2; the
--       scheduler's NOT EXISTS filter is removed in this commit)
--
--   Subscription / plan / pricing:
--     * subscriptions, plans, plan_entitlements, plan_prices,
--       subscription_billing_records, subscription_usage_periods,
--       subscription_plan_changes, checkout_sessions,
--       addon_subscriptions
--
--   Invoicing / payment / audit:
--     * invoices, invoice_lines, payments, billing_audit_log,
--       financial_audit_log, billing_reconciliation_reports,
--       billing_webhook_idempotency
--
--   Wallet / credit:
--     * user_wallets, wallet_settings, wallet_transactions,
--       wallet_auto_top_up_attempts, wallet_auto_top_up_claims,
--       wallet_refund_requests, wallet_notifications,
--       wallet_top_up_sessions, credit_reservations,
--       promotional_credits
--
--   PAYG metering / pricing:
--     * api_usage_events, payg_pricing_configs, payg_retired_operations
--
-- EXPLICITLY PRESERVED (do not touch):
--   * tenants, users, monitors, checks (and partitions), incidents,
--     agent_check_leases (minus one column), agent_check_claims,
--     api_flows, api_flow_steps, api_keys, audit_log, backup_codes,
--     diagnostic_reports, email_verification_tokens,
--     escalation_policies, help_articles, help_categories,
--     maintenance_windows, monitor_assertions,
--     monitor_bulk_import_jobs, monitor_bulk_import_rows,
--     monitor_interval_policies, notification_channels,
--     notification_outbox, oauth_identities, one_time_check_requests,
--     one_time_check_executions, proxy_check_jobs, proxy_entries,
--     recurring_proxy_checks, recurring_proxy_monitors,
--     reset_password_tokens, rum_* tables, status_pages, subscribers,
--     support_messages, support_tickets, tenant_alert_allowances,
--     tenant_invitations, tenant_memberships,
--     tenant_retention_settings, test_access_grants (minus one column)
--
-- Historical migrations (0001..0133) are NOT modified. SQLx verifies
-- migration checksums; forward-only.

BEGIN;

-- 1. Orphaned trigger-protection functions from earlier phases.
--    PostgreSQL leaves function bodies in place when their tables are
--    dropped (DROP TABLE CASCADE only removes the triggers). All nine
--    are function-only here; no trigger will reference them after the
--    DROP TABLE statements below.
DROP FUNCTION IF EXISTS prevent_pricing_config_mutation() CASCADE;
DROP FUNCTION IF EXISTS prevent_payg_pricing_mutation() CASCADE;
DROP FUNCTION IF EXISTS prevent_ambiguous_payg_pricing_overlap() CASCADE;
DROP FUNCTION IF EXISTS prevent_wallet_transaction_mutation() CASCADE;
DROP FUNCTION IF EXISTS prevent_billing_audit_log_mutation() CASCADE;
DROP FUNCTION IF EXISTS validate_invoice_line_source() CASCADE;
DROP FUNCTION IF EXISTS enforce_credit_reservation_transition() CASCADE;
DROP FUNCTION IF EXISTS prevent_credit_reservation_delete() CASCADE;
DROP FUNCTION IF EXISTS enforce_promotional_credit_immutability() CASCADE;

-- 2. Views that reference billing tables.
DROP VIEW IF EXISTS payg_financial_event_contract CASCADE;

-- 3. Residue columns on non-billing tables.
ALTER TABLE agent_check_leases DROP COLUMN IF EXISTS billing_reserved;
ALTER TABLE test_access_grants  DROP COLUMN IF EXISTS plan_tier;

-- 4. Monitor-side scheduler gate.
DROP TABLE IF EXISTS monitor_billing_blocks CASCADE;

-- 5. Subscription / plan / pricing surface.
DROP TABLE IF EXISTS subscription_billing_records CASCADE;
DROP TABLE IF EXISTS subscription_usage_periods  CASCADE;
DROP TABLE IF EXISTS subscription_plan_changes   CASCADE;
DROP TABLE IF EXISTS checkout_sessions           CASCADE;
DROP TABLE IF EXISTS subscriptions               CASCADE;
DROP TABLE IF EXISTS plan_prices                 CASCADE;
DROP TABLE IF EXISTS plan_entitlements           CASCADE;
DROP TABLE IF EXISTS plans                       CASCADE;
DROP TABLE IF EXISTS addon_subscriptions         CASCADE;

-- 6. Invoicing / payment / audit surface.
DROP TABLE IF EXISTS invoice_lines                      CASCADE;
DROP TABLE IF EXISTS invoices                           CASCADE;
DROP TABLE IF EXISTS payments                           CASCADE;
DROP TABLE IF EXISTS billing_reconciliation_reports     CASCADE;
DROP TABLE IF EXISTS billing_webhook_idempotency        CASCADE;
DROP TABLE IF EXISTS billing_audit_log                  CASCADE;
DROP TABLE IF EXISTS financial_audit_log                CASCADE;

-- 7. Wallet / credit surface.
DROP TABLE IF EXISTS wallet_transactions             CASCADE;
DROP TABLE IF EXISTS wallet_top_up_sessions           CASCADE;
DROP TABLE IF EXISTS wallet_auto_top_up_claims        CASCADE;
DROP TABLE IF EXISTS wallet_auto_top_up_attempts      CASCADE;
DROP TABLE IF EXISTS wallet_refund_requests           CASCADE;
DROP TABLE IF EXISTS wallet_notifications             CASCADE;
DROP TABLE IF EXISTS wallet_settings                  CASCADE;
DROP TABLE IF EXISTS user_wallets                     CASCADE;
DROP TABLE IF EXISTS credit_reservations              CASCADE;
DROP TABLE IF EXISTS promotional_credits              CASCADE;

-- 8. PAYG metering / pricing surface.
DROP TABLE IF EXISTS api_usage_events          CASCADE;
DROP TABLE IF EXISTS payg_pricing_configs      CASCADE;
DROP TABLE IF EXISTS payg_retired_operations   CASCADE;

COMMIT;
