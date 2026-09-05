#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

echo "[Worker 4 Fix 4] Restoring infra as application test-only dependency..."

python3 - <<'PY'
from pathlib import Path

path = Path("crates/application/Cargo.toml")
text = path.read_text()

dependency = 'infra = { path = "../infra" }'

if dependency in text:
    print("[Worker 4 Fix 4] infra dev-dependency already present; no edit needed.")
else:
    marker = "[dev-dependencies]\n"

    if marker not in text:
        text = text.rstrip() + "\n\n[dev-dependencies]\n" + dependency + "\n"
    else:
        # Keep infrastructure out of production dependencies.
        text = text.replace(
            marker,
            marker + dependency + "\n",
            1,
        )

    path.write_text(text)
    print("[Worker 4 Fix 4] Added infra under [dev-dependencies].")
PY

echo
echo "===== application/Cargo.toml dependency section ====="
sed -n '/^\[dependencies\]/,/^\[/p' crates/application/Cargo.toml
sed -n '/^\[dev-dependencies\]/,$p' crates/application/Cargo.toml

echo
echo "[Worker 4 Fix 4] Formatting..."
cargo fmt --all

echo
echo "[Worker 4 Fix 4] Checking dependency graph..."
cargo tree -p application -e normal,dev

echo
echo "[Worker 4 Fix 4] Running application tests..."
cargo test -p application

echo
echo "[Worker 4 Fix 4] Running PAYG integration tests..."
cargo test -p infra --test payg_charge_for_usage

echo
echo "[Worker 4 Fix 4] Running all infra tests..."
cargo test -p infra

echo
echo "[Worker 4 Fix 4] Workspace check..."
cargo check --workspace

echo
echo "[Worker 4 Fix 4] PASS"
