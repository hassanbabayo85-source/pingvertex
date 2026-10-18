# GAP 2 — Phase A: Proxy Recurring Restoration — CLOSED

**Status:** ✅ CLOSED
**Date:** 2026-09-13
**Scope:** Proxy recurring monitoring operation + scheduler billing.
**Follows:** GAP 1 (Complete One-Time Check System).

## Objective

Restore `proxy_check.monitoring` as a first-class recurring PAYG
operation, distinct from the one-shot `proxy_check.one_time`, and route
the recurring proxy worker's per-entry billing through the canonical
monitoring operation name.

## What was delivered

### 1. Canonical operation restored

`proxy_check.monitoring` is again canonical in
`crates/domain/src/payg.rs`. The canonical set is now 15 operations.
The operation is no longer listed in `RETIRED_PAYG_OPERATIONS`.

### 2. Retirement filter lifted

Migration `0113_restore_proxy_monitoring.sql` removes the
`payg_retired_operations` row that migration 0110 inserted. The
immutable historical pricing rows for `proxy_check.monitoring`
(inserted by migrations 0091 and 0093) remain queryable for
audit/history and are now exposed by
`PostgresPAYGPricingRepository::list_active()`.

This is forward-only: the pricing rows themselves are not modified or
deleted. Only the retirement filter row is removed.

### 3. Recurring proxy worker billing corrected

`crates/worker/src/proxy_checker.rs` was billing recurring proxy
entries under `proxy_check.one_time` — a latent bug introduced when
`proxy_check.monitoring` was retired in GAP 1's migrations. The
recurring path now uses `proxy_check.monitoring`, so a recurring proxy
job charges at the recurring price and records a recurring usage event.

The one-time proxy path (Test Now) is unchanged and continues to bill
under `proxy_check.one_time`.

### 4. Test infrastructure fix

`crates/infra/src/test_support.rs::ensure_test_schema` used to skip
migrations entirely once `_sqlx_migrations` existed in a schema. This
silently left newly-added migration files un-applied on test schemas
that had already been bootstrapped by an earlier test-binary run — for
example, migration 0113 was not applied, so the retirement test
asserted against stale state.

The function now always calls `sqlx::migrate!().run()`, which is
idempotent and only applies pending migrations. This is a test-harness
fix, not a product behavior change.

### 5. Dashboard pricing display

`apps/leptos-dashboard/src/pages/pricing.rs` now shows
`proxy_check.monitoring` as "Proxy — monitoring".

## Files changed

**New:**

- `migrations/0113_restore_proxy_monitoring.sql`
- `docs/GAP_2_PHASE_A_CLOSURE.md` (this file)

**Modified:**

- `crates/domain/src/payg.rs`
- `crates/worker/src/proxy_checker.rs`
- `crates/infra/src/test_support.rs`
- `crates/infra/tests/postgres_payg_retired_operations.rs`
- `apps/leptos-dashboard/src/pages/pricing.rs`

## Definition of Done (from GAP 2 blueprint)

| Item | Status |
|------|--------|
| `proxy_check.monitoring` active | ✅ |
| Proxy scheduler billing integrated | ✅ (corrected to use the monitoring op) |
| Proxy one-time remains separate | ✅ |
| Old runtime operation names removed | ✅ (no bare `proxy_check` usage at runtime; only in `RETIRED_PAYG_OPERATIONS` and historical migrations) |

## Tests

| Suite | Result |
|-------|--------|
| `domain::payg` | 4/4 |
| `infra::postgres_payg_retired_operations` | 2/2 |
| Full workspace build | pass |
| Full workspace test | pass |
| `cargo clippy --workspace --all-targets` | clean |

Note: `worker::notification_sender::tests::rate_limiter_resets_after_window_expiry`
is intermittently flaky under full-suite load — it uses fixed
`Duration::from_millis(10)` windows and a 20ms sleep. It passes 3/3
when run in isolation. It is unrelated to GAP 2.

## Not in this phase (Phase B)

- **Storage recurring MB/month.** GAP 2 requires storage usage to be
  metered per billing period. The current system charges `storage` as a
  one-time write fee in `upload_status_page_logo.rs`. Turning this into
  a recurring MB-month charge requires a product decision on what
  "stored MB" measures (logos only vs. all tenant data vs. the legacy
  `effective_storage_bytes_per_check` formula). This is deferred to
  Phase B.
- **Wallet reconciliation verification for storage.** Depends on
  Phase B.
- **Storage duplicate prevention, price-version preservation.** Same.

## Pre-existing state

- `proxy_check.one_time` was, and remains, canonical and billable
  through the one-shot proxy Test Now path.
- `monitor_check`, `heartbeat_ping`, `proxy_check`, and `SMS` remain
  in `RETIRED_PAYG_OPERATIONS` and are never billed at runtime.
- Historical pricing rows for retired and non-canonical operations
  remain in `payg_pricing_configs` (immutable, audit-preserving).
