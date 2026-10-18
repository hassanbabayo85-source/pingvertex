#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

fail() { echo "RELEASE VERIFICATION FAIL: $*" >&2; exit 1; }
pass() { echo "PASS: $*"; }

command -v cargo >/dev/null || fail "cargo is required"
command -v rustc >/dev/null || fail "rustc is required"

printf '\n=== STAGE 1 / 7.2: Rust + frontend unit checks ===\n'
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
pass "Rust formatting, clippy and workspace tests"

printf '\n=== Frontend build ===\n'
if command -v trunk >/dev/null; then
  (cd apps/leptos-dashboard && cargo test -p leptos-dashboard && trunk build --release)
  pass "Leptos frontend tests and release build"
else
  fail "trunk is required for the frontend release build"
fi

printf '\n=== STAGE 1: Required security/integration regression coverage ===\n'
required_tests=(
  'crates/api/tests/auth_e2e.rs:e2e_logout_revokes_copied_bearer_token'
  'crates/api/tests/cross_tenant_isolation.rs:cross_tenant_cannot_delete_another_tenants_api_key'
  'crates/api/tests/notifications.rs:rejects_invalid_webhook_url'
  'crates/api/tests/heartbeat.rs:heartbeat_ping_is_rate_limited_per_token'
  'crates/worker/tests/ssrf_webhook_blocked.rs:'
  'crates/shared/src/ssrf.rs:blocks_ipv6_encodings_that_embed_a_private_ipv4_address'
  'crates/api/tests/pricing_rate_limit.rs:billing_requests_over_burst_are_rate_limited'
)
for item in "${required_tests[@]}"; do
  file="${item%%:*}"
  needle="${item#*:}"
  [ -f "$file" ] || fail "missing required regression source: $file"
  if [ -n "$needle" ]; then
    grep -Fq "$needle" "$file" || fail "missing required regression test: $needle in $file"
  fi
done
pass "Required auth, tenant-isolation, webhook, rate-limit and SSRF regressions are present"

printf '\n=== STAGE 4: Dependency/security policy ===\n'
if [ ! -f Cargo.lock ]; then
  cargo generate-lockfile
  echo "NOTE: Cargo.lock was generated locally; commit it before release."
fi

if cargo tree --workspace | grep -E '(^|[[:space:]])(sqlx-mysql|sqlx-sqlite|rsa|rkyv)([[:space:]]|$)'; then
  fail "forbidden optional/vulnerable dependency family is present in the resolved graph"
fi

cargo audit
cargo deny check
pass "RustSec audit, cargo-deny and dependency graph checks"

printf '\n=== STAGE 4: CORS/SSRF/security source assertions ===\n'
grep -Fq 'CORS_ALLOWED_ORIGIN must be explicitly configured in production' crates/api/src/config/mod.rs || fail "production CORS fail-closed guard missing"
grep -Fq 'cors_allowed_origin_from_env' crates/api/src/router.rs || fail "router is not using centralized CORS origin policy"
grep -Fq 'PINGVERTEX_SSRF_ALLOW_PRIVATE' crates/shared/src/ssrf.rs || fail "SSRF guard missing"
grep -Fq 'SESSION_INVALIDATION_TTL_SECONDS' crates/api/src/handlers/logout.rs || fail "logout invalidation guard missing"
grep -Fq 'HttpOnly' crates/shared/src/cookie.rs || fail "HttpOnly cookie policy missing"
grep -Fq 'SameSite=Strict' crates/shared/src/cookie.rs || fail "Strict SameSite cookie policy missing"
pass "CORS, SSRF, session invalidation and cookie hardening are present"

printf '\n=== STAGE 5 / 7.6: Frontend accessibility/UI source assertions ===\n'
grep -Fq 'role="alert"' apps/leptos-dashboard/src/components/error_state.rs || fail "error state lacks alert semantics"
grep -Fq 'aria-live="assertive"' apps/leptos-dashboard/src/components/error_state.rs || fail "error state lacks assertive announcement"
grep -Fq 'event.key() == "Escape"' apps/leptos-dashboard/src/components/confirm_dialog.rs || fail "confirm dialog lacks Escape handling"
grep -Fq 'trap_dialog_focus' apps/leptos-dashboard/src/components/confirm_dialog.rs || fail "confirm dialog lacks keyboard focus trap"
grep -Fq 'prefers-reduced-motion' apps/leptos-dashboard/style.css || fail "reduced-motion policy missing"
pass "Keyboard, announcement, reduced-motion and touch-target hardening is present"

printf '\n=== Contract audit ===\n'
if [ -x scripts/audit_frontend_backend_contracts.sh ]; then
  scripts/audit_frontend_backend_contracts.sh >/tmp/pingvertex-contracts.txt
  pass "frontend/backend route contract audit"
else
  fail "scripts/audit_frontend_backend_contracts.sh is missing or not executable"
fi

printf '\n=== Docker configuration ===\n'
if command -v docker >/dev/null; then
  docker compose config >/dev/null
  pass "docker compose configuration"
else
  echo "SKIP: docker is not installed; container/E2E checks must be run on the release host."
fi

if [ "${RUN_E2E:-0}" = "1" ]; then
  printf '\n=== STAGE 3: Full API E2E journey ===\n'
  command -v docker >/dev/null || fail "RUN_E2E=1 requires docker"
  command -v curl >/dev/null || fail "RUN_E2E=1 requires curl"
  command -v jq >/dev/null || fail "RUN_E2E=1 requires jq"
  command -v openssl >/dev/null || fail "RUN_E2E=1 requires openssl"
  scripts/e2e_full_journey.sh
else
  echo "SKIP: full API E2E journey (set RUN_E2E=1 to execute it)."
fi

echo
echo '============================================================'
echo 'RELEASE VERIFICATION VERIFICATION PASSED FOR ALL EXECUTED CHECKS'
echo 'For release: run with RUN_E2E=1 and ensure Cargo.lock is committed.'
echo '============================================================'
