-- B35 — PostgreSQL Performance Audit.

-- Fills specific gaps found during the audit. This is performance-only:
-- no data model or business-rule changes.

-- audit_log is tenant-scoped in application code (`list_by_tenant`) but
-- had no index on tenant_id. Also covers the append-only history view
-- sorted by created_at.
CREATE INDEX IF NOT EXISTS idx_audit_log_tenant_id
    ON audit_log (tenant_id);

CREATE INDEX IF NOT EXISTS idx_audit_log_actor_user_created_at
    ON audit_log (actor_user_id, created_at DESC);

-- `IncidentRepository::list_open_unacknowledged` is used by the
-- escalation worker and filters exactly these two NULL conditions while
-- ordering by started_at. The old `idx_incidents_resolved_at` could only
-- help with the `resolved_at IS NULL` half of that predicate.
CREATE INDEX IF NOT EXISTS idx_incidents_open_unacknowledged
    ON incidents (started_at ASC)
    WHERE resolved_at IS NULL AND acknowledged_at IS NULL;

-- `MaintenanceWindowRepository::list_active_by_monitor` filters by
-- monitor_id plus a time window. Previously there was no monitor_id
-- index on this table (only tenant_id), so every check-result evaluation
-- for a monitor could force a table scan.
CREATE INDEX IF NOT EXISTS idx_maintenance_windows_monitor_starts
    ON maintenance_windows (monitor_id, starts_at DESC);

-- `TenantRepository::find_by_owner_user_id` had no index.
CREATE INDEX IF NOT EXISTS idx_tenants_owner_user_id
    ON tenants (owner_user_id);

-- Known B34 issue: `find_all_referencing_monitor` uses the JSONB
-- containment operator `monitor_ids @> $1`. A GIN index turns that from
-- a sequential scan into an index scan. Performance-only follow-up.
CREATE INDEX IF NOT EXISTS idx_status_pages_monitor_ids_gin
    ON status_pages USING GIN (monitor_ids);
