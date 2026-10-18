# PART 3 — Database Security Hardening

## Overview

Runtime API and worker no longer connect as the PostgreSQL superuser.
Three least-privilege roles are created on first database init:

| Role | Purpose | Superuser | Create DB | Create Role | Bypass RLS |
|---|---|---|---|---|---|
| `pingvertex_migrator` | Runs sqlx migrations; owns schema | no | no | no | no |
| `pingvertex_app` | API runtime | no | no | no | no |
| `pingvertex_worker` | Background worker | no | no | no | no |

The postgres superuser is used only for first-init bootstrap and for
local `cargo test` (which needs to CREATE SCHEMA per test binary).

## Invariants enforced

* A compromised API process cannot obtain PostgreSQL administrative authority.
  `pingvertex_app` has NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOBYPASSRLS.
* PUBLIC has no privileges on financial / pricing / audit tables. Migration
  `0120_least_privilege_roles.sql` explicitly REVOKE ALL FROM PUBLIC on every
  table, sequence, and function in the public schema.
* Immutable financial data stays immutable at the role layer: `wallet_transactions`,
  `api_usage_events`, `financial_audit_log`, `billing_audit_log`, `audit_log`,
  `pricing_audit_logs`, `payg_retired_operations` are INSERT+SELECT only.
  `pricing_configs` and `payg_pricing_configs` are SELECT-only.

## Files

* `migrations/0120_least_privilege_roles.sql`
* `docker/postgres-init/01-bootstrap-roles.sh`
* `scripts/verify_db_privileges.sh`
* `docker-compose.yml` / `docker-compose.prod.yml`

## Deployment

    docker compose -f docker-compose.yml -f docker-compose.prod.yml down -v
    docker compose -f docker-compose.yml -f docker-compose.prod.yml up -d --build
    DATABASE_URL=postgres://postgres:$POSTGRES_PASSWORD@localhost:5433/uptime_monitor \
        ./scripts/verify_db_privileges.sh

## Existing deployments

docker-entrypoint-initdb.d only runs on first DB init. On an existing volume,
bootstrap the roles manually before the migrate service runs. Use the SQL in
`docker/postgres-init/01-bootstrap-roles.sh` as the template.
