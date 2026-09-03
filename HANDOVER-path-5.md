# Path 5 — Monitor Detail + Monitor Form Redesign Handover

## Path/Phase
Path 5 — Monitor Detail + Monitor Form Redesign (frontend-only)

## Scope
Redesign `monitor_detail.rs` (PageHeader/MetricCard/StatusBadge/ConfirmDialog/TableShell adoption, Acknowledge + Send test actions) and `monitor_form.rs` (conditional fields per monitor kind). Maintenance Windows, Escalation Policies, API Flows out of scope.

## File Ownership
- **Owned (modified):** `apps/leptos-dashboard/src/pages/monitor_detail.rs`, `apps/leptos-dashboard/src/pages/monitor_form.rs`
- **Read-only (inspected, not modified):** `apps/leptos-dashboard/src/api/monitoring.rs`, `apps/leptos-dashboard/src/components/*`, `apps/leptos-dashboard/src/routes/mod.rs`, `crates/api/src/handlers/create_monitor.rs`, `crates/api/src/handlers/update_monitor.rs`, `crates/api/src/validation.rs`, `crates/domain/src/enums/monitor_kind.rs`, `crates/domain/src/entities/monitor.rs`
- **Forbidden (untouched):** all other files per prompt.

## Status
COMPLETE — `cargo fmt --all`, `cargo clippy -p leptos-dashboard --target wasm32-unknown-unknown -- -D warnings`, and `cargo check -p leptos-dashboard --target wasm32-unknown-unknown` all pass cleanly (only an unrelated `proc-macro-error2` future-incompatibility note, pre-existing, not introduced by this path). Two cross-path fixes were required and authorized by the orchestrator mid-path — see "Cross-Path Fixes (Authorized)" below.

## Files Created/Modified
### Owned (within Path 5's file ownership)
- **Modified** `apps/leptos-dashboard/src/pages/monitor_detail.rs`
- **Modified** `apps/leptos-dashboard/src/pages/monitor_form.rs`
- **Created** `HANDOVER-path-5.md` (this file)

### Cross-path (outside Path 5's OWNED FILES, explicitly authorized by orchestrator mid-path — see "Cross-Path Fixes (Authorized)" below)
- **Modified** `apps/leptos-dashboard/src/components/page_header.rs` (Path 2 file)
- **Modified** `apps/leptos-dashboard/src/components/empty_state.rs` (Path 2 file)
- **Modified** `apps/leptos-dashboard/src/pages/dashboard_overview.rs` (Path 3/4 file, FORBIDDEN to Path 5 per prompt)

## Changes
### monitor_detail.rs
- Replaced raw header buttons with `<PageHeader>` (title + Edit/Enable-Disable/Delete actions).
- Delete now opens `<ConfirmDialog>` (`show_delete_confirm` signal) instead of firing immediately.
- Added a 4-up metric strip using `<MetricCard>`: Uptime (`monitor.uptime_percentage`), Avg Response (`monitor.avg_response_time_ms`), Checks (`checks.len()`), Incidents (`incidents.len()`) — all from existing `Monitor`/`Vec` data, no new backend fields invented.
- Checks section now renders via `<TableShell>` with a `<StatusBadge status=c.status>` per row instead of raw `div.check-item` list; falls back to `<EmptyState>` when empty.
- Incidents section now renders via `<TableShell>`, `<StatusBadge>` per row, plus an "Acknowledge" button (calls `monitoring::acknowledge_incident(&incident_id)`, then reloads) shown only when `status` (lowercased) is not `"resolved"`/`"acknowledged"`.
- "Notification Channels" section renamed to "Notifications" in UI copy; each row now has a "Send test" button calling `monitoring::test_notification_channel(&channel_id)`, alongside the existing "Delete" button.
- Did NOT add Maintenance Windows / Escalation Policies / API Flows sections, per scope.

### monitor_form.rs
- Kind dropdown unchanged (http/tcp/heartbeat options preserved — removing an option was not requested and would be a bigger deviation than needed).
- URL field now shown ONLY when `kind == "http"`.
- Host + Port fields now shown ONLY when `kind == "tcp"` (both existing `MonitorPayload` fields — no new fields introduced).
- When `kind == "heartbeat"`, no URL/Host/Port fields render; a note is shown instead ("Heartbeat monitor configuration is not yet supported by this form...").
- No new fields (`heartbeat_token`/`expected_interval`/`grace_period`) were added, per the confirmed absence of those fields in `MonitorPayload` in `api/monitoring.rs`.

## Cross-Path Fixes (Authorized)

Adopting `<PageHeader>` and `<EmptyState>` in `monitor_detail.rs` (as required
by ACCEPTANCE) surfaced a real, pre-existing structural bug in those two
Path-2-owned components, and fixing it broke a third file outside Path 5's
ownership. Both fixes were explicitly authorized by the orchestrator in-chat
before being made; neither was invented or assumed.

