-- Append-only audit log for tenant/user actions.
--
-- We deliberately do NOT add `REVOKE UPDATE, DELETE` on this table.
-- Reason: docker-compose.yml connects as the `postgres` superuser only,
-- with no separate least-privileged application DB role. Because a
-- superuser can bypass `REVOKE` anyway, append-only enforcement currently
-- lives exclusively in the application/domain layer.
--
-- If a distinct least-privileged DB role is introduced in the future,
-- `REVOKE UPDATE, DELETE ON audit_log FROM <app_role>;` should be added
-- here immediately, so the database also enforces append-only behavior.
CREATE TABLE audit_log (
    id UUID PRIMARY KEY,
    actor_user_id UUID NOT NULL REFERENCES users(id),
    tenant_id UUID REFERENCES tenants(id),
    action TEXT NOT NULL,
    target_id TEXT NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
