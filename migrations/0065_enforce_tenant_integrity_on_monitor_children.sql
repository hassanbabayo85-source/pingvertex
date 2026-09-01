-- SEC-P0-3 (2026-09 security review): "Cross-Tenant Parent/Child
-- Relationships". A plain `monitor_id UUID REFERENCES monitors(id)`
-- foreign key only guarantees the referenced monitor exists — it says
-- nothing about which tenant it belongs to. Every write path already
-- checks `monitor.tenant_id() == actor's tenant_id` in the application
-- layer (see CreateApiFlowUseCase, CreateMaintenanceWindowUseCase,
-- CreateEscalationPolicyUseCase, CreateNotificationChannelUseCase), but
-- that leaves this invariant enforced in exactly one place: a single
-- missed check in any current or future write path is a silent
-- cross-tenant data leak with no database-level backstop.
--
-- This migration makes the invariant self-enforcing at the schema level:
-- a child row's tenant_id must always match its parent monitor's
-- tenant_id, and Postgres — not application code — is what guarantees
-- it from here on.
--
-- If this migration fails with a foreign key violation, it means
-- inconsistent cross-tenant rows already exist in the target
-- environment; that data must be investigated and repaired before this
-- migration can be applied there.

-- A composite foreign key must reference a UNIQUE (or PK) constraint on
-- the parent columns. `monitors.id` is already the primary key, so this
-- is a redundant-but-required superset unique constraint that makes
-- (id, tenant_id) referenceable.
ALTER TABLE monitors
    ADD CONSTRAINT uq_monitors_id_tenant_id UNIQUE (id, tenant_id);

-- ---------------------------------------------------------------------
-- api_flows
-- ---------------------------------------------------------------------
ALTER TABLE api_flows
    DROP CONSTRAINT fk_api_flows_monitor;

ALTER TABLE api_flows
    ADD CONSTRAINT fk_api_flows_monitor_tenant
    FOREIGN KEY (monitor_id, tenant_id)
    REFERENCES monitors (id, tenant_id)
    ON DELETE CASCADE;

-- ---------------------------------------------------------------------
-- maintenance_windows
-- ---------------------------------------------------------------------
ALTER TABLE maintenance_windows
    DROP CONSTRAINT maintenance_windows_monitor_id_fkey;

ALTER TABLE maintenance_windows
    ADD CONSTRAINT fk_maintenance_windows_monitor_tenant
    FOREIGN KEY (monitor_id, tenant_id)
    REFERENCES monitors (id, tenant_id)
    ON DELETE CASCADE;

-- ---------------------------------------------------------------------
-- escalation_policies
--
-- monitor_id is nullable here (a NULL monitor_id is a tenant-global
-- policy, not tied to any single monitor). A composite FK with
-- MATCH SIMPLE (Postgres's default match type) is automatically
-- satisfied whenever ANY referencing column is NULL, so global policies
-- are correctly left unconstrained by this FK while monitor-scoped
-- policies are still checked.
-- ---------------------------------------------------------------------
ALTER TABLE escalation_policies
    ADD CONSTRAINT fk_escalation_policies_monitor_tenant
    FOREIGN KEY (monitor_id, tenant_id)
    REFERENCES monitors (id, tenant_id)
    ON DELETE CASCADE;

-- ---------------------------------------------------------------------
-- notification_channels
-- ---------------------------------------------------------------------
ALTER TABLE notification_channels
    DROP CONSTRAINT notification_channels_monitor_id_fkey;

ALTER TABLE notification_channels
    ADD CONSTRAINT fk_notification_channels_monitor_tenant
    FOREIGN KEY (monitor_id, tenant_id)
    REFERENCES monitors (id, tenant_id)
    ON DELETE CASCADE;

-- ---------------------------------------------------------------------
-- NOTE for follow-up (not done in this migration):
--
-- `incidents` and `diagnostic_reports` currently have NO tenant_id
-- column of their own at all — tenant scoping for them is resolved
-- entirely by joining through `monitors`/`checks`. Adding a composite FK
-- there requires first backfilling a `tenant_id` column on those tables,
-- which is a larger, higher-risk migration (large/partitioned tables)
-- that deserves its own review and a maintenance window, so it is
-- deliberately NOT bundled into this fix. Until that lands, continue to
-- rely on the existing application-layer `monitor.tenant_id() ==
-- actor_tenant_id` checks for those two tables specifically.
-- ---------------------------------------------------------------------
