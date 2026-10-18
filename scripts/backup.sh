#!/usr/bin/env bash
# PingVertex — Postgres backup script.
#
# Usage:
#   ./scripts/backup.sh [output-dir]
#
# Dumps the production database (via the private 127.0.0.1:5433 port
# docker-compose.yml already binds Postgres to) to a timestamped,
# gzip-compressed custom-format dump. Intended to be run from a host
# cron job or systemd timer (see deploy/systemd/), NOT from inside a
# container — it reads POSTGRES_PASSWORD from the same .env file the
# compose stack uses. Requires the `postgresql-client` package on the
# host (for `pg_dump`), matching the server's major version (16).
set -euo pipefail

OUTPUT_DIR="${1:-/var/backups/pingvertex}"
TIMESTAMP="$(date -u +%Y%m%dT%H%M%SZ)"
FILE="${OUTPUT_DIR}/uptime_monitor_${TIMESTAMP}.dump"

mkdir -p "${OUTPUT_DIR}"

POSTGRES_PASSWORD="$(grep -E '^POSTGRES_PASSWORD=' .env | cut -d '=' -f2-)"

if [ -z "${POSTGRES_PASSWORD}" ]; then
  echo "POSTGRES_PASSWORD not found in .env — aborting." >&2
  exit 1
fi

PGPASSWORD="${POSTGRES_PASSWORD}" pg_dump \
  --host=127.0.0.1 \
  --port=5433 \
  --username=postgres \
  --dbname=uptime_monitor \
  --format=custom \
  --compress=9 \
  --file="${FILE}"

echo "Backup written to ${FILE}"

# Retention: keep the last 14 daily backups, delete older ones.
find "${OUTPUT_DIR}" -name 'uptime_monitor_*.dump' -mtime +14 -delete
