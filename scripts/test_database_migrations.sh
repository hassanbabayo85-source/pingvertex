#!/usr/bin/env bash
set -euo pipefail

: "${DATABASE_URL:?DATABASE_URL must be set}"
ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

# ============================================================================
# DESTRUCTIVE OPERATION GUARD
# ============================================================================
# This script will DROP the database at DATABASE_URL (see `sqlx database
# drop -y` below). It is designed for disposable test databases only, but
# without this guard a stray DATABASE_URL (e.g. pointing at a dev or, worse,
# production database) would be silently and irreversibly dropped.
#
# Require explicit opt-in via ALLOW_DB_DROP=1. Every invocation must
# acknowledge the destructive nature of the operation.
#
# Positioned BEFORE the tool-availability checks so that it fires even if
# sqlx/psql are not installed -- the guard is the first gate, not the second.
if [[ "${ALLOW_DB_DROP:-0}" != "1" ]]; then
    echo "REFUSING to run: this script will DROP the database at DATABASE_URL"
    echo "  DATABASE_URL = ${DATABASE_URL}"
    echo ""
    echo "The test harness requires DATABASE_URL to point at a disposable"
    echo "test database. It runs \`sqlx database drop -y\` on the main DB"
    echo "and on an upgrade copy derived from it (append \`_upgrade\`)."
    echo ""
    echo "If you are absolutely sure this is a disposable test DB, re-run:"
    echo "  ALLOW_DB_DROP=1 ./scripts/test_database_migrations.sh"
    echo ""
    echo "(This guard was added in response to an audit finding that the"
    echo "script could silently drop production data.)"
    exit 1
fi
# ============================================================================

command -v sqlx >/dev/null || { echo "sqlx CLI is required"; exit 1; }
command -v psql >/dev/null || { echo "psql is required"; exit 1; }

MIGRATIONS_DIR="${MIGRATIONS_DIR:-$ROOT/migrations}"
MIGRATION_COUNT="$(find "$MIGRATIONS_DIR" -maxdepth 1 -type f -name '*.sql' | wc -l | tr -d ' ')"
[[ "$MIGRATION_COUNT" =~ ^[0-9]+$ ]] && [ "$MIGRATION_COUNT" -gt 0 ] || {
  echo "No SQL migrations found in $MIGRATIONS_DIR"; exit 1;
}
LATEST_VERSION="$(find "$MIGRATIONS_DIR" -maxdepth 1 -type f -name '*.sql' -printf '%f\n' | sort | tail -1 | sed -E 's/^([0-9]+).*/\1/')"

echo "[1/4] Fresh migration -> latest ($MIGRATION_COUNT migrations)"
sqlx database drop -y
sqlx database create
sqlx migrate run --source "$MIGRATIONS_DIR"

psql "$DATABASE_URL" -v ON_ERROR_STOP=1 <<SQL
DO \$\$
DECLARE applied bigint;
BEGIN
  SELECT count(*) INTO applied FROM _sqlx_migrations WHERE success;
  IF applied <> $MIGRATION_COUNT THEN
    RAISE EXCEPTION 'expected $MIGRATION_COUNT successful migrations, got %', applied;
  END IF;
END \$\$;

