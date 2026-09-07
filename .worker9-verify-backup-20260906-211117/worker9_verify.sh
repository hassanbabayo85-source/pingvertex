#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

failures=0
run_step() {
  local name="$1"; shift
  echo
  echo "=== $name ==="
  if "$@"; then
    echo "PASS: $name"
  else
    echo "FAIL: $name"
    failures=$((failures+1))
  fi
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "Missing required command: $1"
    return 1
  }
}

echo "Worker 9 comprehensive verification"
echo "Project: $ROOT"

for cmd in cargo sqlx psql docker jq openssl; do
  if ! require_cmd "$cmd"; then
    echo "Preflight failed."
    exit 2
  fi
done

run_step "Rust formatting" cargo fmt --all -- --check
run_step "Workspace unit/integration tests" cargo test --workspace --all-targets
run_step "Migration fresh/upgrade/constraints/FKs/indexes" env DATABASE_URL="${DATABASE_URL:?DATABASE_URL required}" bash scripts/test_database_migrations.sh

# Database-backed integration tests are intentionally ignored in normal cargo
# runs because they require a disposable PostgreSQL instance.
if [ "${RUN_IGNORED_DB_TESTS:-1}" = "1" ]; then
  run_step "Infra ignored PostgreSQL tests" cargo test -p infra --test subscription_usage_transaction -- --ignored
  run_step "Infra ignored invoice isolation tests" cargo test -p infra --test worker8_invoice_isolation -- --ignored
  run_step "Infra ignored wallet isolation tests" cargo test -p infra --test worker8_wallet_isolation -- --ignored
  run_step "Worker ignored proxy double-claim test" cargo test -p worker --test proxy_check_double_claim -- --ignored
fi

run_step "Existing full API/worker E2E" bash scripts/e2e_full_journey.sh

# Three consecutive CI-like passes. This catches timing/idempotency races
# without pretending that one green run proves non-flakiness.
for pass in 1 2 3; do
  run_step "CI stability pass $pass" cargo test --workspace --all-targets
done

echo
if [ "$failures" -eq 0 ]; then
  echo "RESULT: WORKER 9 PASS"
  echo "All runnable migration/backend/E2E suites passed and three consecutive CI-style runs were green."
  exit 0
fi

echo "RESULT: WORKER 9 FAILED ($failures step(s))"
exit 1
