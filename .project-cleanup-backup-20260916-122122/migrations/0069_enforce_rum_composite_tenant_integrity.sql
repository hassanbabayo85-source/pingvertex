-- AUDIT ISSUE #5 / #6 (2026-09 remediation): "RUM tenant/site relationship
-- is not enforced at the database level" / "RUM session/child
-- relationships also lack composite tenant integrity".
--
-- 0062 gave every RUM table its own independent foreign keys:
--     tenant_id -> tenants(id)
--     site_id   -> rum_sites(id)
--     session_id -> rum_sessions(id)
-- Each of those is individually valid, but nothing ties them together —
-- the database can accept a row whose site_id belongs to a DIFFERENT
-- tenant than its tenant_id, or a session/page_view/error row whose
-- site_id doesn't match the site_id of the session it claims to belong
-- to. The application's ingestion path (`IngestRumEventUseCase`) always
-- derives tenant_id/site_id together from a single `find_by_site_key`
-- lookup, so it does not currently produce inconsistent rows — but nothing
-- at the schema level prevents a future worker, admin script, import, or
-- bug from doing so. This mirrors exactly the "Cross-Tenant Parent/Child
-- Relationships" problem migration 0065 already fixed for
-- monitors/api_flows/maintenance_windows/escalation_policies/
-- notification_channels; this migration applies the same composite-FK
-- pattern to the RUM hierarchy.
--
-- If this migration fails with a foreign key violation, it means
-- inconsistent cross-tenant/cross-site RUM rows already exist in the
-- target environment; that data must be investigated and repaired before
-- this migration can be applied there.
--
-- Constraint names are looked up dynamically (same pattern as migration
-- 0060) rather than hardcoded, since PostgreSQL's auto-generated names
-- for the original 0062 column-level `REFERENCES` constraints are not
-- guaranteed to be identical across every environment this has been
-- applied to.

-- ---------------------------------------------------------------------
-- rum_sites: make (id, tenant_id) referenceable by composite FKs below.
-- `id` is already the primary key, so this is a redundant-but-required
-- superset unique constraint (same technique as 0065's
-- uq_monitors_id_tenant_id).
-- ---------------------------------------------------------------------
ALTER TABLE rum_sites
    ADD CONSTRAINT uq_rum_sites_id_tenant UNIQUE (id, tenant_id);

-- ---------------------------------------------------------------------
-- rum_sessions
-- ---------------------------------------------------------------------
DO $$
DECLARE
    fk_name text;
BEGIN
    SELECT tc.constraint_name INTO fk_name
    FROM information_schema.table_constraints tc
    JOIN information_schema.key_column_usage kcu
        ON tc.constraint_name = kcu.constraint_name
       AND tc.table_schema = kcu.table_schema
    WHERE tc.table_schema = current_schema()
      AND tc.table_name = 'rum_sessions'
      AND tc.constraint_type = 'FOREIGN KEY'
      AND kcu.column_name = 'site_id'
    LIMIT 1;

    IF fk_name IS NOT NULL THEN
        EXECUTE format('ALTER TABLE rum_sessions DROP CONSTRAINT %I', fk_name);
    END IF;
END $$;

ALTER TABLE rum_sessions
    ADD CONSTRAINT fk_rum_sessions_site_tenant
    FOREIGN KEY (site_id, tenant_id)
    REFERENCES rum_sites (id, tenant_id)
    ON DELETE CASCADE;

-- Make (id, tenant_id, site_id) referenceable by rum_page_views/
-- rum_js_errors/rum_resource_errors below.
ALTER TABLE rum_sessions
    ADD CONSTRAINT uq_rum_sessions_id_tenant_site UNIQUE (id, tenant_id, site_id);

-- ---------------------------------------------------------------------
-- rum_page_views
-- ---------------------------------------------------------------------
DO $$
DECLARE
    fk_name text;
