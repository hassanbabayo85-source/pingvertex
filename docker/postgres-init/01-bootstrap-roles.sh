#!/bin/bash
# ============================================================================
# Postgres first-init bootstrap: create the three least-privilege roles.
#
# Runs exactly once, on the first initialization of the database cluster,
# as the POSTGRES_USER (a superuser). All statements are idempotent so
# the script is safe if the volume is reused.
#
# Environment (all required — the compose file fails closed if unset):
#   POSTGRES_MIGRATOR_PASSWORD
#   POSTGRES_APP_PASSWORD
#   POSTGRES_WORKER_PASSWORD
# ============================================================================
set -euo pipefail

: "${POSTGRES_MIGRATOR_PASSWORD:?POSTGRES_MIGRATOR_PASSWORD must be set}"
: "${POSTGRES_APP_PASSWORD:?POSTGRES_APP_PASSWORD must be set}"
: "${POSTGRES_WORKER_PASSWORD:?POSTGRES_WORKER_PASSWORD must be set}"

psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<SQL
DO \$\$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_catalog.pg_roles WHERE rolname = 'pingvertex_migrator') THEN
        CREATE ROLE pingvertex_migrator LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOBYPASSRLS
            PASSWORD '${POSTGRES_MIGRATOR_PASSWORD}';
    ELSE
        ALTER ROLE pingvertex_migrator WITH LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOBYPASSRLS
            PASSWORD '${POSTGRES_MIGRATOR_PASSWORD}';
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_catalog.pg_roles WHERE rolname = 'pingvertex_app') THEN
        CREATE ROLE pingvertex_app LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOBYPASSRLS
            PASSWORD '${POSTGRES_APP_PASSWORD}';
    ELSE
        ALTER ROLE pingvertex_app WITH LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOBYPASSRLS
            PASSWORD '${POSTGRES_APP_PASSWORD}';
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_catalog.pg_roles WHERE rolname = 'pingvertex_worker') THEN
        CREATE ROLE pingvertex_worker LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOBYPASSRLS
            PASSWORD '${POSTGRES_WORKER_PASSWORD}';
    ELSE
        ALTER ROLE pingvertex_worker WITH LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOBYPASSRLS
            PASSWORD '${POSTGRES_WORKER_PASSWORD}';
    END IF;
END
\$\$;

-- Schema-level privileges. Revoke PUBLIC first, then grant narrowly.
REVOKE ALL ON SCHEMA public FROM PUBLIC;
GRANT USAGE ON SCHEMA public TO pingvertex_migrator, pingvertex_app, pingvertex_worker;
GRANT CREATE ON SCHEMA public TO pingvertex_migrator;
SQL

echo "pingvertex least-privilege roles bootstrapped."
