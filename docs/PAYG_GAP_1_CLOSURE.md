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

`crates/worker/tests/network_egress_billing.rs`:

- `zero_bytes_is_a_no_op`
- `one_byte_charges_one_unit`
- `one_mib_charges_one_unit`
- `one_mib_plus_one_byte_charges_two_units`
- `retry_with_same_request_id_does_not_double_charge`

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