BEGIN
    SELECT tc.constraint_name INTO fk_name
    FROM information_schema.table_constraints tc
    JOIN information_schema.key_column_usage kcu
        ON tc.constraint_name = kcu.constraint_name
       AND tc.table_schema = kcu.table_schema
    WHERE tc.table_schema = current_schema()
      AND tc.table_name = 'rum_page_views'
      AND tc.constraint_type = 'FOREIGN KEY'
      AND kcu.column_name = 'site_id'
    LIMIT 1;

    IF fk_name IS NOT NULL THEN
        EXECUTE format('ALTER TABLE rum_page_views DROP CONSTRAINT %I', fk_name);
    END IF;
END $$;

DO $$
DECLARE
    fk_name text;
BEGIN
    SELECT tc.constraint_name INTO fk_name
    FROM information_schema.table_constraints tc
    JOIN information_schema.key_column_usage kcu
        ON tc.constraint_name = kcu.constraint_name
       AND tc.table_schema = kcu.table_schema
    WHERE tc.table_schema = current_schema()
      AND tc.table_name = 'rum_page_views'
      AND tc.constraint_type = 'FOREIGN KEY'
      AND kcu.column_name = 'session_id'
    LIMIT 1;

    IF fk_name IS NOT NULL THEN
        EXECUTE format('ALTER TABLE rum_page_views DROP CONSTRAINT %I', fk_name);
    END IF;
END $$;

ALTER TABLE rum_page_views
    ADD CONSTRAINT fk_rum_page_views_site_tenant
    FOREIGN KEY (site_id, tenant_id)
    REFERENCES rum_sites (id, tenant_id)
    ON DELETE CASCADE;

ALTER TABLE rum_page_views
    ADD CONSTRAINT fk_rum_page_views_session_tenant_site
    FOREIGN KEY (session_id, tenant_id, site_id)
    REFERENCES rum_sessions (id, tenant_id, site_id)
    ON DELETE CASCADE;

-- Make (id, tenant_id, site_id) referenceable by rum_js_errors/
-- rum_resource_errors' nullable page_view_id below.
ALTER TABLE rum_page_views
    ADD CONSTRAINT uq_rum_page_views_id_tenant_site UNIQUE (id, tenant_id, site_id);

-- ---------------------------------------------------------------------
-- rum_js_errors
--
-- page_view_id is nullable (not every error is tied to a specific page
-- view). A composite FK with MATCH SIMPLE (Postgres's default match
-- type) is automatically satisfied whenever ANY referencing column is
-- NULL, so page_view_id-less errors remain correctly unconstrained by
-- that particular FK while still being fully constrained by the
-- required site/session FKs below — same reasoning migration 0065 used
-- for escalation_policies' nullable monitor_id.
-- ---------------------------------------------------------------------
DO $$
DECLARE
    fk_name text;
BEGIN
    SELECT tc.constraint_name INTO fk_name
    FROM information_schema.table_constraints tc
    JOIN information_schema.key_column_usage kcu
        ON tc.constraint_name = kcu.constraint_name
       AND tc.table_schema = kcu.table_schema
    WHERE tc.table_schema = current_schema()
      AND tc.table_name = 'rum_js_errors'
      AND tc.constraint_type = 'FOREIGN KEY'
      AND kcu.column_name = 'site_id'
    LIMIT 1;

    IF fk_name IS NOT NULL THEN
        EXECUTE format('ALTER TABLE rum_js_errors DROP CONSTRAINT %I', fk_name);
    END IF;
END $$;

DO $$
DECLARE
    fk_name text;
BEGIN
    SELECT tc.constraint_name INTO fk_name
    FROM information_schema.table_constraints tc
    JOIN information_schema.key_column_usage kcu
        ON tc.constraint_name = kcu.constraint_name
       AND tc.table_schema = kcu.table_schema
    WHERE tc.table_schema = current_schema()
      AND tc.table_name = 'rum_js_errors'
      AND tc.constraint_type = 'FOREIGN KEY'
      AND kcu.column_name = 'session_id'
    LIMIT 1;

    IF fk_name IS NOT NULL THEN
        EXECUTE format('ALTER TABLE rum_js_errors DROP CONSTRAINT %I', fk_name);
    END IF;
