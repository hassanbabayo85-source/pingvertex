# PingVertex Leptos Frontend Migration — Handover

## Worker: L9 — CI Workflow for Leptos Frontend

### Scope
- Add CI workflow for Leptos frontend (apps/leptos-dashboard).
- Ensure frontend builds and tests run on every push/PR.
- Do not break existing backend CI.
- Follow existing patterns from H7/H10 backend CI.

### Status
COMPLETE — CI workflow added as a separate file; no existing jobs modified.

### Files Created/Modified/Deleted
- **Created** `.github/workflows/leptos-frontend.yml` — new workflow for frontend.
- **Updated** `HANDOVER.md` — this file.

### Frontend Changes
- None in application code; only CI infrastructure.

### Tests
Local verification commands (to be run in terminal):
- `cd apps/leptos-dashboard && trunk build --release`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test -p leptos-dashboard`

CI will run the same steps automatically on push/PR, but the test step is limited to the frontend crate to avoid requiring database services.

### Deviations from Prompt
- Used a **separate workflow file** (`.github/workflows/leptos-frontend.yml`) instead of modifying existing backend CI. This is safer and avoids accidental breakage; both workflows will run on their respective triggers.
- Added a conditional step for building Docker image (`Dockerfile.frontend`) only if that file exists (in case L7 did not create it).
- Did not include `cargo audit`/`cargo deny`/trivy for frontend; those are backend-specific. Frontend has no additional dependencies beyond Rust crates (already covered by clippy and tests). If needed later, they can be added.
- **Changed test step from `cargo test --workspace` to `cargo test -p leptos-dashboard`** to avoid running backend integration tests that require PostgreSQL. This keeps the frontend CI lightweight and self-contained.

### Known Issues
- None.

### Notes for Next Worker
- The frontend CI is independent; backend CI remains untouched.
- If L7 provides a `Dockerfile.frontend`, the conditional docker build step will activate automatically.
- Ensure `Cargo.lock` is committed so caching works properly.
- For local full workspace tests, you may need to set up PostgreSQL and Redis; but the frontend CI does not run those tests.

### Completion Criteria
- CI workflow file present and syntactically valid.
- All steps pass when run manually or via GitHub Actions.
- Backend CI jobs remain intact and functional.

## Worker: Audit Remediation — 12 Remaining Findings (2026-09-02)

### Scope
Close all 12 remaining findings from the codebase/database audit
(migrations + domain + application + infra + API + RUM), without
breaking any existing logic. See the full audit findings document for
the original issue numbering/severity used below.

### Status
COMPLETE for issues #1, #2, #4–#12. Issue #3 is a documentation-only
resolution (see below) — no schema change was required or made.

### Files Created/Modified
- **Modified** `crates/domain/src/repositories/check_repository.rs`,
  `crates/application/src/ports/check_repository.rs`,
  `crates/infra/src/repositories/postgres_check_repository.rs` — Issue
  #1: `CheckRepository::find_by_id` now takes `(id, checked_at)`, matching
  `checks`' actual composite PRIMARY KEY since migration 0061. No
  production call site existed for this method (verified by search), so
  this was a safe, mechanical signature change; only test mocks needed
  updating (24 files across `crates/api/tests`, `crates/application/tests`,
  `crates/infra/tests`, `crates/worker/src/scheduler.rs`).
- **Created** `migrations/0069_enforce_rum_composite_tenant_integrity.sql`
  — Issues #5/#6: composite `(site_id, tenant_id)` and
  `(session_id, tenant_id, site_id)` foreign keys across the RUM table
  hierarchy (`rum_sessions`, `rum_page_views`, `rum_js_errors`,
  `rum_resource_errors`), same pattern as migration 0065's monitor-child
  tenant integrity fix.
- **Created** `migrations/0070_fix_diagnostic_reports_composite_uniqueness.sql`
  — Issue #2: replaces the stale `UNIQUE(check_id)` index with
  `UNIQUE(check_id, check_checked_at)`, matching the composite identity
  migrations 0060/0061 already established.
- **Created** `migrations/0071_checks_partition_reconciliation_and_health.sql`
  — Issues #4/#10/#12: `ensure_checks_partition` redefined
  (`CREATE OR REPLACE`, same signature) to use a schema-qualified
  `to_regclass` lookup instead of unqualified `pg_class.relname`; new
  `reconcile_checks_default_partition`, `checks_partition_maintenance`,
  and `checks_partition_health` functions.
- **Modified** `scripts/ensure_checks_partitions.sh` — now calls
  `checks_partition_maintenance` (ensure + reconcile) instead of
  `ensure_checks_partition` alone, prints `checks_partition_health`
  output every run, and warns (non-fatally) if the systemd timer isn't
  active on hosts that have `systemctl`.
- **Created** `scripts/install_checks_partition_timer.sh` — Issue #11:
  idempotent installer for the systemd service+timer
  (`daemon-reload` + `enable --now`), meant to be wired into a deploy
  pipeline instead of relying on a human remembering the manual steps.
- **Modified** `crates/api/src/handlers/rum_ingest.rs` — Issues #7/#8:
  implements the previously-documented-but-missing Origin/Referer vs
  `RumSite.domain` allow-list check (advisory/logging only — Origin and
  Referer remain client-controlled and are not treated as a security
  boundary); adds a second per-`(site_key, IP)` rate limit under the
  existing per-site limit so a public site_key alone can't be used to
  exhaust a site's whole quota.
- **Modified** `crates/infra/src/repositories/postgres_check_repository.rs`
  — Issue #9: added `id DESC` as an explicit tie-breaker after
  `checked_at DESC` in every ordered/paginated query, since `checked_at`
  is not unique and offset pagination on a non-unique sort key can
  otherwise skip or duplicate rows across pages.

### Issue #3 — no code change (documented here instead)
Migration 0061's safety guard (it refuses to run against non-empty
`checks`/`diagnostic_reports` tables) was verified still intact and
untouched. Per the audit's own required-fix options, and since this
project has explicitly confirmed pre-production/fresh-DB-only status (see
0061's own header comment), the correct action is documentation, not a
schema change: **0061 is fresh-database-only and must never be run as an
upgrade migration against a database with existing `checks` history.** If
this project ever needs to upgrade a database that already has real
`checks`/`diagnostic_reports` rows, a new migration implementing a
dual-write/backfill/swap strategy must be written for that specific
upgrade — do not attempt to modify or re-run 0061 for that purpose.

### Tests
Could not be executed in the environment this remediation was authored
in (no `cargo`/toolchain available). Run before deploying:
- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace` (requires a running Postgres — see
  `crates/infra/tests/common/mod.rs` for connection setup)
