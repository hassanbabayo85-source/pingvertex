#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."
command -v cargo >/dev/null || { echo "ERROR: cargo is required" >&2; exit 1; }

printf '\n[1/8] Formatting check\n'
cargo fmt --all -- --check

printf '\n[2/8] Generate Cargo.lock\n'
if [ ! -f Cargo.lock ]; then cargo generate-lockfile; fi

printf '\n[3/8] Dependency graph and advisory scan\n'
if cargo tree --workspace | grep -E '(^|[[:space:]])(sqlx-mysql|sqlx-sqlite|rsa|rkyv)([[:space:]]|$)'; then
  echo 'ERROR: forbidden optional/vulnerable dependency is present in the resolved workspace graph.' >&2
  exit 1
fi
cargo audit

printf '\n[4/8] Cargo deny\n'
cargo deny check

printf '\n[5/8] Clippy\n'
cargo clippy --workspace --all-targets -- -D warnings

printf '\n[6/8] Full test suite\n'
cargo test --workspace

printf '\n[7/8] SSRF regression tests\n'
cargo test -p shared ssrf::tests -- --nocapture

printf '\n[8/8] Source/secret hygiene\n'
if find . -maxdepth 1 -type d \( -name '.architecture-security-backup-*' -o -name '.security-patch-backup-*' -o -name '.dependency-security-backup-*' \) | grep -q .; then
  echo 'ERROR: security backup directories must not remain in the repository root.' >&2
  exit 1
fi
if [ -f overview.xml ]; then
  echo 'ERROR: overview.xml must not remain in the repository root.' >&2
  exit 1
fi

echo
echo 'SECURITY VERIFICATION PASSED'
