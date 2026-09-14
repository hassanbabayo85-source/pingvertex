-- ============================================================================
-- PART 3 — DATABASE SECURITY HARDENING: least-privilege role separation
-- ============================================================================
--
-- Goals:
--   1. Remove excessive PostgreSQL privileges (no superuser for runtime).
--   2. Separate migration / application / worker roles.
--   3. Remove `GRANT ... TO PUBLIC` from financial/pricing/audit tables.
--   4. `REVOKE ALL FROM PUBLIC` on every sensitive object, then grant
--      only what each role needs.
--   5. Handle sequences alongside tables.
--
-- Roles created here (NOLOGIN — passwords are set out-of-band by
-- `docker/postgres-init/01-bootstrap-roles.sh` or an operator ALTER ROLE):
--
--   * pingvertex_migrator — may CREATE/ALTER schema. Runs sqlx migrations.
--   * pingvertex_app      — API runtime. NO superuser. Reads/writes
--                           tenant-scoped data; INSERT-only on
--                           immutable financial/audit tables.
--   * pingvertex_worker   — background worker. NO superuser. Reads/writes
--                           scheduler/check/incident/wallet-transaction
--                           state; INSERT-only on immutable tables.
--
-- The inviolable invariant this migration enforces:
--
--   A compromised API process MUST NOT obtain PostgreSQL administrative
--   authority. `pingvertex_app` has NOSUPERUSER, NOCREATEDB, NOCREATEROLE,
--   NOINHERIT and no BYPASSRLS. Even at full SQL injection, the worst it
--   can do is DML on tenant-scoped rows — it cannot create/drop databases,
--   cannot create roles, cannot grant itself more privileges.
--
-- The immutable financial/audit tables are INSERT + SELECT only for the
-- runtime roles. Their append-only enforcement is ultimately enforced by
-- the existing triggers (see migrations 0029, 0078, 0084); the role-level
-- REVOKE here is defense-in-depth so a trigger misconfiguration alone is
-- not sufficient to permit UPDATE/DELETE.
-- ============================================================================

-- ----------------------------------------------------------------------------
-- PART 1 — Roles (idempotent).
-- ----------------------------------------------------------------------------
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_catalog.pg_roles WHERE rolname = 'pingvertex_migrator') THEN
        CREATE ROLE pingvertex_migrator NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOBYPASSRLS;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_catalog.pg_roles WHERE rolname = 'pingvertex_app') THEN
        CREATE ROLE pingvertex_app NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOBYPASSRLS;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_catalog.pg_roles WHERE rolname = 'pingvertex_worker') THEN
        CREATE ROLE pingvertex_worker NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOBYPASSRLS;
    END IF;

    -- Assert the invariant directly: if for any reason a role with
    -- these names exists as superuser, fail the migration loudly.
    IF (SELECT rolsuper FROM pg_catalog.pg_roles WHERE rolname = 'pingvertex_app') THEN
        RAISE EXCEPTION 'SECURITY: pingvertex_app must NOT be superuser';
    END IF;
    IF (SELECT rolsuper FROM pg_catalog.pg_roles WHERE rolname = 'pingvertex_worker') THEN
        RAISE EXCEPTION 'SECURITY: pingvertex_worker must NOT be superuser';
    END IF;
    IF (SELECT rolsuper FROM pg_catalog.pg_roles WHERE rolname = 'pingvertex_migrator') THEN
        RAISE EXCEPTION 'SECURITY: pingvertex_migrator must NOT be superuser';
    END IF;
END
$$;

-- ----------------------------------------------------------------------------
-- PART 2 — Schema-level privileges.
--
-- Default Postgres has `GRANT USAGE, CREATE ON SCHEMA public TO PUBLIC`
-- (pre-PG15). Revoke it: schema CREATE must be migrator-only.
-- ----------------------------------------------------------------------------
REVOKE ALL ON SCHEMA public FROM PUBLIC;
GRANT USAGE ON SCHEMA public
    TO pingvertex_migrator, pingvertex_app, pingvertex_worker;
GRANT CREATE ON SCHEMA public TO pingvertex_migrator;

