# Handover — Path 3: Dashboard Overview Cockpit Redesign

## Path/Phase
Path 3 — Dashboard Overview Cockpit Redesign (frontend-only)

## Scope
Redesign `apps/leptos-dashboard/src/pages/dashboard_overview.rs` to use
Path 2's design-system components (PageHeader, MetricCard, StatusBadge,
LoadingSpinner, ErrorState) instead of raw divs, add a manual Refresh
action, add a "Monitor Status" mini-list (first 5 monitors), and an
"Active Incidents" widget backed by the aggregate count only (no N+1
per-monitor incident fetching, no chart, no fake time-range control).

## File Ownership
- **Owned Files (modified)**: `apps/leptos-dashboard/src/pages/dashboard_overview.rs`
- **Read-Only (consulted, not modified)**: `apps/leptos-dashboard/src/api/monitoring.rs`,
  `apps/leptos-dashboard/src/components/*` (page_header.rs, metric_card.rs,
  status_badge.rs, error_state.rs, loading.rs), `apps/leptos-dashboard/src/routes/mod.rs`,
  `crates/api/src/routes/dashboard.rs`
- **Forbidden (not touched)**: `apps/leptos-dashboard/src/api/*` (write),
  `apps/leptos-dashboard/src/components/*` (write), `style.css`,
  `monitors.rs`, `monitor_detail.rs`, `monitor_form.rs`, `routes/mod.rs`,
  all `crates/*`

## Status
COMPLETE — `cargo fmt --all` clean, `cargo clippy -p leptos-dashboard
--target wasm32-unknown-unknown -- -D warnings` passes with zero
warnings (exit 0), `cargo check -p leptos-dashboard --target
wasm32-unknown-unknown` passes (exit 0). Manual `trunk serve` visual
verification still pending — see User Test Commands.

## Files Created/Modified/Deleted
- **Modified**: `apps/leptos-dashboard/src/pages/dashboard_overview.rs`
  (only file changed by this path; see git diff --stat below for
  confirmation no other file was touched).

## Changes
1. `PageHeader` (from Path 2) with title "Overview" and a "Refresh"
   button in its action slot, wired to a `load_data()` closure that
   re-invokes `fetch_overview()` and `fetch_monitors()` (manual refresh,
   no auto-poll, per spec for this phase).
2. Metric strip: 6 `MetricCard`s (Monitors, Up, Down, Uptime, Avg
   Response, Active Incidents) replacing the old raw `div.card` list.
3. "Monitor Status" mini-list: first `RECENT_MONITORS_LIMIT = 5`
   monitors from `fetch_monitors()`, each rendered with `StatusBadge`.
4. "Active Incidents" widget: shows `Overview.active_incidents` count
   only (no per-incident list, no N+1 fetch loop), with a link to
   `/monitors` (not `/incidents`, since that route does not exist yet).
5. `LoadingSpinner` / `ErrorState` (with retry wired to `load_data`)
   replace the old raw `div.loading` / `div.error`.
6. No chart — no charting library available in this workspace
   (`Cargo.toml` confirmed via Phase 0 inspection); deferred.
7. No time-range dropdown — `GET /dashboard/overview` handler
   (`crates/api/src/routes/dashboard.rs`, confirmed via Phase 0
   inspection) takes no query extractor, so a disconnected/fake control
   was intentionally omitted rather than built.

## Bug Fix — PageHeader children type mismatch (found via cargo check)
`PageHeader`'s `children: Option<Children>` field, in read-only
`components/page_header.rs` (owned by Path 2), is declared without
`#[prop(optional)]`. This makes the builder's `.children(...)` method
expect `Option<Box<dyn FnOnce() -> AnyView + Send>>` directly, not a
bare closure. The initial implementation passed a bare closure and
failed with `E0308` (`cargo clippy`/`cargo check`, both reproduced).