### 1. `components/page_header.rs` + `components/empty_state.rs` (Path 2 files)
**Root cause (confirmed from actual compiler output, not guessed):** both
components declared `children: Option<Children>` without the
`#[prop(optional)]` attribute. In Leptos 0.7's `#[component]`/`TypedBuilder`
macro expansion, a bare `Option<Children>` field (no `#[prop(optional)]`) is
still treated as a *required* builder field of literal type
`Option<Children>`, and no `ToChildren<F> for Option<Children>` impl exists —
so any `<PageHeader>...</PageHeader>` / `<EmptyState ... />` usage inside a
`view!` macro failed with `E0277`/`E0061` regardless of how the call site was
written. This was not fixable from within Path 5's owned files.

**Fix:** added `#[prop(optional)]` to the `children` field in both files —
one line each, same pattern already used for other optional props in the
same files (e.g. `EmptyState`'s `#[prop(optional, into)] description`).

### 2. `pages/dashboard_overview.rs` (Path 3/4 file, FORBIDDEN to Path 5)
**Root cause:** this page called `PageHeader`'s generated builder directly
(not via the `view!` macro), with a comment explaining it was working around
the *exact* bug fixed above by manually wrapping the children closure in
`Some(Box::new(...))`. Once `#[prop(optional)]` was added, the builder setter
signature changed from taking `Option<Box<...>>` to taking the boxed closure
directly (Leptos's builder macro wraps it in `Option` internally for
optional props) — so the old `Some(Box::new(...))` call site no longer
type-checked (`E0308`).

**Fix:** changed `.children(Some(Box::new(move || {...})))` to
`.children(Box::new(move || {...}))`, and updated the file's own explanatory
comment to describe the new (now-correct) behavior instead of the old
workaround it was originally written around. No other line in this file was
touched.

### Verification
All three files, plus Path 5's own two owned files, were re-checked together
via the full TEST PROTOCOL (`cargo fmt --all`, `cargo clippy -p
leptos-dashboard --target wasm32-unknown-unknown -- -D warnings`, `cargo
check -p leptos-dashboard --target wasm32-unknown-unknown`) after each
change, and all passed clean on the final run.

**Flag for Path 2 owner:** the `#[prop(optional)]` fix to `page_header.rs` /
`empty_state.rs` should be reviewed by whoever owns Path 2's components,
since it changes those files' public prop signatures (backward compatible in
practice — no other call site broke except the one fixed above — but still
worth a second look from the component owner).

## MAJOR FINDING (per NON-NEGOTIABLE-14, not a stop condition)
`crates/domain/src/entities/monitor.rs` shows the domain layer already supports `MonitorKind::Heartbeat` (and `Tcp`/`Dns`/`Flow`) with real validation logic, and `crates/api/src/handlers/create_monitor.rs`/`update_monitor.rs` already accept `heartbeat_interval_seconds`/`heartbeat_grace_period_seconds`/`tcp_port`/`dns_*` on the wire (`MonitorWriteRequest`). However, the **frontend** `MonitorPayload` struct in `api/monitoring.rs` (Path 1, forbidden file) does not expose any of those fields — it only has `url`/`host`/`port`. This means:
- TCP monitors are partially usable from this UI (host+port map onto existing generic fields), assuming the backend's TCP validation only needs host+port-equivalent data — **not independently verified against `crates/domain` TCP validation rules**, since that would require inspecting more of `crates/domain/src/entities/monitor.rs` (validation branch for `MonitorKind::Tcp`) than was in scope to fully diff field-by-field.
- Heartbeat monitors cannot be meaningfully created/edited from this UI at all — there is no frontend field to carry `heartbeat_interval_seconds`/`heartbeat_grace_period_seconds`.
- DNS and Flow kinds aren't offered in the dropdown at all (pre-existing, unchanged by this path).