-- ----------------------------------------------------------------------------
-- PART 3 — REVOKE ALL FROM PUBLIC on every table, sequence, and function.
--
-- This is the blanket defense: whatever a previous migration granted to
-- PUBLIC (e.g. 0050 `GRANT INSERT, SELECT ON pricing_configs TO PUBLIC`,
-- 0078 `GRANT INSERT, SELECT ON wallet_transactions TO PUBLIC`) is now
-- removed. Individual grants to the runtime roles come in Parts 4–5.
-- ----------------------------------------------------------------------------
DO $$
DECLARE
    r RECORD;
BEGIN
    FOR r IN
        SELECT schemaname, tablename
        FROM pg_catalog.pg_tables
        WHERE schemaname = 'public'
    LOOP
        EXECUTE format('REVOKE ALL ON TABLE %I.%I FROM PUBLIC', r.schemaname, r.tablename);
    END LOOP;

    FOR r IN
        SELECT sequence_schema, sequence_name
        FROM information_schema.sequences
        WHERE sequence_schema = 'public'
    LOOP
        EXECUTE format('REVOKE ALL ON SEQUENCE %I.%I FROM PUBLIC',
                       r.sequence_schema, r.sequence_name);
    END LOOP;

    FOR r IN
        SELECT p.oid::regprocedure AS sig
        FROM pg_catalog.pg_proc p
        WHERE p.pronamespace = 'public'::regnamespace
    LOOP
        EXECUTE format('REVOKE ALL ON FUNCTION %s FROM PUBLIC', r.sig);
    END LOOP;
END
$$;

-- ----------------------------------------------------------------------------
-- PART 4 — General DML grants for the two runtime roles.
--
-- Everything in `public` gets SELECT/INSERT/UPDATE/DELETE for BOTH
-- pingvertex_app and pingvertex_worker by default. The DML restriction
-- for immutable financial / pricing tables happens next.
--
-- Rationale: maintaining an exhaustive per-table matrix for ~100 tables
-- is fragile (a new table added by a future migration silently breaks
-- a worker without warning). The security-critical restriction is the
-- IMMUTABILITY of financial/audit/pricing data, which is enforced in
-- Part 5 by explicit REVOKE. Worker-vs-app cross-writes are constrained
-- at the application layer (`jwt_auth`, use-case authorization, tenant
-- scoping), not at the DB role layer.
-- ----------------------------------------------------------------------------
DO $$
DECLARE
    r RECORD;
BEGIN
    FOR r IN
        SELECT schemaname, tablename
        FROM pg_catalog.pg_tables
        WHERE schemaname = 'public'
    LOOP
        EXECUTE format(
            'GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE %I.%I TO pingvertex_app, pingvertex_worker',
            r.schemaname, r.tablename
        );
    END LOOP;

    FOR r IN
        SELECT sequence_schema, sequence_name
        FROM information_schema.sequences
        WHERE sequence_schema = 'public'
    LOOP
        EXECUTE format(
            'GRANT USAGE, SELECT ON SEQUENCE %I.%I TO pingvertex_app, pingvertex_worker',
            r.sequence_schema, r.sequence_name
        );
    END LOOP;
END
$$;

-- ----------------------------------------------------------------------------
-- PART 5 — Immutable financial / audit / pricing tables.
--
-- `immutable_tables`: INSERT + SELECT only for runtime roles. Their
-- real defense is a trigger, but the role-level REVOKE means even a
-- trigger-drop-in-a-compromised-migration cannot re-enable UPDATE/DELETE
-- for these tables. Consistency is preserved on the "who can write"
-- axis: the migrator may still UPDATE/DELETE (e.g. a data migration),
-- but that's a privileged, audit-logged action.
--
-- `pricing_tables`: SELECT only for runtime roles. Prices are set only
-- through the migrator-driven price book (append-only). The API reads
-- the active version; the worker reads the price at capture time.
-- ----------------------------------------------------------------------------
DO $$
DECLARE
    t text;
    immutable_tables text[] := ARRAY[
        'wallet_transactions',
        'api_usage_events',
        'financial_audit_log',
        'billing_audit_log',
        'audit_log',
        'pricing_audit_logs',
        'payg_retired_operations'
    ];
    pricing_tables text[] := ARRAY[
        'pricing_configs',
        'payg_pricing_configs'
    ];
