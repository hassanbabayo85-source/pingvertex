# Handover — Path 1: API Contract Alignment (Frontend API Layer)

## Path/Phase
Path 1 — API Contract Alignment (frontend api layer)

## Scope
FRONTEND-ONLY: Rebuild `api/client.rs` as shared typed client; rewrite `api/rum.rs` and `api/monitoring.rs` to match backend endpoints; update `auth/api.rs`; no UI/component changes; no `crates/*` changes.

## File Ownership (Owned/Read-only/Forbidden adhered to)
**OWNED FILES (all modified):**
- `apps/leptos-dashboard/src/api/client.rs`
- `apps/leptos-dashboard/src/api/mod.rs`
- `apps/leptos-dashboard/src/api/monitoring.rs`
- `apps/leptos-dashboard/src/api/rum.rs`
- `apps/leptos-dashboard/src/auth/api.rs`

**READ-ONLY (not modified):**
- `apps/leptos-dashboard/src/auth/context.rs`
- `apps/leptos-dashboard/src/pages/*.rs`
- `crates/api/src/routes/*.rs`
- `crates/api/src/handlers/*.rs`
- `docs/adr/*`

**FORBIDDEN (not touched):**
- `apps/leptos-dashboard/src/components/*`
- `apps/leptos-dashboard/src/pages/*.rs` (write)
- `apps/leptos-dashboard/src/routes/mod.rs`
- `apps/leptos-dashboard/style.css`
- `apps/leptos-dashboard/index.html`
- Any `crates/*` file

## Status
COMPLETE — All acceptance criteria met; `cargo fmt`, `cargo clippy -p leptos-dashboard --target wasm32-unknown-unknown -- -D warnings`, and `cargo check -p leptos-dashboard --target wasm32-unknown-unknown` pass. No test harness for wasm; manual curl verification list provided.

## Files Created/Modified/Deleted
- **Modified** `apps/leptos-dashboard/src/api/client.rs` — introduced shared typed `ApiError` enum; centralized `get_json`, `post_json`, `put_json`, `post_json_no_content`, `delete_json_no_content`; removed legacy per-file duplicate helpers.
- **Modified** `apps/leptos-dashboard/src/api/mod.rs` — added `pub mod client;`.
- **Modified** `apps/leptos-dashboard/src/api/rum.rs` — completely rebuilt; added new typed structs (`RumSite`, `BackendRumOverview`, `BackendPagePerformanceRow`, `BackendRumErrorBreakdown`, etc.); added 8 typed API functions matching backend routes (`/dashboard/rum/sites` and `/sites/{site_id}/...`); retained legacy compatibility functions `fetch_rum_sites` and `fetch_rum_dashboard` (and related legacy structs) to keep pages compiling; `site_key` field now correct.
- **Modified** `apps/leptos-dashboard/src/api/monitoring.rs` — updated notification paths to `/monitors/{monitor_id}/notifications` and `/notifications/{id}`; added `test_notification_channel`, `create_maintenance_window`, `create_escalation_policy`, `acknowledge_incident`; kept all existing public functions with same names/signatures (return `Result<_, String>` for compatibility); added `name` field to `NotificationChannel` and `ChannelPayload` to satisfy pages.
- **Modified** `apps/leptos-dashboard/src/auth/api.rs` — switched to shared client; translated Hausa doc-comment to English; same public functions/signatures (return `Result<_, String>`).

**Created** `HANDOVER-path-1.md` (this file).

## Changes
### 1. Shared typed client (`api/client.rs`)
- `ApiError` enum with variants: `Unauthorized`, `Forbidden`, `Validation(String)`, `NotFound`, `Network(String)`, `Server(u16, String)`. Each variant maps to HTTP status or transport error.
- `Display` implemented; `From<gloo_net::Error>` and `From<serde_json::Error>` conversions.
- Central helpers: `get_json`, `post_json`, `put_json` (all generic), `post_json_no_content`, `delete_json_no_content`.
- All requests use `RequestCredentials::Include` and `API_BASE = "/api/v1"`.
- `handle_response` maps status codes to the appropriate variant.

