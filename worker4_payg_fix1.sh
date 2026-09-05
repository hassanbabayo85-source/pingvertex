#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

echo "[Worker 4 Fix 1] Fixing ReservationOutcome export and unused import..."

python3 - <<'PY'
from pathlib import Path

mod = Path('crates/application/src/ports/mod.rs')
p = mod.read_text()

old = 'pub use wallet_repository::{WalletRepoResult, WalletRepository};'
new = 'pub use wallet_repository::{ReservationOutcome, WalletRepoResult, WalletRepository};'

if old not in p:
    raise SystemExit(
        'Expected wallet_repository re-export line was not found; '
        'aborting without changes.'
    )

mod.write_text(p.replace(old, new, 1))

pricing = Path('crates/application/src/ports/payg_pricing_repository.rs')
p = pricing.read_text()
p = p.replace('use uuid::Uuid;\n', '', 1)
pricing.write_text(p)
PY

echo "[Worker 4 Fix 1] Running cargo fmt..."
cargo fmt --all

echo "[Worker 4 Fix 1] Running infra tests..."
cargo test -p infra

echo "[Worker 4 Fix 1] Running application tests..."
cargo test -p application

echo "[Worker 4 Fix 1] Running workspace check..."
cargo check --workspace

echo "[Worker 4 Fix 1] PASS"
