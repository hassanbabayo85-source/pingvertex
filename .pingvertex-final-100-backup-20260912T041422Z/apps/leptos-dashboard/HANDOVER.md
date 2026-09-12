# HANDOVER — L5 Node.js Removal & Final Verification

## Worker
L5 — completed after verifying L1-L4 work.

## Scope
Remove Node.js frontend and fix URL prefix for public status API. Also fixed clippy warnings.

## Status
COMPLETE

## Files Changed
- `src/public_api.rs` — fixed `API_BASE` to `/api/v1/public`.
- `src/components/button.rs` — added `#[allow(dead_code)]`.
- `src/components/card.rs` — added `#[allow(dead_code)]`.

## Tests
Passed (see root HANDOVER for details).

## Notes
- Public status endpoints are now `/api/v1/public/status` and `/api/v1/public/status/{subdomain}`.

## Phase 1 — Backend ↔ Frontend Integration Hardening (2026-09-03)

### Status
SOURCE CHANGES COMPLETE; LOCAL RUNTIME VERIFICATION REQUIRED.

### Changes
- Dashboard DTO now mirrors backend `DashboardOverview` exactly.
- Dashboard uses one aggregated `/api/v1/dashboard/overview` request instead of overview + monitors.
- API client explicitly handles 409 Conflict and 429 Rate Limited and parses structured backend errors.
- 5xx details are sanitized before reaching UI; Retry-After is preserved when numeric.
- `RequireAuth` redirects only on a real 401 and keeps network/server failures retryable.
- Dashboard backend counts disabled monitors separately from up/down.

### Verification
Run `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `cd apps/leptos-dashboard && trunk build --release` locally.

