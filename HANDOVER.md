# Handover — Worker L5: Node.js Frontend Removal & Final Verification

## Worker
L5 — Node.js Frontend Removal (final worker)

## Scope
After verifying all Next.js dashboard functionality exists in the Leptos frontend (L1-L4 work), this worker removed the Node.js app (`apps/dashboard/`) and purged all Node.js frontend references from the project. Also fixed public status API URLs in Leptos.

## Status
**COMPLETE** — Node.js frontend fully removed. The project now builds and runs entirely with the Rust/Leptos frontend.

## Feature Parity Checklist
Next.js pages (apps/dashboard) vs Leptos:
- `/` (dashboard overview) → Leptos `DashboardOverviewPage`
- `/monitors` → `MonitorsPage`
- `/monitors/[id]` → `MonitorDetailPage` (+ edit form)
- `/monitors/new` → `MonitorFormPage`
- `/login` → `LoginPage`
- `/register` → `RegisterPage`
- `/status` → `StatusPage` (public)
- `/status/[subdomain]` → `SubdomainStatusPage` (public)
- `/profile` → **stub in Next.js, no Leptos page needed** (Next.js shows "Coming Next" only)
- `/test` → **internal only, removed** (not production)
All Next.js components for status are mirrored in Leptos (overall status, service list, incident history, uptime badge). Additional Leptos pages include forgot/reset password, logout, and RUM dashboard.

## Files Created/Modified/Deleted
- `apps/leptos-dashboard/src/public_api.rs` — fixed URL prefixes (`/api/v1/public/...`)
- `apps/leptos-dashboard/src/components/button.rs` — clippy fix (via `cargo clippy --fix`)
- `apps/leptos-dashboard/src/components/card.rs` — clippy fix (via `cargo clippy --fix`)
- `crates/application/src/use_cases/oauth_login_user.rs` — clippy fix (`collapsible_if`)
- `crates/infra/src/services/oidc_id_token_verifier.rs` — clippy fixes
- `HANDOVER.md` — updated (this file)
- `apps/dashboard/` — **DELETED** (entire Next.js frontend)

## Frontend Changes
- Public status API URLs corrected in Leptos to `/api/v1/public/status` and `/api/v1/public/status/{subdomain}`.
- No other frontend logic changed.

## Tests
Run from repository root:
- `cd apps/leptos-dashboard && trunk build` — ✅ PASS
- `cargo fmt --all` — ✅ PASS
- `cargo clippy --workspace --all-targets -- -D warnings` — ✅ PASS
- `cargo test --workspace` — ✅ PASS except `monitor_quota_race` which fails due to missing PostgreSQL credentials (pre-existing, unrelated to frontend).

## Deviations from Prompt
- L5 initial gap analysis was found to be incorrect; the listed missing features (status pages CRUD, billing, settings, help center, admin) do not exist in the Next.js frontend. Therefore, no additional Leptos work was required.
- We retained comments in `Caddyfile` that mention Node.js, as they are harmless and do not affect routing.

## Known Issues
- `monitor_quota_race` integration tests require a running PostgreSQL with valid credentials; without them, that single test fails, but all other tests pass.

## Notes for Next Worker
- This was the final frontend migration worker. The project is now 100% Rust/Leptos for frontend.
- Ensure `cargo install trunk` is run if trunk not installed.
- Backend APIs unchanged; all endpoints use `/api/v1` prefix.

## Completion Criteria
- [x] `apps/dashboard/` removed
- [x] No Node.js frontend dependencies remain
- [x] Leptos public status API URLs corrected
- [x] Clippy errors fixed
- [x] Build/lint/tests pass (except pre-existing DB test)
