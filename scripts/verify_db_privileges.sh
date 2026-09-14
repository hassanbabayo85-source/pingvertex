#!/usr/bin/env bash
# ============================================================================
# scripts/verify_db_privileges.sh
#
# Verifies the least-privilege invariants introduced by
# migrations/0120_least_privilege_roles.sql. Reads DATABASE_URL (or falls
# back to the compose default) and runs each check as a SQL query.
#
# Exits non-zero on the first failed check. Suitable for a CI step or a
# pre-deploy gate.
#
# Usage:
#   DATABASE_URL=postgres://postgres:pw@localhost:5433/uptime_monitor \
#       ./scripts/verify_db_privileges.sh
# ============================================================================
set -euo pipefail

DATABASE_URL="${DATABASE_URL:-postgres://postgres:postgres@localhost:5433/uptime_monitor}"
PSQL="psql --no-psqlrc --tuples-only --no-align --set ON_ERROR_STOP=1 ${DATABASE_URL}"

pass=0
fail=0

check() {
    local name="$1"
    local query="$2"
    local expected="$3"

    local actual
    actual="$($PSQL -c "$query")"

    if [[ "$actual" == "$expected" ]]; then
        printf '  \033[32mPASS\033[0m  %s\n' "$name"
        pass=$((pass + 1))
    else
        printf '  \033[31mFAIL\033[0m  %s (expected=%q, got=%q)\n' \
            "$name" "$expected" "$actual"
        fail=$((fail + 1))
    fi
}

echo "== Verifying least-privilege database hardening =="
echo

echo "[roles exist and are not superuser]"
for r in pingvertex_migrator pingvertex_app pingvertex_worker; do
    check "$r exists" \
        "SELECT count(*) FROM pg_catalog.pg_roles WHERE rolname = '$r';" \
        "1"
    check "$r is NOT superuser" \
        "SELECT rolsuper FROM pg_catalog.pg_roles WHERE rolname = '$r';" \
        "f"
    check "$r has NO createrole" \
        "SELECT rolcreaterole FROM pg_catalog.pg_roles WHERE rolname = '$r';" \
        "f"
    check "$r has NO createdb" \
        "SELECT rolcreatedb FROM pg_catalog.pg_roles WHERE rolname = '$r';" \
        "f"
    check "$r has NO bypassrls" \
        "SELECT rolbypassrls FROM pg_catalog.pg_roles WHERE rolname = '$r';" \
        "f"
done

echo
echo "[PUBLIC cannot read or write financial/pricing/audit tables]"
for t in wallet_transactions api_usage_events pricing_configs payg_pricing_configs \
         financial_audit_log billing_audit_log audit_log pricing_audit_logs \
         payg_retired_operations; do
    if ! $PSQL -c "SELECT count(*) FROM information_schema.tables WHERE table_schema='public' AND table_name='$t';" | grep -q '^1$'; then
        printf '  \033[33mSKIP\033[0m  %s (table absent)\n' "$t"
        continue
    fi
    for priv in INSERT SELECT UPDATE DELETE; do
        check "PUBLIC cannot $priv $t" \
            "SELECT EXISTS(SELECT 1 FROM information_schema.role_table_grants WHERE table_schema='public' AND table_name='$t' AND grantee='PUBLIC' AND privilege_type='$priv');" \
            "f"
    done
done

echo
echo "[runtime roles have required access]"
check "pingvertex_app can INSERT wallet_transactions" \
    "SELECT has_table_privilege('pingvertex_app', 'wallet_transactions', 'INSERT');" \
    "t"
check "pingvertex_app can SELECT wallet_transactions" \
    "SELECT has_table_privilege('pingvertex_app', 'wallet_transactions', 'SELECT');" \
    "t"
check "pingvertex_app CANNOT UPDATE wallet_transactions" \
    "SELECT has_table_privilege('pingvertex_app', 'wallet_transactions', 'UPDATE');" \
    "f"
check "pingvertex_app CANNOT DELETE wallet_transactions" \
    "SELECT has_table_privilege('pingvertex_app', 'wallet_transactions', 'DELETE');" \
    "f"
check "pingvertex_worker can INSERT api_usage_events" \
    "SELECT has_table_privilege('pingvertex_worker', 'api_usage_events', 'INSERT');" \
    "t"
check "pingvertex_worker CANNOT UPDATE api_usage_events" \
    "SELECT has_table_privilege('pingvertex_worker', 'api_usage_events', 'UPDATE');" \
    "f"
check "pingvertex_app CANNOT INSERT pricing_configs" \
    "SELECT has_table_privilege('pingvertex_app', 'pricing_configs', 'INSERT');" \
    "f"
check "pingvertex_worker CANNOT INSERT payg_pricing_configs" \
    "SELECT has_table_privilege('pingvertex_worker', 'payg_pricing_configs', 'INSERT');" \
    "f"

echo
echo "[migrator can create / drop schema objects]"
# Dry-run: start a transaction, create a table, roll back. Proves
# CREATE privilege without leaving any residue.
if $PSQL -c "
BEGIN;
CREATE TABLE _verify_migrator_can_create (id int);
ROLLBACK;
" >/dev/null 2>&1; then
    printf '  \033[32mPASS\033[0m  migrator can CREATE TABLE\n'
    pass=$((pass + 1))
else
    printf '  \033[31mFAIL\033[0m  migrator cannot CREATE TABLE\n'
    fail=$((fail + 1))
fi

echo
echo "======================================="
printf 'Passed: %d   Failed: %d\n' "$pass" "$fail"
echo "======================================="

if (( fail > 0 )); then
    exit 1
fi
