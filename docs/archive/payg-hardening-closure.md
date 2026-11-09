# GAP 3 — Kashi E: RUM Session-Based Pricing + Daily Aggregation — CLOSED

**Status:** ✅ CLOSED
**Date:** 2026-09-13
**Scope:** Per-session RUM billing, 7-day raw retention with daily aggregation.
**Follows:** GAP 3 Kashi A (nano-USD), Kashi B (flow steps), Kashi C (retention), Kashi D (SMS allowance).

## Objective

Replace per-beacon RUM billing (`rum_event`) with per-session billing
(`rum_session`). A session is 30 minutes of continuous browser activity;
every page view and error inside that window is grouped under one session
row and charged exactly once.

Raw RUM events are kept for 7 days. A worker rolls older days into
`rum_daily_aggregates` (permanent) and then deletes the raw rows. The
dashboard reads raw data for the recent window and aggregates for older
days.

## What was delivered

### 1. Schema (migrations/0118)

- New pricing row `rum_session` (1000 nano-USD per session, same rate as
  the retired `rum_event`).
- `rum_event` added to `payg_retired_operations` so `list_active()` stops
  advertising it. Historical pricing rows remain queryable (append-only).
- New table `rum_daily_aggregates`:
  `(site_id, day)` PK, with `total_sessions`, `page_view_count`,
  `js_error_count`, `resource_error_count`, `avg_load_time_ms`.
  Percentiles and top-error/browser/device/geo breakdowns are NOT
  aggregated — see design notes.

### 2. Domain

- `PaygOperation::RumEvent` renamed to `RumSession`
  (`as_str() == "rum_session"`).
- `RETIRED_PAYG_OPERATIONS` now includes `"rum_event"`.
- `RumSession::SESSION_WINDOW_MINUTES = 30`.
- New `RumDailyAggregate` entity.

### 3. Repository changes

- `RumEventRepository::upsert_session(session, now) -> UpsertedRumSession`
  where `created` is `true` when the beacon started a new 30-minute
  session and `false` when it merely extended an active one.
- New `RumRetentionRepository::run_retention_for_site(tenant, site,
  raw_retention_days, batch_size)` that aggregates then deletes.

Postgres implementations:

- `PostgresRumEventRepository::upsert_session` is a **single-statement**
  `INSERT ... ON CONFLICT (site_id, session_key) DO UPDATE` with
  `CASE WHEN rum_sessions.last_seen_at < cutoff` deciding between
  "extend" and "restart". `RETURNING (started_at = $10) AS is_new`
  reports `created`.
- `PostgresRumRetentionRepository::run_retention_for_site`:
  1. One `INSERT ... ON CONFLICT DO UPDATE` that merges three event
     streams (page views, JS errors, resource errors) and the session
     stream, grouped by UTC day.
  2. Batched `DELETE` for each of the four raw tables.

### 4. Billing flow

`collect_rum_event` now:

1. Generates a `RumSessionId` up front so `request_id = "rum_session:<id>"`
   is stable across the reserve/finalize pair.
2. Reserves one `rum_session` unit via `reserve_for_execution`.
3. Runs ingestion; on any pre-persist failure the reservation is released.
4. Calls `finalize_reserved_attempt(attempted = outcome.session_created)`:
   - new session -> **captured** (one charge)
   - extended session -> **released** (no charge)

Idempotency is by `request_id`, so a retried beacon for the same session
is at most one charge.

### 5. Worker

`RumAggregationWorker` runs every 6 hours:
- `TenantRepository::list_all()` -> `RumSiteRepository::list_by_tenant`
  -> `RumRetentionRepository::run_retention_for_site(tenant, site, 7, 1000)`.

Wired in `worker/main.rs`.

### 6. Dashboard (hybrid read)

`overview()` and `session_stats()` in
`PostgresRumAnalyticsRepository` are now hybrid:

- **Fast path** (range entirely inside the raw window): raw SQL only.
- **Hybrid path** (range crosses the 7-day boundary): raw for
  `[max(range.start, cutoff midnight), range.end)`, aggregates for
  `[range.start.date_naive(), cutoff)`. Counts are summed, `avg_load_time_ms`
  is a page-view-weighted average, and `error_rate` is recomputed.

