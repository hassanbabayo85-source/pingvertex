# PAYG GAP 1 — Network Egress Billing — CLOSED

**Status:** ✅ CLOSED — 2026-09-12
**Commit:** 58526ccb — feat(payg): close network egress billing gap + retire proxy monitoring

## Objective (historical)

Duk lokacin da HTTP/Flow monitor ya yi outbound request kuma source
ya auna egress bytes, worker ya kamata ya charge PAYG operation:

    network_egress

bisa adadin MB da aka auna.

## Live call-sites

`crates/worker/src/durable_scheduler.rs`:

| Line | Path | Function |
|------|------|----------|
| 431  | one-time     | `bill_network_egress(...)` |
| 662  | monitoring   | `bill_network_egress(...)` |
| 785  | helper       | `pub async fn bill_network_egress` |

Re-export: `crates/worker/src/lib.rs:51`

## Idempotency

| Operation | request_id format |
|-----------|-------------------|
| monitoring | `monitor-egress:<monitor_id>:<execution_id>` |
| one-time   | `one-time-egress:<request_id>:<execution_id>` |

Both route through `MeterUsageUseCase::execute_recoverable`,
which resumes a PENDING reservation. Scheduler retries cannot
double-charge.

## Zero-byte rule

`network_egress_units(0) == 0` → no charge.

## Tests

### `crates/worker/tests/network_egress_billing.rs` (HTTP path)

- `zero_bytes_is_a_no_op`
- `one_byte_charges_one_unit`
- `one_mib_charges_one_unit`
- `one_mib_plus_one_byte_charges_two_units`
- `retry_with_same_request_id_does_not_double_charge`

### `crates/worker/tests/flow_network_egress_billing.rs` (API flow path)

- `flow_executor_reports_per_step_egress_bytes`:
  drives `execute_flow_with_resolver` against a local mock HTTP
  server and asserts each step's `egress_bytes` matches the actual
  response body length, then confirms the summed total converts to
  the expected 1 MiB unit count via `network_egress_units`.
- `flow_egress_charge_matches_measured_bytes`:
  feeds the summed egress to `bill_network_egress` with
  `MonitorKind::Flow` and asserts the wallet is debited the exact
  1 MiB × 100 micro-USD expected amount.
- `zero_byte_flow_charges_nothing`:
  empty response body → 0 egress → `bill_network_egress` is a no-op.
- `insufficient_balance_is_a_logged_no_op`:
  wallet with 50 micro-USD, egress of exactly 1 MiB (= 100 micro-USD
  charge). `bill_network_egress` must not panic and must leave the
  wallet at its original balance.

## Regression suites (all green)

| Suite | Result |
|-------|--------|
| `worker::http_check` | 4/4 |
| `worker::worker_graceful_shutdown` | 6/6 |
| `worker::worker_tenant_suspension` | 2/2 |
| `worker::--lib` | 75/75 |
| `application::payg_scheduler_context` | 1/1 |
| `infra::worker9_wallet_payg` | 7/7 |
| `cargo build --workspace` | pass |
| `cargo fmt --all --check` | clean |
