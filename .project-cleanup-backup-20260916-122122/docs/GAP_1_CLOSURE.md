# GAP 1 — Complete One-Time Check System — CLOSED

**Status:** ✅ CLOSED
**Date:** 2026-09-12
**Scope:** Backend, dashboard, billing, tests.

## Objective

Complete the One-Time Check system exactly as the PAYG blueprint
specifies: customer-facing Test Now flow, true multi-region execution,
per-region billing, complete reservation → execute → capture/release
lifecycle, separation between one-time and recurring monitoring, and
proper usage reporting.

## What was delivered

### 1. Canonical operations

`crates/domain/src/payg.rs` already carried the canonical set. Confirmed
both one-time and monitoring variants exist and are distinct:

| One-time | Monitoring |
|----------|------------|
| `http_check.one_time` | `http_check.monitoring` |
| `tcp_check.one_time` | `tcp_check.monitoring` |
| `dns_check.one_time` | `dns_check.monitoring` |
| `api_flow_check.one_time` | `api_flow_check.monitoring` |
| `proxy_check.one_time` | *(recurring proxy monitoring retired)* |

Generic `monitor_check` is retired (`RETIRED_PAYG_OPERATIONS`,
migrations 0110/0111).

### 2. Per-region execution model

New migration `0112_payg_one_time_check_executions.sql` introduces the
`one_time_check_executions` table:

- one row per `(request_id, region)`;
- each row has its own `execution_id` (durable wallet idempotency key);
- status lifecycle `QUEUED → RUNNING → SUCCEEDED|FAILED`;
- records `attempted`, `billable`, `charge_units` for audit.

Parent `one_time_check_requests` status is rolled up from children:
`RUNNING` while any child is pending; `SUCCEEDED` if any child
succeeded; `FAILED` otherwise.

### 3. One-time execution semantics

- Executes exactly once per region.
- Does **not** create a recurring monitor.
- Does **not** create a scheduler entry.
- Does **not** touch `monitors.next_due_at`.
- Does **not** modify `monitors.lease_owner`, `lease_expires_at`,
  `lease_claimed_at`, or `lease_execution_id`.
- Does **not** modify monitor enabled state.

Verified by test `one_time_enqueue_does_not_touch_recurring_scheduler_state`.

### 4. Billing lifecycle

Each regional execution runs the same
`reserve → execute → capture/release` lifecycle used by monitoring:

- `ChargeForUsageUseCase::execute_classified_attempt`;
- reservation is created before the external attempt;
- capture writes debit + usage event + financial audits in one DB txn;
- a non-billable (pre-attempt) failure releases the reservation and
  writes no debit;
- pricing is resolved server-side through `resolve_price`, using the
  execution's own region, so a region-specific rule is honored.

### 5. Multi-region billing

A Test Now request on a monitor with 3 regions produces 3 independent
billable regional executions. Each has a unique `execution_id`, so:

- retrying one region never double-charges (replay returns the existing
  usage event and no debit);
- insufficient balance on one region releases that region's reservation
  and does not prevent the other regions from being charged;
- the usage ledger records each region separately.

Verified by `crates/worker/tests/one_time_multiregion_billing.rs`
(5 tests) and `crates/infra/tests/postgres_one_time_multiregion.rs`
(6 tests).

### 6. Test Now UI

`apps/leptos-dashboard/src/pages/monitor_detail.rs`:

- The existing **Test now** button now calls the one-time endpoint and
  polls `GET /monitors/{id}/checks/one-time/{request_id}` until the
  request reaches a terminal state.
- A new result panel renders one row per region with status, billable /
  released, charge, and error.
- The panel header shows the billed count and the released count, e.g.
  `3 region(s) · 2 billed · 1 released`.

`apps/leptos-dashboard/src/api/monitoring.rs` now exposes the new
response shape (`OneTimeCheckResponse` with a `regions: Vec<...>`
field) so the dashboard deserializes correctly.

### 7. Usage page

`apps/leptos-dashboard/src/pages/usage.rs` lists only canonical
operation names — the stale `monitor_check` and `proxy_check` rows are
gone, replaced by the `*_check.one_time` and `*_check.monitoring`
families plus `sms`, `rum_event`, `storage`, `network_egress`, and
`heartbeat_processing`.

The filter placeholder now uses `http_check.one_time`.

### 8. Tests

| Suite | Result |
|-------|--------|
| `worker::one_time_multiregion_billing` | 5/5 |
| `infra::postgres_one_time_multiregion` | 6/6 |
| `worker::--lib` | 75/75 |
| `application::--lib` | pass |
| Full workspace `cargo test --workspace` | pass |
| Full workspace `cargo build --workspace` | pass |

Coverage added:

- one-time HTTP / TCP / DNS / API-flow / Proxy — through the canonical
  operation name;
- multi-region one-time fan-out;
- reservation creation per regional execution;
- capture on billable attempt;
- release on non-billable attempt;
- insufficient balance on one region does not block others;
- duplicate request / retry idempotency;
- no scheduler mutation;
- no recurring schedule creation;
- tenant isolation;
- server-side price enforcement (region-scoped pricing context).

## Definition of Done

| Item | Status |
|------|--------|
| One-Time operations canonical | ✅ |
| Test Now UI working | ✅ |
| One-time does not create scheduler entry | ✅ |
| Multi-region executes independently | ✅ |
| Each successful regional execution billed correctly | ✅ |
| Failed/non-billable execution releases reservation | ✅ |
| Idempotency prevents duplicate charges | ✅ |
| Usage records distinguish one-time from monitoring | ✅ |
| All tests pass | ✅ |

## Files changed

**New:**

- `migrations/0112_payg_one_time_check_executions.sql`
- `crates/infra/tests/postgres_one_time_multiregion.rs`
- `crates/worker/tests/one_time_multiregion_billing.rs`
- `docs/GAP_1_CLOSURE.md` (this file)

**Modified:**

- `crates/domain/src/entities/one_time_check.rs`
- `crates/domain/src/entities/mod.rs`
- `crates/application/src/ports/scheduling_repository.rs`
- `crates/application/src/ports/mod.rs`
- `crates/infra/src/repositories/postgres_scheduling_repository.rs`
- `crates/api/src/handlers/one_time_check.rs`
- `crates/api/src/state.rs`
- `crates/worker/src/durable_scheduler.rs`
- `apps/leptos-dashboard/src/api/monitoring.rs`
- `apps/leptos-dashboard/src/pages/monitor_detail.rs`
- `apps/leptos-dashboard/src/pages/usage.rs`
