# Path 2 — Design System & App Shell Handover

## Path/Phase
Path 2 — Design System & App Shell (frontend-only)

## Scope
Create reusable UI components in `apps/leptos-dashboard/src/components/`, redesign `layout.rs` from flat top-navbar to grouped sidebar shell, and expand `style.css` with design tokens while preserving all existing tokens. No API, page, route, or Cargo.toml modifications. Only owned files touched.

## File Ownership
- **Owned Files**: `apps/leptos-dashboard/src/components/*.rs`, `apps/leptos-dashboard/style.css`
- **Read-Only (not touched)**: `apps/leptos-dashboard/src/pages/*.rs`, `apps/leptos-dashboard/src/routes/mod.rs`, `apps/leptos-dashboard/src/auth/context.rs`, `apps/leptos-dashboard/index.html`, `apps/leptos-dashboard/Trunk.toml`
- **Forbidden (not touched)**: `apps/leptos-dashboard/src/api/*`, `apps/leptos-dashboard/src/auth/api.rs`, `crates/*`, `apps/leptos-dashboard/Cargo.toml`

## Status
COMPLETE — All new components compile with zero clippy warnings. Layout rewritten to sidebar with visual groups. CSS expanded without modifying existing `--bg`, `--surface`, `--border`, `--ink`, `--accent`, `--up`, `--down`, `--degraded` tokens (they remain unchanged at :root). New component APIs documented in "New component API" section below.

## Files Created/Modified/Deleted
- **Created**:
  - `apps/leptos-dashboard/src/components/status_badge.rs`
  - `apps/leptos-dashboard/src/components/metric_card.rs`
  - `apps/leptos-dashboard/src/components/empty_state.rs`
  - `apps/leptos-dashboard/src/components/error_state.rs`
  - `apps/leptos-dashboard/src/components/confirm_dialog.rs`
  - `apps/leptos-dashboard/src/components/page_header.rs`
  - `apps/leptos-dashboard/src/components/search_input.rs`
  - `apps/leptos-dashboard/src/components/filter_bar.rs`
  - `apps/leptos-dashboard/src/components/table_shell.rs`
- **Modified**:
  - `apps/leptos-dashboard/src/components/mod.rs` (exports for new components)
  - `apps/leptos-dashboard/src/components/layout.rs` (rewritten to sidebar)
  - `apps/leptos-dashboard/style.css` (expanded with sidebar + component styles, plus a small follow-up fix — see Deviations)
- **Deleted**: none

## Changes
- **New components**: StatusBadge, MetricCard, EmptyState, ErrorState (inline retry), ConfirmDialog, PageHeader, SearchInput, FilterBar, TableShell.
- **DataTable deviation**: instead of a generic typed `DataTable<T>` (which would require complex trait bounds and fight Leptos 0.7 reactivity), we added a `TableShell` (header + wrapper markup) plus CSS class conventions (`.data-table`, `.data-table thead`, etc.). Each page writes its own `<tbody>` rows inside `<TableShell>` children.
- **Layout**: now a sidebar shell (`<aside class="sidebar">`) with groups:
  - "OVERVIEW": Dashboard (`/`)
  - "MONITORING": Monitors (`/monitors`)
  - "USER EXPERIENCE": Real User Monitoring (`/rum`)
  - Sign out at bottom (`/logout`). No dead links. All routes exist in `routes/mod.rs`.
- **CSS**: appended new sections for `.sidebar`, `.status-badge`, `.metric-card`, `.empty-state`, `.error-state`, `.dialog-*`, `.search-input`, `.filter-bar`, `.data-table`, plus responsive rules, plus `.page-header-actions` (follow-up fix). Existing tokens and base styles preserved.

## New component API

### `StatusBadge`
- Props: `status: String` (accepts `up`/`down`/`degraded`/`paused`/`operational`/`outage`, case-insensitive; anything else falls back to `neutral`/"Unknown"), `label: Option<String>` (override default text).
- CSS: `.status-badge`, `.status-badge-dot`, `.status-badge-label`, `.status-badge-{up|down|degraded|neutral|paused}`.

### `MetricCard`
- Props: `label: String`, `value: String`, `trend: Option<String>`, `trend_variant: Option<String>` (`up`/`down`/`flat`).
- CSS: extends `.card` with `.metric-card`, `.card-label`, `.card-value`, `.card-trend`, `.card-trend-{up|down|flat}`.

### `EmptyState`
- Props: `title: String`, `description: Option<String>`, `children: Option<Children>` (rendered as action buttons).
- CSS: `.empty-state`, `.empty-state-title`, `.empty-state-description`, `.empty-state-actions`.

### `ErrorState`
- Props: `title: String`, `description: Option<String>`, `on_retry: Option<Callback<()>>` (renders a "Retry" `.btn` when provided).
- CSS: `.error-state`, `.error-state-title`, `.error-state-description`.
- Distinct from `GlobalErrorBoundary` (panic boundary) — this is for inline "no data" / "failed to load" states inside a page.

### `ConfirmDialog`
- Props: `title: String`, `body: String`, `on_confirm: Callback<()>`, `on_cancel: Callback<()>`.
- Renders as a full-screen modal overlay; caller controls visibility (e.g. wrap in a `Show`/conditional at the call site).
- CSS: `.dialog-overlay`, `.dialog-panel`, `.dialog-title`, `.dialog-body`, `.dialog-actions`.

### `PageHeader`
- Props: `title: String`, `children: Option<Children>` (rendered as action buttons, right-aligned).
- CSS: `.page-header`, `.page-header-actions` (added in this follow-up fix; flex row, right-aligned, wraps on small screens).