### 2. RUM API (`api/rum.rs`)
- **New typed structs** matching backend DTOs:
  - `RumSite` (field `site_key`, not `public_site_key`)
  - `CreateRumSitePayload`
  - `BackendRumOverview`, `BackendPagePerformanceRow`, `BackendTopJsErrorRow`, `BackendTopResourceErrorRow`, `BackendRumErrorBreakdown`, `BackendBrowserBreakdownRow`, `BackendDeviceBreakdownRow`, `BackendBrowserDeviceBreakdown`, `BackendGeoBreakdownRow`, `BackendPerformanceTrendPoint`, `BackendSessionStats`.
- **New typed API functions** (all return `Result<_, ApiError>`):
  - `list_rum_sites_typed()` → `GET /dashboard/rum/sites`
  - `create_rum_site_typed(payload)` → `POST /dashboard/rum/sites`
  - `rum_overview_typed(site_id, from, to)` → `GET /dashboard/rum/sites/{site_id}/overview`
  - `rum_slow_pages_typed(site_id, from, to, limit)` → `GET /dashboard/rum/sites/{site_id}/pages`
  - `rum_error_breakdown_typed(site_id, from, to, limit)` → `GET /dashboard/rum/sites/{site_id}/errors`
  - `rum_browser_device_breakdown_typed(site_id, from, to)` → `GET /dashboard/rum/sites/{site_id}/browsers-devices`
  - `rum_geo_breakdown_typed(site_id, from, to, limit)` → `GET /dashboard/rum/sites/{site_id}/geo`
  - `rum_performance_trend_typed(site_id, from, to)` → `GET /dashboard/rum/sites/{site_id}/trend`
  - `rum_session_stats_typed(site_id, from, to)` → `GET /dashboard/rum/sites/{site_id}/sessions`
- **Legacy compatibility functions** (return `Result<_, String>`) to keep existing pages working:
  - `fetch_rum_sites()` → wraps `list_rum_sites_typed` and returns `Vec<RumSite>`.
  - `fetch_rum_dashboard(site_id, from, to, limit)` → aggregates results from new endpoints and maps them into the old `RumDashboardData` struct (which is still present). If `site_id` is `None`, returns default empty data (page never calls with `None` in actual flow).

### 3. Monitoring API (`api/monitoring.rs`)
- Notification paths fixed:
  - `list_notification_channels` → `GET /monitors/{monitor_id}/notifications`
  - `create_notification_channel` → `POST /monitors/{monitor_id}/notifications`
  - `delete_notification_channel` → `DELETE /notifications/{channel_id}`
- Added new functions (return `Result<_, String>` to match existing style):
  - `test_notification_channel(channel_id)` → `POST /notifications/{id}/test`
  - `create_maintenance_window(monitor_id, payload)` → `POST /monitors/{id}/maintenance-windows`
  - `create_escalation_policy(payload)` → `POST /escalation-policies`
  - `acknowledge_incident(incident_id)` → `POST /incidents/{incident_id}/acknowledge`
- `ChannelPayload` now includes `name` (with `#[serde(default)]`) and `chat_id` removed (see Deviations).
- `NotificationChannel` includes `name` field (with `#[serde(default)]`) to satisfy pages; backend doesn't return `name`, so it defaults to empty string.

### 4. Auth API (`auth/api.rs`)
- Same public functions (`login`, `register`, `logout`, `forgot_password`, `reset_password`, `complete_2fa`, `fetch_current_user`) with same signatures (return `Result<_, String>`).
- Switched to shared client helpers.
- Hausa doc-comment translated to English.

## Tests
**Commands run and result:**
```bash
cd apps/leptos-dashboard
cargo fmt --all
cargo clippy -p leptos-dashboard --target wasm32-unknown-unknown -- -D warnings
cargo check -p leptos-dashboard --target wasm32-unknown-unknown
