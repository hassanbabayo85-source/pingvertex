-- Add tenant_id to every tenant-isolated resource table.
--
-- gen_random_uuid() is built into PostgreSQL 16 core, so no
-- CREATE EXTENSION pgcrypto is needed here.

-- ---------------------------------------------------------------------
-- 1. Add nullable tenant_id columns first, so all tables can be
--    backfilled before the NOT NULL constraints are applied.
-- ---------------------------------------------------------------------
ALTER TABLE monitors
    ADD COLUMN tenant_id UUID;

ALTER TABLE api_flows
    ADD COLUMN tenant_id UUID;

ALTER TABLE proxy_check_jobs
    ADD COLUMN tenant_id UUID;

ALTER TABLE notification_channels
    ADD COLUMN tenant_id UUID;

ALTER TABLE status_pages
    ADD COLUMN tenant_id UUID;

ALTER TABLE escalation_policies
    ADD COLUMN tenant_id UUID;

ALTER TABLE maintenance_windows
    ADD COLUMN tenant_id UUID;

-- ---------------------------------------------------------------------
-- 2. Create one default tenant for every existing user.
--    `username` is nullable, so fall back to `email` for the tenant name.
-- ---------------------------------------------------------------------
INSERT INTO tenants (id, name, owner_user_id, created_at)
SELECT
    gen_random_uuid(),
    COALESCE(users.username, users.email),
    users.id,
    users.created_at
FROM users;

-- ---------------------------------------------------------------------
-- 3. Make each user the Owner of their default tenant.
-- ---------------------------------------------------------------------
INSERT INTO tenant_memberships (tenant_id, user_id, role, invited_at, joined_at)
SELECT
    tenants.id,
    tenants.owner_user_id,
    'Owner',
    tenants.created_at,
    tenants.created_at
FROM tenants;

-- ---------------------------------------------------------------------
-- 4. Backfill tenant_id for resources that already carry user_id.
-- ---------------------------------------------------------------------
UPDATE monitors m
SET tenant_id = t.id
FROM tenants t
WHERE t.owner_user_id = m.user_id;

UPDATE proxy_check_jobs pcj
SET tenant_id = t.id
FROM tenants t
WHERE t.owner_user_id = pcj.user_id;

UPDATE notification_channels nc
SET tenant_id = t.id
FROM tenants t
WHERE t.owner_user_id = nc.user_id;

UPDATE status_pages sp
SET tenant_id = t.id
FROM tenants t
WHERE t.owner_user_id = sp.user_id;

UPDATE escalation_policies ep
SET tenant_id = t.id
FROM tenants t
WHERE t.owner_user_id = ep.user_id;

-- ---------------------------------------------------------------------
-- 5. Backfill tenant_id for resources that only carry monitor_id.
--    They are resolved through monitors.user_id -> tenants.owner_user_id.
-- ---------------------------------------------------------------------
UPDATE api_flows af
SET tenant_id = t.id
FROM tenants t
JOIN monitors m ON m.user_id = t.owner_user_id
WHERE af.monitor_id = m.id;

UPDATE maintenance_windows mw
SET tenant_id = t.id
FROM tenants t
JOIN monitors m ON m.user_id = t.owner_user_id
WHERE mw.monitor_id = m.id;

-- ---------------------------------------------------------------------
-- 6. Now enforce NOT NULL, FK, and indexes on all seven tables.
-- ---------------------------------------------------------------------
ALTER TABLE monitors ALTER COLUMN tenant_id SET NOT NULL;
ALTER TABLE monitors
    ADD CONSTRAINT fk_monitors_tenant
    FOREIGN KEY (tenant_id) REFERENCES tenants(id);
CREATE INDEX idx_monitors_tenant_id ON monitors (tenant_id);

ALTER TABLE api_flows ALTER COLUMN tenant_id SET NOT NULL;
ALTER TABLE api_flows
    ADD CONSTRAINT fk_api_flows_tenant
    FOREIGN KEY (tenant_id) REFERENCES tenants(id);
CREATE INDEX idx_api_flows_tenant_id ON api_flows (tenant_id);

ALTER TABLE proxy_check_jobs ALTER COLUMN tenant_id SET NOT NULL;
ALTER TABLE proxy_check_jobs
    ADD CONSTRAINT fk_proxy_check_jobs_tenant
    FOREIGN KEY (tenant_id) REFERENCES tenants(id);
CREATE INDEX idx_proxy_check_jobs_tenant_id ON proxy_check_jobs (tenant_id);

ALTER TABLE notification_channels ALTER COLUMN tenant_id SET NOT NULL;
ALTER TABLE notification_channels
    ADD CONSTRAINT fk_notification_channels_tenant
    FOREIGN KEY (tenant_id) REFERENCES tenants(id);
CREATE INDEX idx_notification_channels_tenant_id ON notification_channels (tenant_id);

ALTER TABLE status_pages ALTER COLUMN tenant_id SET NOT NULL;
ALTER TABLE status_pages
    ADD CONSTRAINT fk_status_pages_tenant
    FOREIGN KEY (tenant_id) REFERENCES tenants(id);
CREATE INDEX idx_status_pages_tenant_id ON status_pages (tenant_id);

ALTER TABLE escalation_policies ALTER COLUMN tenant_id SET NOT NULL;
ALTER TABLE escalation_policies
    ADD CONSTRAINT fk_escalation_policies_tenant
    FOREIGN KEY (tenant_id) REFERENCES tenants(id);
CREATE INDEX idx_escalation_policies_tenant_id ON escalation_policies (tenant_id);

ALTER TABLE maintenance_windows ALTER COLUMN tenant_id SET NOT NULL;
ALTER TABLE maintenance_windows
    ADD CONSTRAINT fk_maintenance_windows_tenant
    FOREIGN KEY (tenant_id) REFERENCES tenants(id);
CREATE INDEX idx_maintenance_windows_tenant_id ON maintenance_windows (tenant_id);
