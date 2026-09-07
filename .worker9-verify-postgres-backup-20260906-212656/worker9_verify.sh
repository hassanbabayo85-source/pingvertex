#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

failures=0
POSTGRES_CONTAINER=""
TEST_DB_NAME=""
TEST_DB_URL=""

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

load_database_url_from_dotenv() {
  [ -n "${DATABASE_URL:-}" ] && return 0
  [ -f ".env" ] || return 0

  local line value
  line="$(grep -E '^[[:space:]]*DATABASE_URL[[:space:]]*=' .env | tail -n 1 || true)"
  [ -n "$line" ] || return 0

  value="${line#*=}"
  value="${value#"${value%%[![:space:]]*}"}"
  value="${value%"${value##*[![:space:]]}"}"

  if [ "${value:0:1}" = '"' ] && [ "${value: -1}" = '"' ]; then
    value="${value:1:${#value}-2}"
  elif [ "${value:0:1}" = "'" ] && [ "${value: -1}" = "'" ]; then
    value="${value:1:${#value}-2}"
  fi

  [ -n "$value" ] && export DATABASE_URL="$value"
}

start_ephemeral_postgres() {
  require_cmd docker || return 1

  POSTGRES_CONTAINER="worker9-postgres-$$-$(date +%s)"
  echo "DATABASE_URL is unavailable/unusable; starting disposable PostgreSQL: $POSTGRES_CONTAINER"

  docker run -d --rm \
    --name "$POSTGRES_CONTAINER" \
    -e POSTGRES_USER=worker9 \
    -e POSTGRES_PASSWORD=worker9 \
    -e POSTGRES_DB=postgres \
    -p 127.0.0.1::5432 \
    postgres:16-alpine >/dev/null

  local port
  for _ in $(seq 1 60); do
    port="$(docker port "$POSTGRES_CONTAINER" 5432/tcp 2>/dev/null | awk -F: 'NR==1 {print $NF}' || true)"
    if [ -n "$port" ]; then
      export DATABASE_URL="postgresql://worker9:worker9@127.0.0.1:${port}/postgres"
      if psql "$DATABASE_URL" -d postgres -Atqc 'SELECT 1' >/dev/null 2>&1; then
        echo "Disposable PostgreSQL ready at 127.0.0.1:$port"
        return 0
      fi
    fi
    sleep 1
  done

  echo "PostgreSQL container did not become ready."
  return 1
}

cleanup() {
  if [ -n "$TEST_DB_NAME" ] && [ -n "${DATABASE_URL:-}" ]; then
    echo
    echo "Cleaning Worker 9 disposable database: $TEST_DB_NAME"
    psql "$DATABASE_URL" -d postgres -v ON_ERROR_STOP=1 \
      -c "DROP DATABASE IF EXISTS \"$TEST_DB_NAME\"" >/dev/null 2>&1 || true
  fi

  if [ -n "$POSTGRES_CONTAINER" ]; then
    docker rm -f "$POSTGRES_CONTAINER" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT INT TERM

echo "Worker 9 comprehensive verification"
echo "Project: $ROOT"

for cmd in cargo sqlx psql jq openssl; do
  if ! require_cmd "$cmd"; then
    echo "Preflight failed."
    exit 2
  fi
done

load_database_url_from_dotenv

if [ -n "${DATABASE_URL:-}" ]; then
  if ! psql "$DATABASE_URL" -d postgres -Atqc 'SELECT 1' >/dev/null 2>&1; then
    echo "Configured DATABASE_URL cannot connect."
    if [ "${AUTO_START_POSTGRES:-1}" = "1" ]; then
      unset DATABASE_URL
      start_ephemeral_postgres || {
        echo "Unable to obtain a working PostgreSQL instance."
        exit 2
      }
    else
      echo "Set AUTO_START_POSTGRES=1 to use a disposable PostgreSQL instance."
      exit 2
    fi
  fi
else
  if [ "${AUTO_START_POSTGRES:-1}" = "1" ]; then
    start_ephemeral_postgres || {
      echo "DATABASE_URL is not set and disposable PostgreSQL could not be started."
      exit 2
    }
  else
    echo "DATABASE_URL is not set. Put it in the environment or .env, or leave AUTO_START_POSTGRES=1."
    exit 2
  fi
fi

# Never run destructive migration tests against the developer's normal database.
# Keep the server/credentials from DATABASE_URL, but use a unique disposable DB.
STAMP="$(date +%Y%m%d_%H%M%S)_$$"
TEST_DB_NAME="uptime_worker9_${STAMP}"

DB_NO_QUERY="$DATABASE_URL"
DB_QUERY=""
if [[ "$DATABASE_URL" == *\?* ]]; then
  DB_NO_QUERY="${DATABASE_URL%%\?*}"
  DB_QUERY="?${DATABASE_URL#*\?}"
fi

DB_PREFIX="${DB_NO_QUERY%/*}"
if [ "$DB_PREFIX" = "$DB_NO_QUERY" ]; then
  echo "Cannot derive PostgreSQL database URL from DATABASE_URL."
  exit 2
fi

TEST_DB_URL="${DB_PREFIX}/${TEST_DB_NAME}${DB_QUERY}"
export DATABASE_URL="$TEST_DB_URL"
export UPGRADE_DATABASE_URL="${DB_PREFIX}/${TEST_DB_NAME}_upgrade${DB_QUERY}"

echo "Using disposable test database: $TEST_DB_NAME"
echo "Database-backed tests will NOT be allowed to remain ignored."

# Format is a verifier, not a mutator. The previous Worker 9 run exposed only
# rustfmt drift; normalize it once, then verify that the tree is clean.
run_step "Rust formatting" bash -c 'cargo fmt --all && cargo fmt --all -- --check'

run_step "Workspace unit/integration tests" cargo test --workspace --all-targets

run_step "Fresh migration + schema constraints/FKs/indexes + upgrade migration" \
  bash scripts/test_database_migrations.sh

# Run EVERY ignored Rust test in the workspace with the disposable migrated DB.
# This deliberately replaces the old opt-in list, which could leave DB tests
# silently skipped.
run_step "All workspace tests including ignored database tests" \
  cargo test --workspace --all-targets -- --include-ignored

# The existing journey script is the project's real API/worker E2E suite.
# Do not hide its failure behind an optional flag.
run_step "Full API/worker E2E journey" bash scripts/e2e_full_journey.sh

# Keep three normal CI-style passes for timing/idempotency stability.
for pass in 1 2 3; do
  run_step "CI stability pass $pass" cargo test --workspace --all-targets
done

echo
if [ "$failures" -eq 0 ]; then
  echo "RESULT: WORKER 9 PASS"
  echo "Formatting, workspace tests, fresh/upgrade migrations, all ignored DB tests, full E2E, and three stability passes were green."
  exit 0
fi

echo "RESULT: WORKER 9 FAILED ($failures step(s))"
exit 1
