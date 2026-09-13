# GAP 3 — Kashi D: SMS Fair-Use Allowance — CLOSED

**Status:** ✅ CLOSED
**Date:** 2026-09-13
**Scope:** Proportional monthly SMS allowance with wallet + email fallback.
**Follows:** GAP 3 Kashi A (nano-USD + per-kind pricing), Kashi B (flow steps pricing).

## Objective

Give every tenant a **proportional, monthly-expiring** free SMS allowance
based on their active monitor count, and route SMS beyond that allowance
through paid wallet credits and, ultimately, the free email channel.

Voice minutes and phone numbers are modeled in the schema for forward
compatibility; both are always 0 today (no Voice integration exists yet).

## What was delivered

### 1. Allowance schema (`migrations/0116`)

A new `tenant_alert_allowances` table with one row per tenant per cycle:

| Column | Meaning |
|--------|---------|
| `cycle_started_at` / `cycle_ends_at` | UTC [1st-of-month, 1st-of-next) |
| `sms_limit` / `sms_used` | Fair-use SMS allowance |
| `voice_limit` / `voice_used` | Reserved (always 0 today) |
| `phone_numbers_limit` / `phone_numbers_used` | Reserved (always 0 today) |

### 2. Proportional formula

`TenantAlertAllowance::compute_sms_limit(enabled_monitors)`:
`ceil(enabled_monitors × 250 / 1000)` — i.e. the blueprint baseline of
1,000 enabled monitors = 250 SMS per month. 1 monitor → 1 SMS; 10 → 3;
1,000 → 250; 1,001 → 251. Only **enabled** monitors count; a tenant
cannot inflate their allowance by creating disabled monitors.

### 3. Lazy cycle reset (primary path)

`PostgresTenantAlertAllowanceRepository::current_cycle` creates the row if
missing, and rolls the cycle forward if `cycle_started_at < current month
start`. `sms_limit` is recomputed at that moment and then frozen for the
rest of the cycle. This makes the allowance correct even if the reset
worker is down — the first SMS of a new month is enough.

`try_consume_sms` is a single atomic `UPDATE ... WHERE sms_used <
sms_limit` that returns `true` iff the allowance was consumed.

### 4. Monthly reset worker (backstop)

`worker::AllowanceResetWorker` walks every tenant once an hour and calls
`current_cycle` with that tenant's enabled-monitor count. Without it, a
tenant who sends no SMS would keep a stale `cycle_ends_at` and the
dashboard would render an out-of-date view; with it, every row is at
most one tick old.

### 5. SMS dispatch — three-tier fallback

`NotificationDispatcher::notify_with_request_id` for SMS now:

1. **Consume the free allowance** — if `try_consume_sms` returns `true`,
   the SMS is delivered without touching the wallet.
2. **Fall back to the wallet** — if the allowance is exhausted and the
   wallet can pay the `sms` operation's `unit_price`, the SMS is
   delivered and the wallet is debited exactly once.
3. **Fall back to email** — if the allowance is exhausted and the wallet
   cannot pay (`InsufficientBalance` or `SpendLimitExceeded`), the
   notification is delivered to every enabled email channel belonging to
   the tenant. Email is free and unlimited; no metering.

### 6. Test sends bypass the allowance

`NotificationEvent::Test` requests are prefixed with `test:` inside
`NotificationDispatcher::notify`, and the allowance branch checks that
prefix. A "Send test notification" button therefore never drains a
tenant's free SMS quota — the test send goes through the metered path
only.

### 7. MonitorRepository::count_enabled_for_tenant

A new method on the trait with a **default** of "fall back to total
count" and a Postgres override of `... AND enabled = true`. The InMemory
implementation also honors the enabled filter, so tests are consistent
across backends.

### 8. Billing API surface

`GET /api/v1/billing/allowance` returns the current cycle's `sms_limit`,
`sms_used`, `sms_remaining`, the frozen `enabled_monitors`, and the
cycle window. The dashboard's billing page renders a "Fair-use alert
allowance" card with `sms_used / sms_limit` and the fallback explanation.

## Files changed

**New:**

- `migrations/0116_tenant_alert_allowances.sql`
- `crates/domain/src/entities/tenant_alert_allowance.rs`
- `crates/application/src/ports/tenant_alert_allowance_repository.rs`
- `crates/infra/src/repositories/postgres_tenant_alert_allowance_repository.rs`
- `crates/infra/src/repositories/in_memory_tenant_alert_allowance_repository.rs`
- `crates/worker/src/allowance_reset_worker.rs`
- `crates/api/src/handlers/billing_allowance.rs`
- `crates/domain/tests/tenant_alert_allowance.rs` (9 tests)
- `crates/infra/tests/tenant_alert_allowance.rs` (7 tests)
- `crates/worker/tests/sms_allowance_fallback.rs` (4 tests)
- `docs/GAP_3_KASHI_D_CLOSURE.md` (this file)

**Modified:**

- `crates/domain/src/entities/mod.rs`
- `crates/application/src/ports/mod.rs`
- `crates/application/src/ports/monitor_repository.rs`
- `crates/infra/src/repositories/postgres_monitor_repository.rs`
- `crates/infra/src/repositories/in_memory_monitor_repository.rs`
- `crates/infra/src/repositories/mod.rs`
- `crates/infra/src/services/notification_dispatcher.rs`
- `crates/worker/src/lib.rs`
- `crates/worker/src/main.rs`
- `crates/api/src/state.rs`
- `crates/api/src/main.rs`
- `crates/api/src/handlers/mod.rs`
- `crates/api/src/routes/billing.rs`
- `apps/leptos-dashboard/src/api/billing.rs`
- `apps/leptos-dashboard/src/pages/billing.rs`

## Tests

| Suite | Result |
|-------|--------|
| `domain::tenant_alert_allowance` | 9/9 |
| `infra::tenant_alert_allowance` | 7/7 |
| `worker::sms_allowance_fallback` | 4/4 |
| Full workspace build (`--all-targets`) | pass |
| Dashboard wasm build | pass |

## Design notes

- **Enabled-only count** — disabled monitors do not contribute to the
  allowance, so a tenant cannot game the formula by creating disabled
  monitors. This is enforced at the SQL level, not just in application
  code.
- **Frozen `sms_limit`** — once a cycle starts, the limit is not
  recomputed until the next cycle, so adding monitors mid-month does not
  retroactively expand the allowance. This is deterministic and matches
  the "monthly snapshot" model the blueprint specified.
- **Test-send bypass** — enforced in `notify()` by rewriting the
  request_id prefix, not by trusting the caller. A future caller cannot
  accidentally (or intentionally) drain the allowance by sending a test.

## Not in this phase

- **Voice + phone numbers** — the schema is present; the Twilio Voice
  integration, `NotificationChannelKind::Voice`, and phone-number
  provisioning are deferred.
- **Extended retention add-on** — Kashi C.
- **RUM session-based pricing** — Kashi E.
- **`add_on_billing.rs` restoration** — the endpoints still return
  "Gone". Paid SMS credits are a separate follow-up; the wallet path
  already works via `MeterUsageUseCase` on the `sms` operation.