**Fix (within this path's owned file only)**: changed the call site to
`.children(Some(Box::new(move || { view! { ... }.into_any() })))`,
matching the field's actual expected type. No change to `page_header.rs`
itself (forbidden file). Verified via `cargo clippy -D warnings` and
`cargo check`, both now exit 0.

**Recommendation for Path 2 owner**: consider adding `#[prop(optional)]`
to `PageHeader`'s `children` field, or changing its type, since any
other page passing children to `PageHeader` will hit the same compile
error and need the same workaround.

## Tests
Commands run (orchestrator-executed, output reproduced above in chat):
cd apps/leptos-dashboard
cargo fmt --all                                                            -> exit 0
cargo clippy -p leptos-dashboard --target wasm32-unknown-unknown -- -D warnings  -> exit 0, 0 warnings
cargo check -p leptos-dashboard --target wasm32-unknown-unknown            -> exit 0

Note: a pre-existing future-incompat notice for proc-macro-error2
v2.0.1 appears in clippy/check output; this is unrelated to this
path's change (also noted in HANDOVER-path-2.md) and is not a warning
under -D warnings (build still exits 0).

Not yet run: `trunk serve` manual visual check (requires orchestrator to
run and view in browser — see User Test Commands).

## Deviations from Prompt
- **PageHeader children type workaround** (see "Bug Fix" section above)
  — required to make the page compile at all; scoped entirely to the
  owned file, no forbidden file touched. This is a MAJOR issue per the
  prompt's own JUDGMENT RULE (compile error blocks completion), but was
  resolved within owned-file boundaries without needing sign-off, since
  no security/architecture decision was involved — purely a Leptos
  builder-API type mismatch.

## Suggested Commit Message(s)
feat(dashboard): redesign overview page with design-system components

- Replace raw div cards with PageHeader/MetricCard/StatusBadge/
  LoadingSpinner/ErrorState from the Path 2 component library
- Add manual refresh action, 5-monitor status mini-list, and an
  active-incidents count widget (no N+1 fetch, no chart, no
  disconnected time-range control)
- Work around PageHeader's non-optional children field by passing
  Some(Box::new(...)) explicitly at the call site

## User Test Commands
```bash
cd apps/leptos-dashboard && trunk serve
```
Then open the served URL (after logging in) and check:
- "Overview" header renders with a working "Refresh" button.
- Six metric cards render with live values (Monitors/Up/Down/Uptime/Avg
  Response/Active Incidents).
- "Monitor Status" shows up to 5 monitors with colored status badges
  (or "No monitors yet." if empty).
- "Active Incidents" shows a count and a working "View monitors" link
  to `/monitors`.
- Loading spinner shows briefly on refresh; forcing an API error (e.g.
  stop the backend) shows ErrorState with a working Retry button.

## Rollback Plan
Single-file change: `git checkout -- apps/leptos-dashboard/src/pages/dashboard_overview.rs`
reverts to the pre-Path-3 version. No migrations, no API/route/component
changes — rollback is safe at any time and affects only this one page.

## Known Issues/Risks
- Time-range control ("Last 24h" etc.) is a known gap — backend
  `/dashboard/overview` has no query-param support yet. Needs a Path 1
  (or later) follow-up to add query extraction + repository filtering
  before a dropdown can be wired up honestly.
- Full per-incident list on this page is deferred to Phase 2, pending a
  dedicated `/incidents` aggregate backend endpoint (per prompt's
  NON-NEGOTIABLE 16) — avoids an N+1 client-side fetch loop.
- No chart (uptime/response-time trend) — no charting library in
  `Cargo.toml` yet and no dependency-addition authorization for this
  path (per NON-NEGOTIABLE 17).
- `PageHeader`'s `children` prop typing issue (see Bug Fix section)
  will resurface for any other page adopting `PageHeader` with action
  buttons until Path 2's owner addresses it.

## Notes for Next Path
- Dashboard overview page now fully adopts the Path 2 component set;
  use it as a reference pattern for `MetricCard`/`StatusBadge`/
  `LoadingSpinner`/`ErrorState` usage in other pages.
- The `PageHeader` children workaround (`Some(Box::new(|| {...}.into_any()))`)
  should be copied verbatim by any other page passing action buttons to
  `PageHeader`, until/unless Path 2's owner changes the component's
  prop type.

## Completion Criteria
- [x] Page uses MetricCard/StatusBadge/LoadingState(Spinner)/ErrorState/
      PageHeader from Path 2, no raw divs for these.
- [x] No N+1 incident-fetching loop.
- [x] No fake/disconnected time-range control.
- [x] `cargo check`/`cargo clippy` pass (both exit 0, confirmed by
      orchestrator-run output above).
