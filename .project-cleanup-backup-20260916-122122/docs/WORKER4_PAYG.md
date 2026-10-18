# Worker 4 — PAYG domain, metering and pricing

This implementation was built from the source tree first, not from the existing
documentation. The repository already contained a partial wallet/reservation
foundation (`migrations/0074_wallet_payg_core.sql` and wallet repositories), so
Worker 4 completes and hardens that foundation instead of introducing a second
wallet system.

## Money model

All wallet/payment/PAYG monetary values are signed/unsigned integer units in
Postgres `BIGINT` and Rust `i64`. The initial PAYG price book defines one wallet
unit as one micro-USD (`10^-6 USD`). No financial calculation accepts `FLOAT`.

## Metering flow

`MeterUsageUseCase` delegates to `ChargeForUsageUseCase`:

1. authenticated application boundary supplies the tenant id;
2. validate operation, positive units and request id;
3. check usage-event idempotency;
4. load the server-side active PAYG price;
5. calculate `units * unit_price` with checked integer multiplication;
6. atomically reserve credits;
7. run the billable operation;
8. capture on success or release on failure;
9. persist an immutable usage event containing the pricing version.

The reservation locks the wallet row in PostgreSQL, so concurrent requests for
different request ids cannot overspend the same balance.

## Idempotency

Financial ledger rows require a tenant-scoped idempotency key. The database
enforces `(tenant_id, idempotency_key)`. Usage events enforce
`(tenant_id, request_id)`. Duplicate completed requests return the previously
persisted usage result.

## Immutability

Wallet ledger rows and usage events have database triggers that reject UPDATE
and DELETE. Application repositories also expose no mutation operation for
ledger rows.

## Security

- Client metering DTO contains only `operation`, `units`, and `request_id`.
- `serde(deny_unknown_fields)` rejects `price`, `total`, and `unit_price`.
- The server resolves prices and calculates charges.
- Insufficient balance is represented by `PAYG_INSUFFICIENT_BALANCE`; the API
  error layer provides HTTP 402 mapping.
- Wallet balance is constrained to `>= 0`.
- Wallet routes are authenticated and rate-limited.

