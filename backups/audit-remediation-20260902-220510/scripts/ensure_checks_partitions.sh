#!/usr/bin/env bash
# PingVertex — ensures `checks` has partitions for the next N months.
# Companion to migration 0068 (ensure_checks_partition()). Intended to
# run from a host cron job / systemd timer (see deploy/systemd/), same
# pattern as scripts/backup.sh.
set -euo pipefail

MONTHS_AHEAD="${1:-3}"

POSTGRES_PASSWORD="$(grep -E '^POSTGRES_PASSWORD=' .env | cut -d '=' -f2-)"
if [ -z "${POSTGRES_PASSWORD}" ]; then
  echo "POSTGRES_PASSWORD not found in .env — aborting." >&2
  exit 1
fi

for i in $(seq 0 "${MONTHS_AHEAD}"); do
  PGPASSWORD="${POSTGRES_PASSWORD}" psql \
    --host=127.0.0.1 \
    --port=5433 \
    --username=postgres \
    --dbname=uptime_monitor \
    -c "SELECT ensure_checks_partition((CURRENT_DATE + (INTERVAL '1 month' * ${i}))::date);"
done

echo "checks partitions ensured through $(date -u -d "+${MONTHS_AHEAD} months" +%Y-%m 2>/dev/null || date -u -v+${MONTHS_AHEAD}m +%Y-%m)"
