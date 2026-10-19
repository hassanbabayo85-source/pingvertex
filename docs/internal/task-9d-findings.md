# Task 9D.1 — Critical Correctness Defect: `checkout_sessions.amount_minor`

**Date:** 2026-10-19
**Discovered by:** Task 9D.1 concurrency test against real PostgreSQL
**Introduced by:** Task 6 (Paystack subscription checkout)
**Severity:** Critical correctness defect (production-impacting)
**Category:** Correctness, not security
**Status:** Reported as fixed; verify using actual test results.

---

## Symptom

The repository INSERT references `checkout_sessions.amount_minor`, but migration 0129 does not add that column. Consequently, checkout requests can fail with PostgreSQL error 42703.

## Root Cause

`PostgresCheckoutSessionRepository::create` inserts 15 columns, including `amount_minor`.

Migration 0129 was intended to add this column to `checkout_sessions`, but a fragile string guard matched the unrelated `amount_minor BIGINT` declaration in `subscription_billing_records`. As a result, the required ALTER TABLE statement was skipped.

## Why It Was Previously Missed

Earlier tests used `InMemoryCheckoutSessionRepository`, which cannot validate the actual PostgreSQL schema. `cargo check` also cannot verify that raw SQL strings match database columns.

## Fix

Create `migrations/0131_checkout_sessions_amount_minor.sql`:

```sql
ALTER TABLE checkout_sessions
    ADD COLUMN IF NOT EXISTS amount_minor BIGINT;

ALTER TABLE checkout_sessions
    DROP CONSTRAINT IF EXISTS chk_checkout_sessions_amount_minor;

ALTER TABLE checkout_sessions
    ADD CONSTRAINT chk_checkout_sessions_amount_minor
    CHECK (amount_minor IS NULL OR amount_minor >= 0);
```

Do not modify migration 0129 if it has already been applied. Verify migration ordering and execution against both fresh and previously migrated test databases.

## Required Verification

Run:

```bash
cargo test -p api --test subscription_billing_concurrency -- --test-threads=1
cargo test -p api --tests --no-fail-fast
```

Record the actual results. Do not claim tests passed unless the commands completed successfully.

## Concurrency Invariants

1. Ten concurrent signed Paystack webhook deliveries for the same reference must activate exactly one subscription and create exactly one successful billing record.
2. Ten concurrent checkout verification attempts for the same session must preserve the same invariants.
3. Concurrent verification across three tenants must not cross-contaminate subscriptions or billing records.

Verify the relevant database UNIQUE constraints and transaction behavior rather than assuming they guarantee every invariant automatically.

## Process Lessons

1. In-memory repositories do not validate production SQL or database schemas.
2. Avoid fragile substring guards when generating migrations.
3. Run schema-sensitive integration tests against real PostgreSQL.
4. Keep applied migrations immutable and introduce corrective migrations when necessary.

## Follow-up

Include the confirmed finding, root cause, corrective migration, test evidence, and any unresolved risks in the Task 9E final report.
