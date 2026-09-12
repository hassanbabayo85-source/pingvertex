-- FOLLOW-UP AUDIT ISSUE #2 (post-remediation review, 2026-09):
-- "RUM page_view_id is still a plain FK".
--
-- Migration 0069 already fixed the primary tenant/site/session integrity
-- gaps (audit issues #5/#6) but deliberately left rum_js_errors.page_view_id
-- and rum_resource_errors.page_view_id as plain, single-column FKs to
-- rum_page_views(id) — see the "NOTE for follow-up" at the bottom of 0069.
-- That was scoped out at the time as lower marginal value since
-- page_view_id is nullable and every row is already fully tenant/site
-- constrained via its (required) session_id relationship. A follow-up
-- review flagged it as worth closing for strict 100% relational
-- integrity, so this migration finishes that job.
--
-- page_view_id is NULLABLE (not every error is tied to a specific page
-- view). PostgreSQL's default FK match type is MATCH SIMPLE, under which
-- a composite FK is automatically satisfied whenever ANY referencing
-- column is NULL — so rows with page_view_id IS NULL remain completely
-- unaffected by this change and continue to be constrained only by the
-- site/session FKs migration 0069 already added. This migration cannot
-- reject or alter any existing NULL-page_view_id row.
--
-- Depends on migration 0069's uq_rum_page_views_id_tenant_site UNIQUE
-- (id, tenant_id, site_id) constraint on rum_page_views, which this
-- migration's composite FKs reference.
--
-- As with 0069, if this migration fails with a foreign key violation, it
-- means some existing non-NULL page_view_id row's tenant_id/site_id does
-- not match its page view's tenant_id/site_id; that data must be
-- investigated and repaired before this migration can be applied there.

-- ---------------------------------------------------------------------
-- rum_js_errors
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
      AND kcu.column_name = 'page_view_id'
    LIMIT 1;

    IF fk_name IS NOT NULL THEN
        EXECUTE format('ALTER TABLE rum_js_errors DROP CONSTRAINT %I', fk_name);
    END IF;
END $$;

ALTER TABLE rum_js_errors
    ADD CONSTRAINT fk_rum_js_errors_page_view_tenant_site
    FOREIGN KEY (page_view_id, tenant_id, site_id)
    REFERENCES rum_page_views (id, tenant_id, site_id)
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
      AND kcu.column_name = 'page_view_id'
    LIMIT 1;

    IF fk_name IS NOT NULL THEN
        EXECUTE format('ALTER TABLE rum_resource_errors DROP CONSTRAINT %I', fk_name);
    END IF;
END $$;

ALTER TABLE rum_resource_errors
    ADD CONSTRAINT fk_rum_resource_errors_page_view_tenant_site
    FOREIGN KEY (page_view_id, tenant_id, site_id)
    REFERENCES rum_page_views (id, tenant_id, site_id)
    ON DELETE CASCADE;