Percentiles (`p95_load_time_ms`, `p75_lcp_ms`, `p75_inp_ms`, `p75_cls`) and
per-session duration come from the raw side only. Beyond 7 days they are
`None`; the dashboard renders "—".

`slow_pages`, `error_breakdown`, `browser_device_breakdown`, and
`geo_breakdown` continue to use raw data only, so they report nothing
beyond 7 days.

## Files changed

**New:**

- `migrations/0118_rum_session_operation_and_aggregates.sql`
- `crates/domain/src/entities/rum_daily_aggregate.rs`
- `crates/application/src/ports/rum_retention_repository.rs`
- `crates/infra/src/repositories/postgres_rum_retention_repository.rs`
- `crates/worker/src/rum_aggregation_worker.rs`
- `crates/infra/tests/postgres_rum_session_window.rs` (4 tests)
- `crates/infra/tests/postgres_rum_aggregation.rs` (3 tests)
- `crates/infra/tests/payg_rum_session_billing.rs` (4 tests)
- `docs/internal/payg-hardening-closure.md` (this file)

**Modified:**

- `crates/domain/src/payg.rs`
- `crates/domain/src/entities/rum_session.rs`
- `crates/domain/src/entities/mod.rs`
- `crates/application/src/ports/rum_event_repository.rs`
- `crates/application/src/ports/mod.rs`
- `crates/application/src/use_cases/ingest_rum_event.rs`
- `crates/infra/src/repositories/postgres_rum_event_repository.rs`
- `crates/infra/src/repositories/postgres_rum_analytics_repository.rs`
- `crates/infra/src/repositories/in_memory_wallet_repository.rs`
- `crates/infra/src/repositories/mod.rs`
- `crates/worker/src/lib.rs`
- `crates/worker/src/main.rs`
- `crates/api/src/handlers/rum_ingest.rs`
- `crates/api/src/state.rs`

## Tests

| Suite | Result |
|-------|--------|
| `infra::postgres_rum_session_window` | 4/4 |
| `infra::postgres_rum_aggregation` | 3/3 |
| `infra::payg_rum_session_billing` | 4/4 |
| Full workspace test | pass |
| Full workspace clippy | clean |

## Design notes

- **Single-statement upsert** — the extend-vs-restart decision lives in
  the `CASE WHEN ... last_seen_at < cutoff` clause of a single
  `INSERT ... ON CONFLICT ... DO UPDATE`. Concurrent beacons for the same
  `(site_id, session_key)` cannot create duplicate session rows and cannot
  double-charge: exactly one reports `created = true`.
- **Aggregate-before-delete** — the retention worker rolls raw rows into
  `rum_daily_aggregates` first and only then deletes. The aggregate step
  uses `ON CONFLICT DO UPDATE`, so a crash between the two steps is safe
  to retry.
- **Date-based cutoff** — a day is aggregated once *every* timestamp in it
  is older than the retention window. Both the aggregate filter and the
  delete filter use the same `cutoff_midnight`, so a day is either fully
  raw or fully aggregated — never split.
- **Session-start-only counting** — `total_sessions` counts rows by
  `started_at`, which is the same identity the raw `session_stats` query
  uses. Sessions that span midnight UTC are attributed to the day they
  started in, matching the raw side.
- **Hybrid read** — the dashboard API is unchanged; only the two
  aggregate-capable queries (`overview`, `session_stats`) merge sources.
  Detailed breakdowns deliberately stay raw-only so the schema does not
  grow to carry JSONB percentiles and top-error maps.
- **Retired-but-queryable** — `rum_event` remains in
  `payg_pricing_configs` for audit; only the retirement filter row stops
  `list_active()` from advertising it.

## Not in this phase

- **Percentile / top-error aggregation for >7 days.** Deliberately out of
  scope. Extending `rum_daily_aggregates` with JSONB columns would be the
  follow-up if the dashboard needs them.
- **Live migration of existing raw rows.** The aggregation worker will
  backfill `rum_daily_aggregates` on its first run for any site with raw
  rows older than 7 days.

## Pre-existing state

- `network_egress` continues to be metered and billed as before.
- The other GAP 3 phases (A, B, C, D) are unaffected.
