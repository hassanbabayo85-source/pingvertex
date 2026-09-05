#!/usr/bin/env bash
set -euo pipefail

ROOT="${1:-.}"
ROOT="$(cd "$ROOT" && pwd)"
cd "$ROOT"

if [[ ! -f "Cargo.toml" || ! -f "deny.toml" ]]; then
  echo "ERROR: run this from the PingVertex workspace root."
  exit 1
fi

STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
BACKUP=".phase7-cargo-deny-schema-fix-backup-$STAMP"
mkdir -p "$BACKUP"
cp deny.toml "$BACKUP/deny.toml"

echo "Backup created: $ROOT/$BACKUP"
echo "Fixing cargo-deny advisory scope..."

python3 - <<'PY'
from pathlib import Path

p = Path("deny.toml")
text = p.read_text()

old = 'unmaintained = "warn"'
new = 'unmaintained = "workspace"'

if old in text:
    text = text.replace(old, new, 1)
    p.write_text(text)
    print('Changed unmaintained = "warn" -> unmaintained = "workspace".')
elif 'unmaintained = "workspace"' in text:
    print('deny.toml already uses unmaintained = "workspace".')
else:
    raise SystemExit("ERROR: could not find the cargo-deny unmaintained setting.")
PY

echo
echo "Validating deny.toml..."
if command -v cargo-deny >/dev/null 2>&1; then
  cargo deny check
else
  echo "ERROR: cargo-deny is not installed."
  exit 1
fi

echo
echo "cargo-deny now passes."
echo
echo "Continue with the complete Phase 7 release gate:"
echo "  cargo clippy --workspace --all-targets -- -D warnings"
echo "  cargo test --workspace --all-targets"
echo "  RUN_E2E=1 ./scripts/phase7_verify.sh"
