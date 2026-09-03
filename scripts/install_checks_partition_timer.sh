#!/usr/bin/env bash
# PingVertex — AUDIT ISSUE #11 fix: "partition timer deployment is
# manual / not guaranteed".
#
# deploy/systemd/pingvertex-checks-partition.{service,timer} have existed
# in this repo since migration 0068, but nothing automated actually
# installing them — an administrator had to remember to run
# `systemctl daemon-reload` and `systemctl enable --now
# pingvertex-checks-partition.timer` by hand after deploying. This script
# does that installation step, idempotently, so it can be wired into a
# deployment pipeline instead of relying on a human remembering it.
#
# This intentionally does NOT run automatically as part of any other
# script (in particular not from ensure_checks_partitions.sh, which now
# only WARNS if the timer looks inactive — see AUDIT ISSUE #12's health
# check there) — installing a systemd unit is a privileged, host-level
# action that a deploy pipeline or administrator should invoke
# explicitly, not something a routine maintenance script should do on
# its own.
set -euo pipefail

UNIT_DIR="/etc/systemd/system"
SERVICE_NAME="pingvertex-checks-partition.service"
TIMER_NAME="pingvertex-checks-partition.timer"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if ! command -v systemctl >/dev/null 2>&1; then
  echo "systemctl not found — this host does not appear to use systemd." >&2
  echo "Install/schedule scripts/ensure_checks_partitions.sh via this host's" >&2
  echo "own scheduler (cron, launchd, a container-level scheduler, etc.) instead." >&2
  exit 1
fi

if [ "$(id -u)" -ne 0 ]; then
  echo "This script installs system-wide systemd units and must be run as root (sudo)." >&2
  exit 1
fi

for unit in "${SERVICE_NAME}" "${TIMER_NAME}"; do
  src="${REPO_ROOT}/deploy/systemd/${unit}"
  if [ ! -f "${src}" ]; then
    echo "Expected unit file not found: ${src}" >&2
    exit 1
  fi
  echo "Installing ${unit} -> ${UNIT_DIR}/${unit}"
  install -m 0644 "${src}" "${UNIT_DIR}/${unit}"
done

echo "NOTE: ${SERVICE_NAME}'s WorkingDirectory defaults to"
echo "      /opt/pingvertex/uptime-monitor — edit ${UNIT_DIR}/${SERVICE_NAME}"
echo "      first if this deployment lives elsewhere, then re-run this script."

systemctl daemon-reload
systemctl enable --now "${TIMER_NAME}"

if systemctl is-enabled --quiet "${TIMER_NAME}" && systemctl is-active --quiet "${TIMER_NAME}"; then
  echo "OK: ${TIMER_NAME} is enabled and active."
  systemctl status --no-pager "${TIMER_NAME}" || true
else
  echo "WARNING: ${TIMER_NAME} was installed but is not both enabled and active." >&2
  echo "         Check 'systemctl status ${TIMER_NAME}' and 'journalctl -u ${SERVICE_NAME}'." >&2
  exit 1
fi
