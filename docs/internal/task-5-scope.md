# TASK 5 — Scope decisions

## In scope (completed)

- `Monitor.entitlement_suspended` column + entity field + repository
  (`migrations/0128_*`)
- `ReconcileMonitorEntitlementsUseCase` — deterministic
  "oldest N keep, newest suspend" policy; idempotent
- `DurableScheduler` — wallet/PAYG gate replaced with `EntitlementResolver`.
  `payg_activated_at` removed from the monitor execution path.
  Monitor-path `MeterUsageUseCase` calls removed. Fail-safe when the
  resolver is absent (skip, do not run unmetered).
- `ProxyCheckWorker` — `MeterUsageUseCase` made optional; default `None`.
  `.with_metering(...)` remains for the transition window.
- `BulkMonitorImportWorker` — `payg_activated_at` removed;
  `CreateMonitorUseCase` now receives `.with_entitlement_resolver(...)`.
  Previously it was called with no resolver and would have failed with
  `EntitlementResolverMissing` at runtime.

## Intentionally out of scope for Task 5

### `retention_worker.rs`

Uses `wallet.payg_activated_at` to choose between retention policies
(legacy vs. addon vs. default). This is **data lifecycle**, not
execution authorization. Task 5 says:

> *"Remove wallet-balance checks and wallet reservations from ordinary
> **monitor execution authorization**."*

Retention policy selection is not execution authorization.
Additionally, `Entitlements` does not currently expose
`history_retention_days`; adding it would require migration 0129 plus
seeding all five tiers.

**Decision:** keep the current PAYG-aware retention selection for
Task 5. The default value (30 days) and the historical legacy/addon
overrides are correct today and cannot break existing data.

**Follow-up:** TASK 8 — add `history_retention_days` to
`Entitlements` and rewire `retention_worker.rs` to consume it.

### `auto_top_up_worker.rs`, `wallet_expiration_worker.rs`,
### `wallet_notification_email_worker.rs`, `reconciliation_worker.rs`

All four are pure PAYG workers with no equivalent in the subscription
model. Their only job is to keep the wallet ledger consistent. Task 5
does not remove them — the goal was to stop using wallet as an
*authorization* input, not to delete PAYG infrastructure while legacy
customers still have balances.

**Decision:** keep them running unchanged for now. TASK 8 will:

1. Delete the automatic top-up flow.
2. Keep the expiration sweep for historical promotional credits (audit
   trail).
3. Retire the wallet notification mailer once no wallet customer exists.
4. Keep `reconciliation_worker` for historical reconciliation only.

## What Task 5 does NOT change

- The wallet ledger tables (`user_wallets`, `wallet_transactions`,
  `credit_reservations`, `wallet_auto_top_up_*`). Untouched.
- PAYG pricing tables (`pricing_configs`, `payg_pricing_configs`).
  Untouched.
- `MeterUsageUseCase` and `ChargeForUsageUseCase`. Still exist; only
  removed from the monitor execution path.
- Historical financial records. Untouched.
- One-time proxy job billing. Still metered; Task 8 will decide.

## Verification

- `cargo check --workspace`: clean (one pre-existing warning in
  `external_status_pages_pagination.rs`, unrelated to Task 5).
- `domain --lib`: 358 passed.
- `application --lib`: 118 passed (110 previous + 8 new for
  `reconcile_monitor_entitlements`).
- All other suites unchanged.