**Recommendation for a follow-up path:** extend `MonitorPayload` (Path 1's owned file) to include `tcp_port`, `dns_record_type`, `dns_expected_value`, `heartbeat_interval_seconds`, `heartbeat_grace_period_seconds`, `method`, `expected_status`, `assertions`, etc. to match `MonitorWriteRequest`, then re-open this form for full per-kind field support.

## Tests
**Commands run (final, from `apps/leptos-dashboard/`):**
```bash
cargo fmt --all -- --check
cargo clippy -p leptos-dashboard --target wasm32-unknown-unknown -- -D warnings
cargo check -p leptos-dashboard --target wasm32-unknown-unknown
```
**Result:** All three passed clean. `fmt --check` produced no diff. `clippy -D
warnings` and `check` both finished with no errors; the only output was an
unrelated `proc-macro-error2 v2.0.1` future-incompatibility note (pre-existing
dependency-level warning, not introduced by this path, not a `-D warnings`
failure since future-incompat notes aren't lint warnings).

Manual `trunk serve` verification of the acknowledge/test/conditional-form
flows (see User Test Commands below) has NOT been run yet — that remains the
orchestrator's/user's manual step before merging.

## Deviations from Prompt
- Used `<EmptyState>` (Path 2 component, not explicitly required by acceptance criteria) for empty Checks/Incidents/Notifications lists instead of raw `<p>` text, for consistency with the rest of the redesigned page. This is additive polish, not a required component, and does not conflict with any acceptance criterion.
- Kept the Kind dropdown showing all three original options (http/tcp/heartbeat) rather than removing "Heartbeat" — the prompt only asked for conditional field rendering, not removing options; removing an option felt like a bigger unrequested change than showing a "not yet supported" note.

## Suggested Commit Message(s)
- `feat(frontend): redesign monitor detail page with design system components`
- `feat(frontend): add acknowledge/test-notification actions to monitor detail`
- `feat(frontend): conditional URL/Host+Port fields on monitor form by kind`

## User Test Commands
```bash
cd apps/leptos-dashboard && trunk serve
```
Then manually verify:
- Open a monitor detail page: PageHeader actions (Edit/Enable-Disable/Delete) render right-aligned; Delete opens ConfirmDialog and only deletes on Confirm.
- Metric strip shows 4 MetricCards with real values.
- Checks table renders with StatusBadge per row.
- An unresolved incident shows an "Acknowledge" button; clicking it calls the API and the button disappears/status updates after reload.
- Each notification row has "Send test"; clicking it does not error (assuming backend endpoint reachable).
- Create/Edit a monitor: selecting "HTTP" shows only URL; selecting "TCP" shows only Host+Port; selecting "Heartbeat" shows the "not yet supported" note and no URL/Host/Port fields.

## Rollback Plan
Owned files only:
```bash
git checkout -- apps/leptos-dashboard/src/pages/monitor_detail.rs apps/leptos-dashboard/src/pages/monitor_form.rs
rm HANDOVER-path-5.md
```
Full rollback including the two authorized cross-path fixes:
```bash
git checkout -- apps/leptos-dashboard/src/pages/monitor_detail.rs \
  apps/leptos-dashboard/src/pages/monitor_form.rs \
  apps/leptos-dashboard/src/components/page_header.rs \
  apps/leptos-dashboard/src/components/empty_state.rs \
  apps/leptos-dashboard/src/pages/dashboard_overview.rs
rm HANDOVER-path-5.md
```
Note: rolling back `page_header.rs`/`empty_state.rs` alone (without also
rolling back `monitor_detail.rs`) would reintroduce the original
`E0277`/`E0061` compile failure, since `monitor_detail.rs` depends on the
`#[prop(optional)]` fix. No backend/API/route changes were made anywhere in
this path, so rollback is otherwise safe at any time.

## Known Issues/Risks
- See MAJOR FINDING above re: Heartbeat/DNS/Flow monitor kinds not fully supported end-to-end due to a frontend DTO gap (not introduced by this path).
- TCP field mapping (host+port) has not been cross-checked against `crates/domain`'s TCP-specific validation branch in detail; if that validation requires a field not present in the generic `host`/`port` pair, TCP monitor creation could still 400 from the backend — flagged for the orchestrator to confirm via manual `trunk serve` testing.
- The two cross-path fixes (`page_header.rs`, `empty_state.rs`, `dashboard_overview.rs`) are outside this path's normal ownership boundary; they were necessary to make this path's required component adoption compile at all, were authorized live by the orchestrator, and are called out explicitly above and in the scope-leak check — but they still represent this path touching files it would not normally own, and should be reviewed by the Path 2 and Path 3/4 owners respectively.
- No automated test harness exists for these pages (consistent with Path 1/2's precedent); verification beyond compile-checking is manual via `trunk serve`.

## Notes for Next Path
- If a future path extends `MonitorPayload` with per-kind fields, this form's conditional-rendering scaffolding (`{move || if kind.get() == "..." { ... } else { ().into_any() } }`) can be extended with additional branches for `dns`/`heartbeat` without restructuring.
- Do not assume this document reflects current file contents once further paths touch owned files here — re-read actual files per project's SOURCE OF TRUTH rule.

## Completion Criteria
- [x] `cargo fmt --all -- --check` clean
- [x] `cargo clippy -p leptos-dashboard --target wasm32-unknown-unknown -- -D warnings` clean (target confirmed via Path 1/2 precedent, consistent with `.cargo/config.toml`'s custom target-dir and CI's frontend job)
- [x] `cargo check -p leptos-dashboard --target wasm32-unknown-unknown` clean
- [ ] Manual `trunk serve` verification of acknowledge/test/conditional-form flows (see User Test Commands) — NOT YET RUN, remains a manual step for the user/orchestrator
- [x] Scope-leak check: `monitor_detail.rs`, `monitor_form.rs`, `HANDOVER-path-5.md` (owned/created) + `page_header.rs`, `empty_state.rs`, `dashboard_overview.rs` (cross-path, explicitly authorized — see "Cross-Path Fixes (Authorized)") — no other file touched, confirmed via `git status --short`.