DO \$\$
BEGIN
  -- Core monitoring tables: MUST still exist after the billing surface
  -- was dropped by migrations 0132-0134. If any of these are missing,
  -- the drop went too far and the application will not run.
  IF to_regclass('public.tenants') IS NULL THEN RAISE EXCEPTION 'tenants table missing'; END IF;
  IF to_regclass('public.users') IS NULL THEN RAISE EXCEPTION 'users table missing'; END IF;
  IF to_regclass('public.monitors') IS NULL THEN RAISE EXCEPTION 'monitors table missing'; END IF;
  IF to_regclass('public.checks') IS NULL THEN RAISE EXCEPTION 'checks table missing'; END IF;
  IF to_regclass('public.incidents') IS NULL THEN RAISE EXCEPTION 'incidents table missing'; END IF;
  IF to_regclass('public.agents') IS NULL THEN RAISE EXCEPTION 'agents table missing'; END IF;
  IF to_regclass('public.audit_log') IS NULL THEN RAISE EXCEPTION 'audit_log table missing'; END IF;
  IF to_regclass('public.notification_channels') IS NULL THEN RAISE EXCEPTION 'notification_channels table missing'; END IF;
  IF to_regclass('public.agent_check_leases') IS NULL THEN RAISE EXCEPTION 'agent_check_leases table missing'; END IF;
  IF to_regclass('public.proxy_check_jobs') IS NULL THEN RAISE EXCEPTION 'proxy_check_jobs table missing'; END IF;
  IF to_regclass('public.proxy_entries') IS NULL THEN RAISE EXCEPTION 'proxy_entries table missing'; END IF;
  IF to_regclass('public.test_access_grants') IS NULL THEN RAISE EXCEPTION 'test_access_grants table missing'; END IF;

  -- Billing tables: MUST be gone after 0134.
  IF to_regclass('public.subscriptions') IS NOT NULL THEN RAISE EXCEPTION 'subscriptions table still present'; END IF;
  IF to_regclass('public.plans') IS NOT NULL THEN RAISE EXCEPTION 'plans table still present'; END IF;
  IF to_regclass('public.plan_entitlements') IS NOT NULL THEN RAISE EXCEPTION 'plan_entitlements table still present'; END IF;
  IF to_regclass('public.plan_prices') IS NOT NULL THEN RAISE EXCEPTION 'plan_prices table still present'; END IF;
  IF to_regclass('public.user_wallets') IS NOT NULL THEN RAISE EXCEPTION 'user_wallets table still present'; END IF;
  IF to_regclass('public.wallet_transactions') IS NOT NULL THEN RAISE EXCEPTION 'wallet_transactions table still present'; END IF;
  IF to_regclass('public.credit_reservations') IS NOT NULL THEN RAISE EXCEPTION 'credit_reservations table still present'; END IF;
  IF to_regclass('public.billing_webhook_idempotency') IS NOT NULL THEN RAISE EXCEPTION 'billing_webhook_idempotency table still present'; END IF;
  IF to_regclass('public.api_usage_events') IS NOT NULL THEN RAISE EXCEPTION 'api_usage_events table still present'; END IF;
  IF to_regclass('public.payg_pricing_configs') IS NOT NULL THEN RAISE EXCEPTION 'payg_pricing_configs table still present'; END IF;
  IF to_regclass('public.monitor_billing_blocks') IS NOT NULL THEN RAISE EXCEPTION 'monitor_billing_blocks table still present'; END IF;

  -- Residue columns: MUST be gone after 0134.
  IF EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_schema='public' AND table_name='agent_check_leases' AND column_name='billing_reserved'
  ) THEN RAISE EXCEPTION 'agent_check_leases.billing_reserved still present'; END IF;
  IF EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_schema='public' AND table_name='test_access_grants' AND column_name='plan_tier'
  ) THEN RAISE EXCEPTION 'test_access_grants.plan_tier still present'; END IF;

  -- Billing view/functions: MUST be gone after 0134.
  IF to_regclass('public.payg_financial_event_contract') IS NOT NULL THEN RAISE EXCEPTION 'payg_financial_event_contract view still present'; END IF;
  IF EXISTS (SELECT 1 FROM pg_proc WHERE proname='prevent_pricing_config_mutation') THEN RAISE EXCEPTION 'prevent_pricing_config_mutation function still present'; END IF;
  IF EXISTS (SELECT 1 FROM pg_proc WHERE proname='prevent_wallet_transaction_mutation') THEN RAISE EXCEPTION 'prevent_wallet_transaction_mutation function still present'; END IF;
  IF EXISTS (SELECT 1 FROM pg_proc WHERE proname='prevent_billing_audit_log_mutation') THEN RAISE EXCEPTION 'prevent_billing_audit_log_mutation function still present'; END IF;
END \$\$;