- Apply migrations 0069–0071 against a staging database (or a scratch
  copy of production) **before** applying to any environment with real
  data — 0069 in particular will fail loudly (by design) if any existing
  RUM rows are already cross-tenant-inconsistent, and 0071's
  `reconcile_checks_default_partition` moves live rows between tables.

### Deviations from Prompt
- Did not edit any existing migration file in place (0061, 0068, etc.) —
  followed this repo's own established convention (see migration 0063's
  comment: "migrations here are treated as append-only") and instead
  added new migrations, using `CREATE OR REPLACE FUNCTION` where a
  function's *definition* needed to change but its name/signature could
  stay stable for existing callers.
- Issue #1's "choose Option A or B" decision was made as **Option B**
  (composite `(CheckId, checked_at)` identity), since migrations
  0060/0061 had already moved the database itself to that model — Option
  A would have required an additional migration to add back a
  single-column uniqueness guarantee the partitioned table structure
  doesn't naturally provide.
- Issue #7 was implemented as **Option A** (implement the allow-list
  check) rather than Option B (remove the misleading docs), per the
  audit's own recommendation, and kept strictly advisory/logging-only per
  its own security nuance about Origin/Referer not being a strong
  boundary.

### Known Issues
- None identified beyond what's noted in the "Tests" section above
  (unverified against a real toolchain/database in this environment).

### Notes for Next Worker
- If a future migration needs `rum_page_views.page_view_id`-level
  tenant/site composite FKs on `rum_js_errors`/`rum_resource_errors`,
  see the follow-up note left at the bottom of migration 0069 — it was
  deliberately scoped out of this remediation as lower marginal value.
- `checks_partition_health(months_ahead)` (migration 0071) is meant to be
  wired into whatever metrics/alerting this project uses next; it
  currently only surfaces via `ensure_checks_partitions.sh`'s log output.
