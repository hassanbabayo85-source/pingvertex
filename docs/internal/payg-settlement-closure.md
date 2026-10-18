# GAP 3 — Kashi C: Per-Tenant Retention — CLOSED

**Status:** ✅ CLOSED
**Date:** 2026-09-13
**Scope:** Per-tenant history retention with legacy grandfathering.
**Follows:** GAP 3 Kashi A, Kashi B, Kashi D.

## Objective

Give every tenant an explicit, per-tenant history-retention setting,
default 30 days for PAYG tenants, with a hard upper bound of 3650 days
(`pricing_configs.max_retention_days`). Preserve existing tenants'
historical retention values via grandfathering so the retention worker
does not delete their history on the first sweep.

## What was delivered

### 1. New column: `pricing_configs.default_retention_days`

Migration 0117 adds `default_retention_days INTEGER NOT NULL DEFAULT 30`
to `pricing_configs`. This is DDL, so the append-only row-level trigger
is unaffected. `PricingConfig` carries the field through Rust.

### 2. New table: `tenant_retention_settings`

One row per tenant:

| Column | Notes |
|--------|-------|
| `tenant_id` | PK, FK tenants |
| `retention_days` | `1 <= retention_days <= 3650` |
| `source` | `default` / `legacy` / `addon` |
| `created_at` / `updated_at` | |

`RetentionSource::priority()` orders writers: Addon > Legacy > Default.
The repository's `upsert` refuses to downgrade.

### 3. Grandfathering migration

The migration seeds one row per tenant:

- `subscriptions.retention_days IS NOT NULL` -> that value, source
  `legacy`. (Zero rows today; the column exists but no tenant has ever
  set it.)
- Otherwise -> `pricing_configs.default_retention_days`, source
  `default`.

`ON CONFLICT DO NOTHING` makes the seed idempotent and safe to re-run.

### 4. `TenantRetentionSettingsRepository`

    async fn get(tenant_id) -> Option<TenantRetentionSettings>
    async fn upsert(&settings)              // priority-aware
    async fn effective_retention(tenant, default_retention_days, max_retention_days)

`effective_retention` returns the tenant's row value (clamped to
`[1, max_retention_days]`), falling back to `default_retention_days`.

### 5. `RetentionWorker` uses the effective value

The worker resolves each tenant's retention in this order:

1. PAYG-activated -> `effective_retention()` (30 by default).
2. Non-PAYG with a `legacy`/`addon` row -> keep that value.
3. All other non-PAYG -> `free_tier.retention_days` (2 days, unchanged).

PAYG is detected via `user_wallets.payg_activated_at`.

### 6. API exposure

- `GET /api/v1/billing/allowance` now also returns:
  `retention_days`, `max_retention_days`, `default_retention_days`,
  `retention_source`.
- `GET /api/v1/payg/pricing`'s `free_tier` now also returns
  `max_retention_days` and `default_retention_days`.

### 7. Dashboard

- Billing page renders a "History retention" row inside the fair-use
  allowance card, plus a "Retention source" row.
- Pricing page's "Free entitlement" paragraph explains the free-tier
  retention and the PAYG default/max.

## Files changed

New:

- `migrations/0117_retention_settings.sql`
- `crates/domain/src/entities/tenant_retention_settings.rs`
- `crates/application/src/ports/tenant_retention_settings_repository.rs`
- `crates/infra/src/repositories/postgres_tenant_retention_settings_repository.rs`
- `crates/infra/src/repositories/in_memory_tenant_retention_settings_repository.rs`
- `crates/infra/tests/tenant_retention_settings.rs` (10 tests)
- `docs/internal/payg-settlement-closure.md` (this file)

Modified:

- `crates/domain/src/entities/mod.rs`
- `crates/domain/src/pricing/config.rs`
- `crates/application/src/ports/mod.rs`
- `crates/infra/src/repositories/mod.rs`
- `crates/infra/src/repositories/postgres_pricing_config_repository.rs`
- `crates/worker/src/retention_worker.rs`
- `crates/worker/src/main.rs`
- `crates/worker/tests/retention_worker.rs`
- `crates/worker/tests/worker_graceful_shutdown.rs`
- `crates/api/src/state.rs`
- `crates/api/src/main.rs`
- `crates/api/src/handlers/billing_allowance.rs`
- `crates/api/src/handlers/payg_pricing.rs`
- `crates/api/src/handlers/pricing_types.rs`
- `crates/api/src/handlers/admin_pricing.rs`
- `apps/leptos-dashboard/src/api/billing.rs`
- `apps/leptos-dashboard/src/pages/billing.rs`
- `apps/leptos-dashboard/src/pages/pricing.rs`

## Tests

| Suite | Result |
|-------|--------|
| `infra::tenant_retention_settings` | 10/10 |
| `worker::retention_worker` | 5/5 |
| `worker::worker_graceful_shutdown` | 6/6 |
| Full workspace build (`--all-targets`) | pass |
| Dashboard wasm build | pass |

## Design notes

- Priority-aware upsert: a `Default` writer cannot clobber an existing
  `Legacy` or `Addon` row. This preserves grandfathering and paid
  add-ons across concurrent writers.
- Enabled-only monitors: the retention worker's own decision is
  independent of monitor count; only the PAYG-activated flag matters.
  Monitor count drives the SMS allowance (Kashi D), not retention.
- Non-PAYG grandfathering: a non-PAYG tenant that had 3650-day legacy
  retention keeps it. This is enforced by the worker reading
  `retention_source` explicitly, not by inferring from
  `payg_activated_at`.

## Not in this phase

- Extended-retention purchase flow: the `Addon` source and priority
  exist; the checkout/payment flow to actually sell 90/180/365-day
  tiers is deferred to a follow-up.
- RUM session-based pricing: Kashi E.
