# GAP — Add-on Billing Restoration — CLOSED

**Status:** ✅ CLOSED
**Date:** 2026-09-13
**Scope:** Restore SMS credits, Enterprise SLA, and Extended Retention add-ons under PAYG.
**Follows:** GAP 3 Kashi A (nano-USD), Kashi B (flow steps), Kashi C (retention), Kashi D (SMS allowance), Kashi E (RUM session pricing).

## Objective

Restore the add-on billing surface that PAYG had retired behind
`ApiError::gone("subscription add-ons are retired; use PAYG billing")`.
Add-ons are purchased through `PaymentProvider` checkout (Paystack), not
from the wallet, and the runtime provides:

- **SMS credits** — per-country pricing sourced from Twilio plus a
  configurable markup.
- **Enterprise SLA** — a flat monthly price from `PricingConfig`.
- **Extended Retention** — 90/180/365-day tiers, priced dynamically per
  tenant from the number of enabled monitors.

The retired `AddOnType::Rum` variant is replaced by
`AddOnType::ExtendedRetention`.

## What was delivered

### 1. Domain

- `AddOnType::Rum` removed; `AddOnType::ExtendedRetention` added
  (`as_str() == "retention"`).
- New `RetentionTier` enum (`D90`, `D180`, `D365`) with `as_str()`
  (`"90" | "180" | "365"`), `days()` (`90 | 180 | 365`), and `FromStr`.
- New `compute_storage_delta(monitors, interval, tier_days, config)`
  in `domain::pricing::calculation`: `compute_storage(tier) -
  compute_storage(default_30)`, floored at zero.
- `CheckoutSession.plan_tier: Option<PlanTier>`:
  `None` for add-on checkouts, `Some(tier)` for subscription checkouts.
- `RetentionSource::Addon` priority (2) > `Legacy` (1) > `Default` (0)
  is preserved verbatim.

### 2. Application

- `CheckoutRequest.plan_tier: Option<PlanTier>`.
- New `BillingError::Invalid(String)` for bad-session cases (not user
  error, not provider error); mapped to `ApiError::conflict`.
- `initiate_checkout` and `change_subscription_plan` set
  `plan_tier = Some(tier)`; add-on paths set `None`.
- `verify_checkout` rejects a session without a plan tier via
  `BillingError::Invalid`.
- Add-on verification uses the separate
  `VerifyAddOnCheckoutUseCase` (`verify_addon_checkout.rs`).

### 3. Persistence

- **Migration `0119_checkout_sessions_plan_tier_nullable.sql`**:
  `ALTER TABLE checkout_sessions ALTER COLUMN plan_tier DROP NOT NULL;`
- `PostgresCheckoutSessionRepository` round-trips `Option<PlanTier>`.
- `TenantRetentionSettingsRepository::delete(tenant_id)` added to the
  trait and to all implementors (`Arc<T>` blanket, `Noop`, `InMemory`,
  `Postgres`). See design notes for why.

### 4. API handlers

`crates/api/src/handlers/add_on_billing.rs`:

- `create_add_on` — no subscription required. Computes:
  - SMS: provider unit price × `sms_markup_multiplier` (currency match
    enforced).
  - Enterprise: `enterprise_sla_monthly_price`.
  - Retention: `compute_storage_delta(...) × markup_multiplier`,
    `quantity == 1` enforced, `tier` required.
  Creates the add-on in `Pending`, opens a `PaymentProvider` checkout,
  attaches the checkout id, persists a `CheckoutSession` with
  `plan_tier = None`.
- `verify_add_on` — on `Completed`, for `ExtendedRetention` it upserts
  `TenantRetentionSettings { source: Addon, days: tier.days() }` and
  writes a `retention_settings_changed` financial audit entry.
  `SmsCredits` / `EnterpriseSla` need no runtime state.
- `cancel_add_on` — for `ExtendedRetention`, calls
  `tenant_retention_settings_repository.delete(tenant_id)` so
  `effective_retention` falls back to `default_retention_days`.
- `billing_summary` — new shape:
  `{ wallet_balance_units, wallet_currency, active_add_ons,
  add_ons_total, currency }`.

### 5. Routes

`crates/api/src/routes/billing.rs`:

- `POST /billing/addons` → `create_add_on`
- `POST /billing/addons/verify` → `verify_add_on`
- `POST /billing/addons/cancel` → `cancel_add_on`
- `GET  /billing/summary` → `billing_summary`

### 6. Tests

`crates/api/tests/add_on_billing.rs` — 6 handler-level tests:

- `create_sms_add_on_without_subscription`
- `create_enterprise_sla_add_on`
- `create_extended_retention_add_on_90`
- `verify_extended_retention_updates_settings`
- `cancel_extended_retention_reverts_to_default`
- `billing_summary_shows_wallet_and_addons`

The tests call the handlers directly with `State(AppState)` +
`Extension(AuthenticatedUser)`, mirroring `payg_pricing_entitlement.rs`,
so JWT middleware is not exercised.

### 7. Dashboard

- `pages/add_ons.rs` — "Extended Retention" option in the type select;
  when selected, a 90/180/365 tier select replaces the quantity and
  country inputs; the successful create result renders a "Complete
  payment" link to `checkout_url` when present.
- `pages/billing.rs` — new "Add-ons" card fed by `billing::summary()`
  showing active add-ons and the total; a "Manage add-ons" link to
  `/billing/addons`.
