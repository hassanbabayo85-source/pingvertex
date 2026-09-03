#!/usr/bin/env bash
# PingVertex — ensures `checks` has partitions for the next N months.
# Companion to migration 0068 (ensure_checks_partition()) and migration
# 0071 (checks_partition_maintenance() / reconcile_checks_default_
# partition() / checks_partition_health() — AUDIT ISSUES #4 / #10 / #12).
# Intended to run from a host cron job / systemd timer (see
# deploy/systemd/), same pattern as scripts/backup.sh.
set -euo pipefail

MONTHS_AHEAD="${1:-3}"

POSTGRES_PASSWORD="$(grep -E '^POSTGRES_PASSWORD=' .env | cut -d '=' -f2-)"
if [ -z "${POSTGRES_PASSWORD}" ]; then
  echo "POSTGRES_PASSWORD not found in .env — aborting." >&2
  exit 1
fi

run_psql() {
  PGPASSWORD="${POSTGRES_PASSWORD}" psql \
    --host=127.0.0.1 \
    --port=5433 \
    --username=postgres \
    --dbname=uptime_monitor \
    "$@"
}

# AUDIT ISSUE #4 fix: use checks_partition_maintenance() instead of
# ensure_checks_partition() alone — it creates the target month's
# partition (same as before) AND reconciles any rows that previously
# fell into checks_default because their partition didn't exist yet at
# insert time. Safe to run every time: reconciliation is a no-op once a
# partition has already been swept clean.
for i in $(seq 0 "${MONTHS_AHEAD}"); do
  run_psql -c "SELECT checks_partition_maintenance((CURRENT_DATE + (INTERVAL '1 month' * ${i}))::date);"
done

echo "checks partitions ensured through $(date -u -d "+${MONTHS_AHEAD} months" +%Y-%m 2>/dev/null || date -u -v+${MONTHS_AHEAD}m +%Y-%m)"

# AUDIT ISSUE #12 fix: surface partition health every run, not just on
# request — an operator watching this job's logs (or scraping them) now
# gets an explicit signal instead of having to know to ask. A non-zero
# checks_default_row_count means rows landed in the default partition at
# some point (partition maintenance fell behind before this run) and is
# worth alerting on even though this run's reconciliation above should
# have just cleared it.
echo "--- partition health ---"
run_psql -c "SELECT * FROM checks_partition_health(${MONTHS_AHEAD});"

# AUDIT ISSUE #11 fix: this script being deployed does not guarantee the
# systemd timer that's supposed to invoke it on a schedule is actually
# enabled (see deploy/systemd/pingvertex-checks-partition.{service,timer}
# and scripts/install_checks_partition_timer.sh). When systemctl is
# available on the host this runs on, check the timer's own state and
# warn loudly (without failing this run — the partition work above still
# succeeded) if it isn't active, since an operator running this script
# manually is exactly the situation where the timer might have silently
# never been installed.
if command -v systemctl >/dev/null 2>&1; then
  if ! systemctl is-enabled --quiet pingvertex-checks-partition.timer 2>/dev/null \
     || ! systemctl is-active --quiet pingvertex-checks-partition.timer 2>/dev/null; then
    echo "WARNING: pingvertex-checks-partition.timer is not enabled+active on this host." >&2
    echo "         Future partitions will not be created automatically until it is." >&2
    echo "         Run scripts/install_checks_partition_timer.sh to install it." >&2
  fi
fi

