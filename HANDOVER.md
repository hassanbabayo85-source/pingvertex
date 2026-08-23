# PingVertex Backend-Track Worker Handover

## Worker
Worker B32

## Scope
Status Pages Backend (data model + logic only, NO UI):
- create/edit/delete status page
- monitor selection
- public read endpoint (status, uptime, incidents, maintenance windows, branding)
- incident history for display
- security: public read leaks nothing tenant-internal; tenant isolation on management endpoints

## Status
COMPLETE

## Files Created
- `crates/api/tests/public_status_maintenance_windows.rs`

## Files Modified
- `crates/api/src/handlers/types.rs` — added `PublicMaintenanceWindowResponse`; added `maintenance_windows` field to `PublicMonitorResponse`; `PublicMonitorResponse::from_parts` now takes a `&[MaintenanceWindow]` parameter.
- `crates/api/src/handlers/get_public_status.rs` — fetches each monitor's active maintenance windows via `state.maintenance_window_repository.list_active_by_monitor` and passes them into `from_parts`.
- `crates/api/src/handlers/public_status_page.rs` (`render_public_status_page`) — same wiring, used by both `/public/status/{subdomain}` and verified-custom-domain host routing.

## Files Deleted
None.

## Backend Changes
No new use cases, entities, or repositories — all of create/update/delete/list status pages, monitor selection, custom-domain verification + host-header routing, logo upload, and the public read endpoint (with a Redis-backed cache) already existed in the codebase prior to this worker running, undocumented in the previous HANDOVER.md. See "Deviations from prompt" below.

## Database Changes
None. `status_pages` (0018) and `maintenance_windows` tables already existed.

## API Changes
- `GET /api/v1/public/status` and `GET /api/v1/public/status/{subdomain}` (and the verified-custom-domain host-routed equivalent) now include a `maintenance_windows` array per monitor: `[{ id, starts_at, ends_at, reason }]`, limited to currently-active windows, sorted most-recent-start-first, truncated to 10.

## Security Changes
- `PublicMaintenanceWindowResponse` deliberately excludes `tenant_id` (the only other field on the domain entity), matching the existing pattern for `PublicIncidentResponse` / `PublicMonitorResponse`.
- No change to tenant isolation, monitor-ownership checks, or custom-domain verification — all pre-existing and already covered by tests in `crates/api/tests/status_pages.rs` and `crates/application/tests/*status_page*`.

## Tests
- Added `crates/api/tests/public_status_maintenance_windows.rs` (2 tests, both pass): confirms the maintenance-window repository correctly filters to active-only windows, and confirms the public status-page endpoint's response shape still round-trips correctly with the new field wired in.
- Pre-existing status-page test suite (`status_pages.rs`, 7 tests; `application` crate's create/update/delete/list/upload-logo status page tests, 20 tests total) was NOT modified and all still pass.
- Full workspace run: **PASS** — every crate (domain, application, infra, api, worker, agent, shared) green, including Postgres-backed integration tests. Two pre-existing warnings (unused `SocketAddr` import in `worker/src/proxy_checker.rs`, unused `BoxFuture` alias in `worker/tests/flow_execution.rs`) are unrelated to this worker's changes and were left untouched per scope discipline.

## Deviations from prompt
1. **Status pages backend was already ~95% built.** Before writing any code I inspected the actual codebase (per the Judgment Rule) and found: the `StatusPage` domain entity, migration (`0018_create_status_pages.sql`), `StatusPageRepository` (port + Postgres + in-memory impls), `StatusPageCache` (port + Redis impl), all five use cases (create/update/delete/list/upload-logo) with tenant-isolation and monitor-ownership checks, API handlers and routes for both management and public reads, verified-custom-domain host-header routing middleware, and a full test suite already asserting the exact security properties this task calls for (no tenant leakage on public reads, monitor-selection scoping, tenant isolation on management endpoints). None of this was mentioned in the prior HANDOVER.md (B31's), meaning an earlier, undocumented worker built it. Per the Judgment Rule I treated re-implementing any of it as out of scope / risk of regression, and instead only fixed the one genuine gap.
2. **Only gap found: maintenance windows were absent from the public response**, despite `MaintenanceWindowRepository` (with `list_active_by_monitor`, already used by the worker's incident evaluator) existing and being fully wired into `AppState`. This is a minor, additive fix — no design decision, no architecture change — so per the Judgment Rule I implemented it directly rather than stopping to ask: added `PublicMaintenanceWindowResponse`, extended `PublicMonitorResponse`/`from_parts`, and wired the fetch into both public-read code paths (`get_public_status`, `render_public_status_page`).
3. **New test file instead of editing the existing large `status_pages.rs`.** To stay within the effort budget and avoid reproducing (and risking a transcription error in) an already-long, already-passing test file, the new maintenance-window tests live in their own file with their own minimal test scaffolding, following the same pattern already used across `crates/api/tests/*.rs` (each file defines its own local Noop repos rather than sharing a `common` module, unlike the `application` crate's tests).

## User Test Commands
```bash
cargo fmt --all
cargo check --workspace
cargo test --workspace
```

## Known Issues
None.

## Known Risks
None outstanding — the first test-run attempt surfaced a missing `use application::ports::MaintenanceWindowRepository;` trait import in the new test file, which was fixed and reconfirmed passing.

## Important Notes For Next Worker
- SOURCE OF TRUTH is the actual codebase, not this file. In particular, status pages, custom domains, and the public status endpoint are already substantially complete — inspect the real files (`crates/domain/src/entities/status_page.rs`, `crates/api/src/handlers/*status_page*`, `crates/api/src/middleware/custom_domain_routing.rs`) before assuming anything here needs to be built.
- If a future worker touches `PublicMonitorResponse::from_parts`, note its signature changed in this worker to take a fourth `&[MaintenanceWindow]` argument — update all call sites accordingly.

## Completion Criteria
Met: `cargo fmt --all && cargo check --workspace && cargo test --workspace` all passed with 0 failures across the full workspace, confirmed by the user.
