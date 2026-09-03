# HANDOVER — Path 1: Frontend Product Shell & Navigation

## Status: COMPLETE (pending human-run build verification — see below)

## What changed
- **New file** `src/components/navigation.rs` — single data source (`NAV_GROUPS`)
  describing every nav group/item from the orchestration plan (Overview,
  Monitoring, User Experience, Public, Team, Developer, Billing, Support,
  Settings). Each item carries a `NavStatus`: `Available` or `ComingSoon`.
  Only `/`, `/monitors`, `/rum` are marked `Available`, because those are the
  only routes actually registered in `routes/mod.rs` today (verified by
  reading the file directly, not assumed).
- **Modified** `src/components/layout.rs` — sidebar now renders from
  `NAV_GROUPS` instead of a hardcoded three-link list. `Available` items
  render as a real `<A>` (clickable, router-aware, keeps existing
  active-link styling). `ComingSoon` items render as a disabled `<span
  aria-disabled="true">` with a "soon" badge — never a `<a href>`, so there
  is no dead/broken link anywhere in the shell.
- **Modified** `src/components/mod.rs` — added `pub mod navigation;`.
- **Modified** `style.css` — added `.sidebar-group-disabled` and
  `.sidebar-soon-badge` rules only. No existing rule was changed.

## Explicitly NOT touched (per ownership rules)
- `routes/mod.rs`, `pages/mod.rs` — untouched. Zero new `<Route>` entries
  were added. This is intentionally Path 10's job.
- `components/require_auth.rs` — untouched, read-only reference only.

## How to flip an item live later (for whoever runs Path 10)
When a route lands in `routes/mod.rs`, change that single item's `status`
field in `navigation.rs` from `NavStatus::ComingSoon` to
`NavStatus::Available`. No other file needs to change for that link to
start working.

## Deviations from prompt
- `style.css` is not listed in Path 1's OWNED FILES in the original plan,
  and no other Path owns it either — this is a gap in the plan (no Path was
  assigned stylesheet ownership). I added two new, additive-only CSS rules
  under an explicit `/* Path 1 additions */` comment block so this is easy
  to find and revert if the orchestrator wants a different owner for CSS
  going forward. Logging this here per the OWNERSHIP OVERRIDE PROTOCOL.

## Verification NOT yet done (no cargo toolchain / no network in the sandbox
this was built in — must be run on your machine)
```
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p leptos-dashboard
trunk build   # or your usual dev build command, to sanity-check the CSR bundle
```
Existing `/`, `/monitors`, `/rum` links and the sign-out flow should be
manually clicked through after the build to confirm no visual regression.

## Confirmed findings relevant to OTHER Paths (read directly from source,
not guessed — passing these to the orchestrator so Paths 8/9/4 don't have
to re-derive them)
1. **Path 8 blocker is real.** `crates/api/src/routes/monitors.rs` registers
   only `POST /monitors/{id}/maintenance-windows` and
   `POST /escalation-policies`. No `GET`/list route exists for either. Path
   8 cannot build a "list maintenance windows" or "list escalation
   policies" UI against today's backend — this is a genuine backend gap,
   not a frontend task.
2. **Path 9's RUM 0.0-fill bug is real.** In
   `apps/leptos-dashboard/src/api/rum.rs`, `fetch_rum_dashboard()` (the
   legacy adapter feeding `pages/rum_dashboard.rs`) hardcodes
   `avg_fcp: 0.0`, `avg_ttfb: 0.0` (overview), `avg_cls: 0.0`, `avg_fcp:
   0.0` (slow pages), and `avg_cls: 0.0`, `avg_fcp: 0.0` (trend) in three
   separate places — none of these are read from the typed backend
   response. A monitor with genuinely no FCP data will show "0.0 ms" next
   to real monitors with data, which is misleading. Path 9 should treat
   this as confirmed, not merely alleged by the earlier audit.
3. **Path 4's secret-disclosure question is answered.**
   `crates/api/src/handlers/create_api_key.rs` builds
   `ApiKeyCreateResponse::new(created.api_key, created.raw_key)` — the raw
   secret is returned only from the create endpoint's response body. Path 4
   still needs to confirm `list_api_keys.rs` does NOT also return
   `raw_key` before finalizing the "shown once" UI copy, but the create-path
   behavior is confirmed.