BEGIN
    FOREACH t IN ARRAY immutable_tables LOOP
        IF EXISTS (
            SELECT 1 FROM information_schema.tables
            WHERE table_schema = 'public' AND table_name = t
        ) THEN
            EXECUTE format(
                'REVOKE UPDATE, DELETE, TRUNCATE ON TABLE %I FROM pingvertex_app, pingvertex_worker',
                t
            );
            EXECUTE format(
                'GRANT INSERT, SELECT ON TABLE %I TO pingvertex_app, pingvertex_worker',
                t
            );
        END IF;
    END LOOP;

    FOREACH t IN ARRAY pricing_tables LOOP
        IF EXISTS (
            SELECT 1 FROM information_schema.tables
            WHERE table_schema = 'public' AND table_name = t
        ) THEN
            EXECUTE format(
                'REVOKE INSERT, UPDATE, DELETE, TRUNCATE ON TABLE %I FROM pingvertex_app, pingvertex_worker',
                t
            );
            EXECUTE format('GRANT SELECT ON TABLE %I TO pingvertex_app, pingvertex_worker', t);
        END IF;
    END LOOP;
END
$$;

-- ----------------------------------------------------------------------------
-- PART 6 — Migrator retains full ownership of its own schema objects.
-- (It created the objects; ALTER/GRANT/DROP is implicit by ownership.
-- This block is a no-op safeguard that documents the expectation.)
-- ----------------------------------------------------------------------------
GRANT SELECT, INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER
    ON ALL TABLES IN SCHEMA public TO pingvertex_migrator;
GRANT USAGE, SELECT, UPDATE
    ON ALL SEQUENCES IN SCHEMA public TO pingvertex_migrator;

-- The migrator also needs write access to sqlx's bookkeeping table so
-- that a migration-tracking reset can be performed by the migrator
-- itself (e.g. `sqlx migrate info`, or an intentional down-migration).
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables
               WHERE table_schema = 'public' AND table_name = '_sqlx_migrations') THEN
        EXECUTE 'GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE _sqlx_migrations TO pingvertex_migrator';
    END IF;
END
$$;

-- ----------------------------------------------------------------------------
-- PART 7 — Default privileges for FUTURE objects.
--
-- Any table / sequence / function created by the migrator (or by the
-- current session's role, e.g. `postgres` on a legacy install) AFTER
-- this migration gets the same hardening automatically, so a future
-- migration cannot silently re-open a PUBLIC hole.
-- ----------------------------------------------------------------------------
DO $$
DECLARE
    creator text;
BEGIN
    FOR creator IN
        SELECT DISTINCT rolname FROM pg_catalog.pg_roles
        WHERE rolname IN ('pingvertex_migrator', current_user)
    LOOP
        EXECUTE format(
            'ALTER DEFAULT PRIVILEGES FOR ROLE %I IN SCHEMA public REVOKE ALL ON TABLES FROM PUBLIC',
            creator
        );
        EXECUTE format(
            'ALTER DEFAULT PRIVILEGES FOR ROLE %I IN SCHEMA public GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO pingvertex_app, pingvertex_worker',
            creator
        );
        EXECUTE format(
            'ALTER DEFAULT PRIVILEGES FOR ROLE %I IN SCHEMA public REVOKE ALL ON SEQUENCES FROM PUBLIC',
            creator
        );
        EXECUTE format(
            'ALTER DEFAULT PRIVILEGES FOR ROLE %I IN SCHEMA public GRANT USAGE, SELECT ON SEQUENCES TO pingvertex_app, pingvertex_worker',
            creator
        );
        EXECUTE format(
            'ALTER DEFAULT PRIVILEGES FOR ROLE %I IN SCHEMA public REVOKE ALL ON FUNCTIONS FROM PUBLIC',
            creator
        );
    END LOOP;
END
$$;
