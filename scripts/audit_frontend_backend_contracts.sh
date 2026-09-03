#!/usr/bin/env bash
set -euo pipefail
ROOT="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
cd "$ROOT"
echo "== Dashboard contract =="
grep -nE 'DashboardOverview|monitor_summary|incident_summary|/dashboard/overview' apps/leptos-dashboard/src/api/monitoring.rs apps/leptos-dashboard/src/pages/dashboard_overview.rs crates/application/src/dto/dashboard_overview.rs crates/api/src/routes/dashboard.rs
echo
echo "== Protected dashboard route =="
grep -n -B2 -A3 'dashboard::router' crates/api/src/router.rs
echo
echo "== Performance indexes =="
grep -RInE 'idx_monitors_tenant_id|idx_checks_monitor_checked_at|idx_incidents_monitor_resolved_at|idx_incidents_monitor_started_at' migrations
echo
echo "== Frontend API functions =="
grep -RInE '^pub async fn ' apps/leptos-dashboard/src/api | sort
echo
echo "== Backend route declarations =="
grep -RInE '\.route\(' crates/api/src/routes | sort