END $$;

ALTER TABLE rum_js_errors
    ADD CONSTRAINT fk_rum_js_errors_site_tenant
    FOREIGN KEY (site_id, tenant_id)
    REFERENCES rum_sites (id, tenant_id)
    ON DELETE CASCADE;

ALTER TABLE rum_js_errors
    ADD CONSTRAINT fk_rum_js_errors_session_tenant_site
    FOREIGN KEY (session_id, tenant_id, site_id)
    REFERENCES rum_sessions (id, tenant_id, site_id)
    ON DELETE CASCADE;

-- ---------------------------------------------------------------------
-- rum_resource_errors (same shape as rum_js_errors above)
-- ---------------------------------------------------------------------
DO $$
DECLARE
    fk_name text;
BEGIN
    SELECT tc.constraint_name INTO fk_name
    FROM information_schema.table_constraints tc
    JOIN information_schema.key_column_usage kcu
        ON tc.constraint_name = kcu.constraint_name
       AND tc.table_schema = kcu.table_schema
    WHERE tc.table_schema = current_schema()
      AND tc.table_name = 'rum_resource_errors'
      AND tc.constraint_type = 'FOREIGN KEY'
      AND kcu.column_name = 'site_id'
    LIMIT 1;

    IF fk_name IS NOT NULL THEN
        EXECUTE format('ALTER TABLE rum_resource_errors DROP CONSTRAINT %I', fk_name);
    END IF;
END $$;

DO $$
DECLARE
    fk_name text;
BEGIN
    SELECT tc.constraint_name INTO fk_name
    FROM information_schema.table_constraints tc
    JOIN information_schema.key_column_usage kcu
        ON tc.constraint_name = kcu.constraint_name
       AND tc.table_schema = kcu.table_schema
    WHERE tc.table_schema = current_schema()
      AND tc.table_name = 'rum_resource_errors'
      AND tc.constraint_type = 'FOREIGN KEY'
      AND kcu.column_name = 'session_id'
    LIMIT 1;

    IF fk_name IS NOT NULL THEN
        EXECUTE format('ALTER TABLE rum_resource_errors DROP CONSTRAINT %I', fk_name);
    END IF;
END $$;

ALTER TABLE rum_resource_errors
    ADD CONSTRAINT fk_rum_resource_errors_site_tenant
    FOREIGN KEY (site_id, tenant_id)
    REFERENCES rum_sites (id, tenant_id)
    ON DELETE CASCADE;

ALTER TABLE rum_resource_errors
    ADD CONSTRAINT fk_rum_resource_errors_session_tenant_site
    FOREIGN KEY (session_id, tenant_id, site_id)
    REFERENCES rum_sessions (id, tenant_id, site_id)
    ON DELETE CASCADE;

-- ---------------------------------------------------------------------
-- NOTE for follow-up (not done in this migration, mirrors 0065's own
-- note): `page_view_id` on rum_js_errors/rum_resource_errors is still
-- only a plain `REFERENCES rum_page_views(id)` FK, not tenant/site
-- composite. Adding that would require rum_page_views' own
-- uq_rum_page_views_id_tenant_site (added above) plus threading
-- page_view's tenant_id/site_id through as NOT NULL companions on the
-- error tables, which they already carry — but since page_view_id is
-- itself optional and always reached via a session that is already
-- fully tenant/site-constrained by the FKs above, the marginal
-- integrity gain did not justify bundling a third composite FK into
-- this migration. Revisit if page_view_id ever becomes a primary
-- access/join path for these tables.
-- ---------------------------------------------------------------------
