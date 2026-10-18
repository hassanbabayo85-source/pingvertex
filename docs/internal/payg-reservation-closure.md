# GAP 3 — Kashi B: Flow Steps-Based Pricing — CLOSED

**Status:** ✅ CLOSED
**Date:** 2026-09-13
**Scope:** Flow-check pricing billed per executed step, with a partial-capture wallet lifecycle.
**Follows:** GAP 3 Kashi A (nano-USD unit + per-kind pricing + storage retired).

## Objective

A Flow monitor is an ordered list of HTTP steps. Before this change, every
flow check was billed as exactly one unit regardless of how many steps it
ran. GAP 3 Kashi B makes a flow check reserve `MAX_STEPS × unit_price` up
front, then bill only the number of steps whose HTTP request actually
started. The remainder of the reservation is released in the same capture
transaction.

## What was delivered

### 1. Reserve-then-partial-capture lifecycle

`ChargeForUsageUseCase::execute_classified_attempt_with_output` now reads
`MeteredAttempt::actual_units`. When `Some(n)`, it:

1. Computes `actual_charge = n × unit_price`.
2. Validates `0 < actual_charge <= reservation.amount_units`.
3. Builds the `UsageEvent` with `units_billed = n` and
   `charge_units = actual_charge`.
4. Calls `capture(..., actual_charge_units: Some(actual_charge))` so the
   wallet is debited by exactly `n` and the reservation remainder is
   released inside the same transaction.

When `actual_units` is `None`, the behavior is unchanged: the full
reserved amount is captured.

### 2. `MeteredAttempt::actual_units`

`crates/application/src/use_cases/charge_for_usage.rs` adds
`actual_units: Option<i64>`. Existing constructors pass `None`. Flow
executors populate `Some(steps_executed)`.

### 3. `WalletRepository::capture` now takes `actual_charge_units`

The trait, the `Arc<T>` blanket impl, `PostgresWalletRepository`,
`InMemoryWalletRepository`, and `NoopWalletRepository` all take the new
argument. Postgres:

- reads the reservation with `FOR UPDATE`;
- validates `0 < actual_charge_units <= reservation.amount_units` inside
  the same transaction;
- debits `capture_amount` instead of `reservation.amount_units`;
- marks the reservation `CAPTURED` and returns the wallet's balance to
  reflect only the actual charge (the remainder is implicitly released).

Replays of a `CAPTURED` reservation return the original `WalletTransaction`
without touching the balance again.

`validate_usage_for_reservation` was relaxed from
`event.charge_units == reservation.amount_units` to
`event.charge_units <= reservation.amount_units`.

### 4. `CheckExecution::steps_executed`

`crates/worker/src/durable_scheduler.rs` adds
`steps_executed: Option<i64>` to `CheckExecution`. FlowCheckExecutor
populates `Some(outcomes.iter().filter(|o| o.attempted).count() as i64)` —
the number of steps whose HTTP request actually started, including the
one that failed (its bandwidth and target-side work were still consumed).
HttpCheckExecutor, TcpCheckExecutor, and DnsCheckExecutor pass `None`.

### 5. Scheduler reserves MAX_STEPS for flows

Both the one-time and the monitoring paths in `durable_scheduler.rs`
compute `units = if matches!(monitor.kind, MonitorKind::Flow) { MAX_STEPS as i64 } else { 1 }`
and forward the executor's `steps_executed` into
`MeteredAttempt::actual_units`.

`MAX_STEPS` is 10 (`crates/domain/src/entities/api_flow.rs`), so a flow
check reserves `10 × unit_price` before execution. For
`api_flow_check.monitoring` at 1000 nano-USD per step, that is a 10_000
nano-USD reservation.

### 6. Pricing

`api_flow_check.one_time` and `api_flow_check.monitoring` were already
set to 1000 nano-USD (per request/check) by migration 0114. No new
pricing row is needed — the existing row is now applied per step, not
per flow.

`unit_name = 'request/check'` is preserved; each executed step is one
HTTP request.

## Files changed

**New:**

- `crates/infra/tests/postgres_partial_capture.rs` (5 tests)
- `crates/worker/tests/flow_step_pricing.rs` (4 tests)
- `docs/internal/payg-reservation-closure.md` (this file)

**Modified:**

- `crates/application/src/use_cases/charge_for_usage.rs`
- `crates/application/src/ports/wallet_repository.rs`
- `crates/infra/src/repositories/postgres_wallet_repository.rs`
- `crates/infra/src/repositories/in_memory_wallet_repository.rs`
- `crates/api/src/state.rs` (Noop wallet)
- `crates/worker/src/durable_scheduler.rs`
- `crates/worker/src/flow_check_executor.rs`
- `crates/worker/src/http_check_executor.rs`
- `crates/worker/src/tcp_check_executor.rs`
- `crates/worker/src/dns_check_executor.rs`
- `crates/worker/src/proxy_checker.rs`
- `crates/worker/tests/one_time_multiregion_billing.rs` (7 constructions)
- `crates/application/tests/payg_scheduler_context.rs` (1 construction)
- All existing `.capture(...)` call sites across tests

## Tests

| Suite | Result |
|-------|--------|
| `worker::flow_step_pricing` (new) | 4/4 |
| `infra::postgres_partial_capture` (new) | 5/5 |
| `worker::one_time_multiregion_billing` | 5/5 |
| `infra::wallet_payg` | 7/7 |
| Full workspace build (`--all-targets`) | pass |
| `cargo clippy` on the new tests | clean |

Note: `infra::scheduler_claim::concurrent_claim_attempts_do_not_double_claim`
is intermittently flaky under full-suite load. It passes 3/3 in isolation
and no scheduler-claim logic was modified by this phase.

## Not in this phase

- SMS/Voice fair-use allowance — Kashi D
- Extended retention add-on — Kashi C
- RUM session-based pricing — Kashi E

## Pre-existing state

- `network_egress` continues to be metered and billed as before.
- One-time proxy checks (`proxy_check.one_time`) still reserve 1 unit and
  capture 1 unit; flow-only partial capture does not change them.
- `MeteredAttempt::actual_units = None` for Http/Tcp/Dns/Proxy/Heartbeat.
