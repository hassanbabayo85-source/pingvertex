# GAP 3 — Kashi A: PAYG Unit Switch to Nano-USD + Per-Kind Pricing + Storage Retirement — CLOSED

**Status:** ✅ CLOSED
**Date:** 2026-09-13
**Scope:** PAYG unit change, per-kind check pricing, storage operation retirement.
**Follows:** GAP 1 (One-Time Check), GAP 2A (Proxy Recurring).

## Objective

Switch the PAYG runtime unit from micro-USD (1 = $0.000001) to nano-USD
(1 = $10^-9), enabling the newly-approved per-kind check pricing
(HTTP=1000, TCP=700, DNS=500) without fractional prices. Simultaneously
retire the `storage` PAYG operation: the per-check price now covers
storage, and status-page logo uploads are free.

## What was delivered

### 1. Unit switch (micro-USD → nano-USD, ×1000)

`migrations/0114_switch_payg_unit_to_nano_usd.sql` scales every historical
financial row in one transaction:

- `user_wallets.balance_units` × 1000
- `wallet_transactions.amount_units` and `balance_after_units` × 1000
- `credit_reservations.amount_units` and `unit_price` × 1000
- `api_usage_events.unit_price` and `charge_units` × 1000

A pre-flight `DO $$` block refuses the migration if any existing value
exceeds 9×10^15 (the i64 overflow threshold after ×1000). All prices are
inserted in the same transaction so no runtime path can observe a
mixed-unit state.

### 2. Per-kind check pricing (nano-USD)

Migration 0114 also inserts a new price version:

| Operation | nano-USD | USD |
|-----------|---------:|----:|
| `dns_check.one_time` / `.monitoring` | 500 | $0.0000005 |
| `tcp_check.one_time` / `.monitoring` | 700 | $0.0000007 |
| `http_check.one_time` / `.monitoring` | 1000 | $0.0000010 |
| `api_flow_check.one_time` / `.monitoring` | 1000 | (base rate) |
| `proxy_check.one_time` / `.monitoring` | 2000 | (unchanged relative) |
| `heartbeat_processing` | 1000 | |
| `rum_event` | 1000 | |
| `sms` | 1000000 | |
| `network_egress` | 100000 | |

The historical micro-USD pricing rows remain queryable (the price book is
append-only). `list_active()` collapses to the newest version per
operation via `DISTINCT ON (operation) ... ORDER BY version DESC`.

### 3. `MIN_PAYG_TOP_UP_UNITS` updated

`crates/domain/src/payg.rs` — $5 now equals 5_000_000_000 nano-USD
(was 5_000_000 micro-USD).

### 4. Storage operation retired

`migrations/0115_retire_storage_operation.sql` adds a row to
`payg_retired_operations` for `storage`. `PaygOperation::ALL` drops from
15 to 14 — the `Storage` variant is removed, and `storage` is added to
`RETIRED_PAYG_OPERATIONS`.

The historical pricing rows for `storage` (migrations 0079 / 0091 / 0093
/ 0114) remain queryable for audit. `list_active()` no longer advertises
the operation.

### 5. Logo uploads are now free

`UploadStatusPageLogoUseCase` no longer takes a `MeterUsageUseCase`, and
the `with_metering()` builder is gone. The API handler
(`crates/api/src/handlers/upload_status_page_logo.rs`) no longer wraps
the upload in a metering reservation. Logo uploads succeed without a
PAYG balance.

The `UploadStatusPageLogoError::InsufficientBalance` variant was removed
since no code path can produce it.

### 6. Dashboard display update

All six dashboard `usd()` functions (`pricing.rs`, `usage.rs`,
`wallet.rs`, `transactions.rs`, `billing.rs`) now divide by
1_000_000_000 (nano-USD) instead of 1_000_000 (micro-USD). The storage
label and card were removed from `pricing.rs`, `usage.rs`, and the
`platform_admin.rs` operation select.

## Files changed

**New:**

- `migrations/0114_switch_payg_unit_to_nano_usd.sql`
- `migrations/0115_retire_storage_operation.sql`
- `docs/internal/payg-metering-closure.md` (this file)

**Modified:**

- `crates/domain/src/payg.rs`
- `crates/application/src/use_cases/upload_status_page_logo.rs`
- `crates/application/tests/upload_status_page_logo_use_case.rs`
- `crates/api/src/handlers/upload_status_page_logo.rs`
- `crates/api/src/handlers/payg_pricing.rs`
- `crates/api/tests/pricing.rs`
- `crates/infra/tests/wallet_payg.rs`
- `crates/infra/tests/postgres_payg_retired_operations.rs`
- `apps/leptos-dashboard/src/pages/pricing.rs`
- `apps/leptos-dashboard/src/pages/usage.rs`
- `apps/leptos-dashboard/src/pages/platform_admin.rs`
- `apps/leptos-dashboard/src/pages/wallet.rs`
- `apps/leptos-dashboard/src/pages/transactions.rs`
- `apps/leptos-dashboard/src/pages/billing.rs`

## Tests

| Suite | Result |
|-------|--------|
| `domain::payg` | 5/5 |
| `infra::postgres_payg_retired_operations` (non-ignored) | 1/1 |
| `infra::wallet_payg` | 7/7 |
| Full workspace build | pass |
| Full workspace test | pass |
| `cargo clippy --workspace --all-targets` | clean |
| Dashboard wasm build | pass |

## Deployment note

**Production DB is at migration version 106; test DB is at 115.** Before
this migration can be applied to production, migrations 107–113 must be
applied first. Deployment sequencing is out of scope for this closure.

## Not in this phase

- **Flow steps-based pricing** (units = steps_len × HTTP rate) — Kashi B
- **SMS/Voice fair-use allowance** — Kashi D
- **Extended retention add-on** — Kashi C
- **RUM session-based pricing** — Kashi E

## Pre-existing state

- `proxy_check.monitoring` remains canonical (GAP 2A).
- `monitor_check`, `heartbeat_ping`, `proxy_check`, `SMS`, and `storage`
  are in `RETIRED_PAYG_OPERATIONS` and are never billed at runtime.
- Historical pricing rows for every retired/non-canonical operation
  remain in `payg_pricing_configs` (immutable, audit-preserving).