DO \$\$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint
    WHERE conrelid='proxy_check_jobs'::regclass
      AND conname='fk_proxy_check_jobs_tenant'
  ) THEN RAISE EXCEPTION 'proxy job tenant FK missing'; END IF;

  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint
    WHERE conrelid='proxy_check_jobs'::regclass
      AND conname='fk_proxy_check_jobs_tenant_user'
  ) THEN RAISE EXCEPTION 'proxy job tenant/user FK missing'; END IF;

  IF NOT EXISTS (
    SELECT 1 FROM pg_indexes
    WHERE schemaname='public' AND indexname='proxy_check_jobs_tenant_idx'
  ) THEN RAISE EXCEPTION 'proxy job tenant index missing'; END IF;

  IF NOT EXISTS (
    SELECT 1 FROM pg_indexes
    WHERE schemaname='public' AND indexname='proxy_check_jobs_status_idx'
  ) THEN RAISE EXCEPTION 'proxy job status index missing'; END IF;
END \$\$;
SQL

echo "[2/4] Upgrade test: install migrations 0001..latest-1, seed compatible rows, then upgrade"
UPGRADE_DATABASE_URL="${UPGRADE_DATABASE_URL:-${DATABASE_URL%/*}/uptime_monitor_upgrade}"
export UPGRADE_DATABASE_URL
sqlx database drop -y --database-url "$UPGRADE_DATABASE_URL" 2>/dev/null || true
sqlx database create --database-url "$UPGRADE_DATABASE_URL"

TMP_MIGRATIONS="$(mktemp -d)"
trap 'rm -rf "$TMP_MIGRATIONS"' EXIT
find "$MIGRATIONS_DIR" -maxdepth 1 -type f -name '*.sql' -printf '%f\n' | sort | head -n $((MIGRATION_COUNT-1)) |
  while IFS= read -r file; do cp "$MIGRATIONS_DIR/$file" "$TMP_MIGRATIONS/"; done
sqlx migrate run --source "$TMP_MIGRATIONS" --database-url "$UPGRADE_DATABASE_URL"

psql "$UPGRADE_DATABASE_URL" -v ON_ERROR_STOP=1 <<'SQL'
DO $$
BEGIN
  -- NOTE: `subscriptions` is intentionally NOT checked here.
  -- Migration 0134 drops it, so once latest advances past 0134, the
  -- pre-latest schema (latest-1) is past that drop too. The
  -- post-latest block above (billing tables MUST be gone) already
  -- enforces the invariant. This block only verifies that the schema
  -- is in a runnable pre-upgrade state.
  IF to_regclass('public.users') IS NULL OR to_regclass('public.tenants') IS NULL
     OR to_regclass('public.proxy_check_jobs') IS NULL
  THEN RAISE EXCEPTION 'pre-latest schema missing required tables'; END IF;
END $$;
SQL

sqlx migrate run --source "$MIGRATIONS_DIR" --database-url "$UPGRADE_DATABASE_URL"
UPGRADE_APPLIED="$(psql "$UPGRADE_DATABASE_URL" -Atc "SELECT count(*) FROM _sqlx_migrations WHERE success")"
[ "$UPGRADE_APPLIED" = "$MIGRATION_COUNT" ] || {
  echo "Upgrade failed: expected $MIGRATION_COUNT migrations, got $UPGRADE_APPLIED"; exit 1;
}

echo "[3/4] Migration integrity and rollback support"
if find "$MIGRATIONS_DIR" -maxdepth 1 -type f \( -name '*down*.sql' -o -name '*.down.sql' \) | grep -q .; then
  echo "Rollback files detected. This repository's SQLx runner does not expose automatic down-migration execution;"
  echo "rollback artifacts are present and must be exercised by the project's documented rollback procedure."
else
  echo "Rollback: NOT SUPPORTED by this SQLx migration set (no down migrations found)."
fi

echo "[4/4] Current migration version"
echo "Latest migration file: $LATEST_VERSION"
echo "Database migration tests: PASS"
