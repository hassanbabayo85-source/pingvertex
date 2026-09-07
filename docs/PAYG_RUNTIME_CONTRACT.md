# PAYG Runtime Contract

## Billing boundary

`monitor_check` is authorized before the real HTTP/TCP/DNS/Flow attempt. The scheduler supplies a durable execution identity derived from the database lease execution id. A debit is captured only when the executor reports that the billable external attempt actually started.

## Billable attempts

- HTTP: a request after SSRF validation starts the billable attempt. HTTP 4xx/5xx and network errors after request start are billable. SSRF rejection, disabled-monitor validation, malformed configuration, and client-construction failures before request start are not billable.
- TCP: DNS/target resolution must succeed first; `TcpStream::connect` starting is the billable attempt. Refusal and timeout are billable. Invalid/missing port and pre-connect validation failures are not billable.
- DNS: issuing the resolver lookup is the billable attempt. No-record and resolver failures are billable because the lookup was attempted; malformed record configuration is not.
- Flow: a flow is billable once at least one HTTP step request starts. Placeholder resolution, missing flow configuration, SSRF rejection, and other failures before any request starts are not billable.
- Proxy: each proxy entry has its own durable request id (`proxy-check:<job>:<entry>`) and is metered independently. A successful, refused, timed-out, protocol-failed, or authentication-failed proxy attempt is billable once the proxy connection attempt starts. Private-IP/invalid-proxy preflight failures are not billable.

## Insufficient balance

Reservation occurs before the external attempt. If the wallet cannot reserve the server-resolved price, no network operation is started and the wallet enters `PAYG_BALANCE_EXHAUSTED`. The dashboard exposes the blocked state.

## Crash/retry

Reservations are short-lived and expired by `WalletExpirationWorker`. A scheduler lease keeps the same execution id through lease recovery. Wallet reservations and ledger debits are idempotent by tenant/request id and reservation id. Captures write the debit, usage event, and financial audit records in the same database transaction.

## Ledger/source of truth

`wallet_transactions` is append-only. `api_usage_events` is immutable and uniquely keyed by `(tenant_id, request_id)`. PAYG prices are immutable and server-resolved. The Free tier's monitor count, minimum interval, and retention values are resolved from the active database `pricing_configs.free_*` fields; the static `Plan` catalog supplies paid-tier capabilities.

## Reconciliation

The reconciliation worker expires stale reservations, repairs legacy captured-debit usage gaps where historical pricing is available, repairs missing financial audit rows, and reports duplicate/mismatch invariants. PAYG reconciliation is independent of payment-provider webhook reconciliation.