- `api/billing.rs` — `AddOn` gains optional `checkout_session_id` /
  `checkout_url`; `BillingSummary` matches the new backend shape.
- `routes/mod.rs` — `AddOnsPage` registered at `/billing/addons`.
- `components/navigation.rs` — "Add-ons" nav item under the BILLING
  group.

## Files changed

**New:**

- `migrations/0119_checkout_sessions_plan_tier_nullable.sql`
- `crates/api/tests/add_on_billing.rs`
- `docs/internal/add-on-restoration-closure.md` (this file)

**Modified:**

- `crates/domain/src/entities/add_on_subscription.rs`
- `crates/domain/src/entities/checkout_session.rs`
- `crates/domain/src/entities/mod.rs`
- `crates/domain/src/pricing/calculation.rs`
- `crates/application/src/ports/payment_provider.rs`
- `crates/application/src/ports/tenant_retention_settings_repository.rs`
- `crates/application/src/use_cases/billing_support.rs`
- `crates/application/src/use_cases/change_subscription_plan.rs`
- `crates/application/src/use_cases/initiate_checkout.rs`
- `crates/application/src/use_cases/verify_checkout.rs`
- `crates/application/tests/change_subscription_plan.rs`
- `crates/application/tests/checkout_use_cases.rs`
- `crates/infra/src/repositories/in_memory_tenant_retention_settings_repository.rs`
- `crates/infra/src/repositories/postgres_checkout_session_repository.rs`
- `crates/infra/src/repositories/postgres_tenant_retention_settings_repository.rs`
- `crates/infra/src/services/fake_billing_provider.rs`
- `crates/infra/tests/paystack_billing_provider.rs`
- `crates/api/src/handlers/add_on_billing.rs`
- `crates/api/src/handlers/initiate_checkout.rs`
- `crates/api/src/routes/billing.rs`
- `crates/api/src/state.rs`
- `apps/leptos-dashboard/src/api/billing.rs`
- `apps/leptos-dashboard/src/components/navigation.rs`
- `apps/leptos-dashboard/src/pages/add_ons.rs`
- `apps/leptos-dashboard/src/pages/billing.rs`
- `apps/leptos-dashboard/src/routes/mod.rs`

## Tests

| Suite | Result |
|-------|--------|
| `api::add_on_billing` (new) | 6/6 |
| `application::checkout_use_cases` | 5/5 |
| `application::change_subscription_plan` | 5/5 |
| `application::add_on_subscriptions` | 1/1 |
| `infra::paystack_billing_provider` | 37/37 (3 ignored) |
| `infra::tenant_retention_settings` | 10/10 |
| `domain` (lib) | 250/250 |
| `leptos_dashboard` (lib) | 10/10 |
| Full workspace test | pass |
| Full workspace clippy | clean |
| `cargo build -p leptos-dashboard --target wasm32-unknown-unknown` | pass |

## Design notes

- **Cancellation deletes the retention row, it does not write a
  `Default` row.** `TenantRetentionSettingsRepository::upsert` is
  priority-aware by contract: a `Default`-priority write is silently
  discarded while an `Addon`-priority row exists. Cancelling an
  `ExtendedRetention` add-on therefore calls the new
  `delete(tenant_id)`; with the row gone, `effective_retention` already
  falls back to `pricing_config.default_retention_days` — the same
  outcome, without violating the priority rule. This was caught by
  `cancel_extended_retention_reverts_to_default` failing on the
  pre-fix code.
- **`CheckoutSession.plan_tier` is `Option<PlanTier>`.** Add-on
  checkouts have no plan tier; subscription checkouts continue to carry
  `Some(tier)`. Column made nullable in migration 0119. Existing
  subscription paths (`change_subscription_plan`, `verify_checkout`)
  were updated to require `Some(tier)` explicitly.
- **Retention pricing is per-tenant.** The price is
  `compute_storage_delta(enabled_monitors, free_tier.interval_seconds,
  tier.days(), config) × markup_multiplier`. It scales with the number
  of monitors a tenant actually has enabled; the interval is pinned to
  the pricing config's free-tier interval so the delta reflects the
  retention change only.
- **Add-ons are not wallet-funded.** They go through the
  `PaymentProvider` checkout flow, so the wallet balance is untouched
  when an add-on is purchased. `billing_summary` reports the wallet
  balance and active add-ons side by side.

## Not in this phase

- **Reconciliation worker verification for `rum_session`.** The user
  agreed to take this up as Phase C after add-on restoration closes.
  The reconciliation worker's handling of the new canonical
  `rum_session` operation and the current 14-operation canonical set is
  the next deliverable.
- **Percentile / top-error aggregation beyond 7 days.** Carried over
  from GAP 3 Kashi E; unrelated to add-ons.
- **Add-on cancellation refunds.** Cancelling an add-on reverts the
  retention setting but does not issue a wallet credit or provider
  refund. Out of scope.

## Pre-existing state

- Storage remains retired from the canonical operation set.
- Per-kind pricing (HTTP=1000, TCP=700, DNS=500 nano-USD), nano-USD
  units, flow-steps reserve/capture, per-tenant retention, SMS
  fair-use allowance, and RUM session-based pricing are unaffected.
- `CheckoutSession.plan_tier: Option<PlanTier>` is now the contract for
  every checkout path.