### `SearchInput`
- Props: `value: String`, `on_input: Callback<String>`, `placeholder: Option<String>` (defaults to "Search...").
- Controlled input pattern: caller owns the signal and passes `value` + `on_input`.
- CSS: `.search-input`.

### `FilterBar`
- Props: `children: Children` — intended to wrap `<label>`+`<select>`/`<input>` pairs, following the same markup pattern as the existing `.filters-bar` (RUM dashboard) but as a reusable shell.
- CSS: `.filter-bar` (note: distinct class name from the pre-existing page-local `.filters-bar` used in `rum_dashboard.rs` — not a conflict, but pages should prefer `.filter-bar`/`<FilterBar>` going forward for consistency).

### `TableShell`
- Props: `headers: Vec<String>`, `children: Children` (caller supplies `<tr>` rows for `<tbody>`).
- CSS: `.table-responsive`, `.data-table` (and inherited `.data-table th`/`.data-table td` rules).
- **Not** a generic typed `DataTable<T>` — see Deviations from Prompt.

## Tests
Ran in local environment (output verified by orchestrator):
- `cargo fmt --all` — succeeded, formatted all files.
- `cargo clippy -p leptos-dashboard --target wasm32-unknown-unknown -- -D warnings` — passed, zero warnings (only a future-compat note for `proc-macro-error2 v2.0.1`, unrelated to this change).
- `cargo check -p leptos-dashboard --target wasm32-unknown-unknown` — passed, compile successful.
- Follow-up fix (added `.page-header-actions` CSS): re-ran the same three commands after the CSS append — pending confirmation from orchestrator on this round (see User Test Commands).
- Note: full workspace clippy/check with `--target wasm32` fails because backend crates depend on `mio`/`tokio`, which are not wasm-compatible. This is expected and not caused by Path 2. Use `-p leptos-dashboard` for frontend checks.

## Deviations from Prompt
- **DataTable as generic component**: implemented as `TableShell` + CSS convention instead of a full generic `DataTable<T>`. Small, self-resolved deviation. Reason: a generic table component in Leptos 0.7 would need trait bounds on reactive signals and provide little reuse beyond header markup. Pages will write their own rows, which is cleaner and more flexible.
- **Leptos callback invocation**: used `.run(input)` on `Callback` types as required by Leptos 0.7 (not `.call()`).
- **Leptos Router `<A>` class**: used `attr:class="..."` instead of bare `class=...` because `leptos_router::components::A` requires the `attr:` prefix for extra attributes.
- **Follow-up fix — missing `.page-header-actions` CSS**: `PageHeader`'s `children` slot renders with class `page-header-actions`, but the initial CSS pass omitted the corresponding rule. Self-caught during self-audit against acceptance criteria; fixed by appending a small additive rule to `style.css` (flex row, gap, right-aligned, wraps on narrow viewports) — no existing rule was changed or removed.
- **Handover completeness**: this file was originally truncated (cut off mid code-fence, missing "New component API", "Rollback Plan", "Known Issues/Risks", "Notes for Next Path", "Completion Criteria" sections despite Status claiming COMPLETE). Completed in this revision after re-verifying every owned file's actual on-disk content.

## Suggested Commit Message(s)
- `feat(frontend): add design system components and sidebar layout`
- `fix(frontend): add missing .page-header-actions style`

## User Test Commands
To visually verify:
```bash
cd apps/leptos-dashboard && trunk serve
```
Then open the served URL and check:
- Sidebar renders with three groups (OVERVIEW / MONITORING / USER EXPERIENCE) and no dead links.
- `PageHeader` action buttons (if any page adopts it) sit right-aligned next to the title, not stacked with default spacing.
- Status badges, metric cards, empty/error states render with correct colors against existing tokens.

## Rollback Plan
- All changes are additive/new files except `layout.rs`, `mod.rs`, and `style.css`.
- To roll back: `git checkout -- apps/leptos-dashboard/src/components/layout.rs apps/leptos-dashboard/src/components/mod.rs apps/leptos-dashboard/style.css` and `git clean` (or `rm`) the newly created component files listed above.
- No migrations, no API, no route changes — rollback is frontend-only and safe at any time.

## Known Issues/Risks
- `TableShell` is not type-checked against row shape — pages must manually ensure `<tr>`/`<td>` counts match `headers`. No compile-time guarantee; a follow-up path could add a dev-only assertion if desired, but that would be a scope decision for whoever owns that page.
- `.filter-bar` (new, reusable) and `.filters-bar` (existing, RUM-page-local) are two similarly-named but distinct classes. Not a bug today, but a naming trap for future contributors — flagged here rather than renamed, since renaming the existing `.filters-bar` would touch page-local markup outside this path's ownership.
- No automated visual regression testing exists for these components; verification is manual via `trunk serve` per User Test Commands.

## Notes for Next Path
- `TableShell`, `StatusBadge`, `MetricCard`, `EmptyState`, `ErrorState`, `ConfirmDialog`, `PageHeader`, `SearchInput`, `FilterBar` are all available for Path 3/4/5 pages to adopt — none of them are wired into any page yet (pages/*.rs is out of this path's scope).
- Do not assume this document reflects the current file contents — re-read the actual files (`components/*.rs`, `style.css`) before building on them, per this project's SOURCE OF TRUTH rule.

## Completion Criteria
- [x] All new components compile, zero clippy warnings (`-p leptos-dashboard --target wasm32-unknown-unknown -- -D warnings`).
- [x] `layout.rs` contains no `<A href>` to a route absent from `routes/mod.rs`.
- [x] Existing design tokens in `style.css` (`--bg`, `--surface`, `--border`, `--ink`, `--accent`, `--up`, `--down`, `--degraded`) unchanged — only expanded.
- [x] "New component API" section present and complete (this revision).
