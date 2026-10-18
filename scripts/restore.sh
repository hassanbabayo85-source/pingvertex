#!/usr/bin/env bash
# PingVertex — Postgres restore script.
#
# Usage:
#   ./scripts/restore.sh /var/backups/pingvertex/uptime_monitor_TIMESTAMP.dump
#
# DESTRUCTIVE: drops and recreates the `uptime_monitor` database before
# restoring. Prompts for confirmation. Run against the private
# 127.0.0.1:5433 Postgres port, same as backup.sh. Requires
# `postgresql-client` on the host.
set -euo pipefail

DUMP_FILE="${1:?Usage: ./scripts/restore.sh path/to/dump.file}"

if [ ! -f "${DUMP_FILE}" ]; then
  echo "Dump file not found: ${DUMP_FILE}" >&2
  exit 1
fi

POSTGRES_PASSWORD="$(grep -E '^POSTGRES_PASSWORD=' .env | cut -d '=' -f2-)"

if [ -z "${POSTGRES_PASSWORD}" ]; then
  echo "POSTGRES_PASSWORD not found in .env — aborting." >&2
  exit 1
fi

read -r -p "This will DROP and recreate 'uptime_monitor' on 127.0.0.1:5433, then restore from ${DUMP_FILE}. Type 'yes' to continue: " CONFIRM
if [ "${CONFIRM}" != "yes" ]; then
  echo "Aborted."
  exit 1
fi

export PGPASSWORD="${POSTGRES_PASSWORD}"

dropdb --host=127.0.0.1 --port=5433 --username=postgres --if-exists uptime_monitor
createdb --host=127.0.0.1 --port=5433 --username=postgres uptime_monitor

pg_restore \
  --host=127.0.0.1 \
  --port=5433 \
  --username=postgres \
  --dbname=uptime_monitor \
  --no-owner \
  --jobs=4 \
  "${DUMP_FILE}"

echo "Restore complete."
