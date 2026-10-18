# PingVertex — Frontend ↔ Backend Contract Audit (2026-09-03)

## Source-of-truth trace

`dashboard_overview.rs → api/monitoring.rs → /api/v1/dashboard/overview → crates/api/src/routes/dashboard.rs → GetDashboardOverviewUseCase → DashboardRepository → PostgresDashboardRepository → SQL/migrations → JSON → frontend`

## DashboardOverview result

| Backend | Frontend | Result |
|---|---|---|
| `monitor_summary.total/up/down/disabled` | same | ALIGNED |
| `monitors[]` | same | ALIGNED |
| `incident_summary.open_count` | same | ALIGNED |
| `incident_summary.recent[]` | same | ALIGNED |

The old frontend summary (`total_monitors`, `uptime_percentage`, `average_response_time_ms`, `active_incidents`) was removed. The dashboard now performs one aggregated request and consumes the backend aggregation directly.

## Security / performance findings applied

- Dashboard route is JWT-protected in `crates/api/src/router.rs`.
- Repository scopes monitor data by `TenantId` and incident data through tenant-owned monitors.
- LATERAL joins provide latest check/open incident without a client-side N+1 loop.
- Existing indexes cover tenant monitor lookup and monitor/check/incident lookups.
- Disabled monitors no longer inflate `up`/`down`; they are counted only in `disabled`.
- Frontend 5xx messages are sanitized.
- Frontend explicitly maps 409 and 429; numeric `Retry-After` is retained.
- Structured `{message}` API errors are parsed.
- Authentication redirects happen only on an actual 401; network/server failures remain retryable.
- No unsafe automatic mutation retries were introduced.

## Runtime verification still required

This source snapshot cannot run against the user's local PostgreSQL/Redis/browser environment. After installation run:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd apps/leptos-dashboard && trunk build --release
cd ../..
```

## Frontend API inventory

- `advanced_monitoring.rs::list_maintenance` → /escalation-po, /maintenance-windows/, /monitors/
- `advanced_monitoring.rs::create_maintenance` → /escalation-policies, /maintenance-windows/, /monitors/
- `advanced_monitoring.rs::delete_maintenance` → /escalation-policies, /escalation-policies/, /maintenance-windows/, /monitors/
- `advanced_monitoring.rs::list_escalations` → /escalation-policies, /escalation-policies/, /maintenance-windows/, /monitors/
- `advanced_monitoring.rs::create_escalation` → /escalation-policies, /escalation-policies/, /maintenance-windows/
- `advanced_monitoring.rs::update_escalation` → /escalation-policies, /escalation-policies/
- `advanced_monitoring.rs::delete_escalation` → /escalation-policies, /escalation-policies/
- `billing.rs::summary` → /billing/addons, /billing/checkout, /billing/checkout/verify, /billing/summary, /pricing/quote
- `billing.rs::quote` → /billing/addons, /billing/checkout, /billing/checkout/verify, /billing/summary, /pricing/quote
- `billing.rs::checkout` → /billing/addons, /billing/checkout, /billing/checkout/verify, /billing/summary, /pricing/quote
- `billing.rs::verify_checkout` → /billing/addons, /billing/checkout, /billing/checkout/verify, /billing/summary, /pricing/quote
- `billing.rs::add_on` → /billing/addons, /billing/checkout, /billing/checkout/verify
- `developer.rs::list_keys` → /api-keys, /api-keys/, /monitors/
- `developer.rs::create_key` → /api-keys, /api-keys/, /monitors/
- `developer.rs::delete_key` → /api-keys, /api-keys/, /monitors/
- `developer.rs::list_flows` → /api-keys, /api-keys/, /monitors/
- `developer.rs::create_flow` → /api-keys, /api-keys/, /monitors/
- `developer.rs::update_flow` → /api-keys/, /monitors/
- `developer.rs::delete_flow` → /monitors/
- `monitoring.rs::fetch_monitors` → /monitors, /monitors/
- `monitoring.rs::get_monitor` → /monitors, /monitors/
- `monitoring.rs::create_monitor` → /das, /monitors, /monitors/
- `monitoring.rs::update_monitor` → /dashboard/overview, /monitors, /monitors/
- `monitoring.rs::delete_monitor` → /dashboard/overview, /monitors, /monitors/
- `monitoring.rs::set_monitor_enabled` → /dashboard/overview, /monitors, /monitors/
- `monitoring.rs::fetch_overview` → /dashboard/overview, /monitors/
- `monitoring.rs::fetch_incidents` → /dashboard/overview, /monitors/
- `monitoring.rs::fetch_checks` → /dashboard/overview, /monitors/, /notifications/
- `monitoring.rs::list_notification_channels` → /dashboard/overview, /monitors/, /notifications/
- `monitoring.rs::create_notification_channel` → /monitors/, /notifications/
- `monitoring.rs::delete_notification_channel` → /escalation-policies, /monitors/, /notifications/
- `monitoring.rs::test_notification_channel` → /escalation-policies, /incidents/, /monitors/, /notifications/
- `monitoring.rs::create_maintenance_window` → /escalation-policies, /incidents/, /monitors/, /notifications/
- `monitoring.rs::create_escalation_policy` → /escalation-policies, /incidents/, /monitors/, /notifications/
- `monitoring.rs::acknowledge_incident` → /escalation-policies, /incidents/, /monitors/
- `rum.rs::list_rum_sites_typed` → /dashboard/rum/sites, /dashboard/rum/sites/
- `rum.rs::create_rum_site_typed` → /dashboard/rum/sites, /dashboard/rum/sites/
- `rum.rs::rum_overview_typed` → /dashboard/rum/sites, /dashboard/rum/sites/
- `rum.rs::rum_slow_pages_typed` → /dashboard/rum/sites/
- `rum.rs::rum_error_breakdown_typed` → /dashboard/rum/sites/
- `rum.rs::rum_browser_device_breakdown_typed` → /dashboard/rum/sites/
- `rum.rs::rum_geo_breakdown_typed` → /dashboard/rum/sites/
- `rum.rs::rum_performance_trend_typed` → /dashboard/rum/sites/
- `rum.rs::rum_session_stats_typed` → /dashboard/rum/sites/
- `status_pages.rs::list` → /st, /status-pages, /status-pages/
- `status_pages.rs::create` → /status-pages, /status-pages/
- `status_pages.rs::update` → /status-pages, /status-pages/
- `status_pages.rs::delete` → /monitors, /status-pages, /status-pages/
- `status_pages.rs::request_custom_domain` → /monitors, /status-pages, /status-pages/
- `status_pages.rs::remove_custom_domain` → /monitors, /status-pages/
- `status_pages.rs::upload_logo` → /monitors, /status-pages/
- `status_pages.rs::monitors` → /monitors, /status-pages/
- `support.rs::categories` → /help/articles, /help/articles/, /help/categories, /help/search, /support/tickets, /support/tickets/
- `support.rs::articles` → /help/articles, /help/articles/, /help/categories, /help/search, /support/tickets, /support/tickets/
- `support.rs::article` → /help/articles, /help/articles/, /help/categories, /help/search, /support/tickets, /support/tickets/
- `support.rs::search` → /help/articles, /help/articles/, /help/search, /support/tickets, /support/tickets/
- `support.rs::tickets` → /help/articles, /help/articles/, /help/search, /support/tickets, /support/tickets/
- `support.rs::ticket` → /help/articles, /help/articles/, /help/search, /support/tickets, /support/tickets/
- `support.rs::create_ticket` → /help/articles/, /help/search, /support/tickets, /support/tickets/
- `support.rs::reply` → /help/search, /support/tickets, /support/tickets/
- `support.rs::set_status` → /support/tickets, /support/tickets/
- `team.rs::list_members` → /invitations, /invitations/pending, /members, /members/
- `team.rs::add_member` → /invitations, /invitations/, /invitations/pending, /members, /members/
- `team.rs::change_role` → /invitations, /invitations/, /invitations/accept, /invitations/pending, /members, /members/
- `team.rs::remove_member` → /invitations, /invitations/, /invitations/accept, /invitations/pending, /members, /members/
- `team.rs::list_invitations` → /invitations, /invitations/, /invitations/accept, /invitations/pending, /members, /members/
- `team.rs::create_invitation` → /invitations, /invitations/, /invitations/accept, /invitations/pending, /members/
- `team.rs::revoke_invitation` → /invitations, /invitations/, /invitations/accept, /invitations/pending, /members/
- `team.rs::accept_invitation` → /invitations, /invitations/, /invitations/accept, /invitations/pending, /members/

## Backend route inventory

- `api_flows.rs` → `/monitors/{id}/flows`
- `api_flows.rs` → `/monitors/{id}/flows/{flow_id}`
- `api_keys.rs` → `/api-keys`
- `api_keys.rs` → `/api-keys/{id}`
- `api_keys.rs` → `/external/monitors`
- `api_keys.rs` → `/external/checks`
- `api_keys.rs` → `/external/incidents/{id}`
- `api_keys.rs` → `/external/incidents/{incident_id}/acknowledge`
- `api_keys.rs` → `/external/status-pages`
- `api_keys.rs` → `/external/status-pages/{id}`
- `auth.rs` → `/register`
- `auth.rs` → `/login`
- `auth.rs` → `/oauth/google`
- `auth.rs` → `/oauth/apple`
- `auth.rs` → `/verify-email`
- `auth.rs` → `/resend-verification`
- `auth.rs` → `/forgot-password`
- `auth.rs` → `/reset-password`
- `auth.rs` → `/totp/verify`
- `auth.rs` → `/logout`
- `auth.rs` → `/me`
- `auth.rs` → `/totp/enable`
- `auth.rs` → `/totp/confirm`
- `auth.rs` → `/totp/disable`
- `auth.rs` → `/password/change`
- `billing.rs` → `/billing/checkout`
- `billing.rs` → `/billing/checkout/verify`
- `billing.rs` → `/billing/addons`
- `billing.rs` → `/billing/summary`
- `billing.rs` → `/billing/webhook`
- `checks.rs` → `/checks`
- `dashboard.rs` → `/dashboard/overview`
- `health.rs` → `/health`
- `health.rs` → `/ready`
- `help.rs` → `/help/categories`
- `help.rs` → `/help/articles`
- `help.rs` → `/help/articles/{slug}`
- `help.rs` → `/help/search`
- `internal_agents.rs` → `/agents/{agent_id}/checks/claim`
- `internal_agents.rs` → `/agents/{agent_id}/results`
- `invitations.rs` → `/invitations`
- `invitations.rs` → `/invitations/pending`
- `invitations.rs` → `/invitations/accept`
- `invitations.rs` → `/invitations/{id}`
- `members.rs` → `/members`
- `members.rs` → `/members/{user_id}`
- `members.rs` → `/members/{user_id}/role`
- `monitors.rs` → `/monitors`
- `monitors.rs` → `/monitors/{id}`
- `monitors.rs` → `/monitors/{id}/enable`
- `monitors.rs` → `/monitors/{id}/disable`
- `monitors.rs` → `/monitors/{id}/checks`
- `monitors.rs` → `/monitors/{id}/incidents`
- `monitors.rs` → `/monitors/{id}/maintenance-windows`
- `monitors.rs` → `/escalation-policies`
- `monitors.rs` → `/escalation-policies/{id}`
- `monitors.rs` → `/incidents/{incident_id}/acknowledge`
- `notifications.rs` → `/monitors/{monitor_id}/notifications`
- `notifications.rs` → `/notifications/{id}`
- `notifications.rs` → `/notifications/{id}/test`
- `pricing.rs` → `/pricing/quote`
- `pricing.rs` → `/pricing/configs/active`
- `pricing.rs` → `/pricing/configs`
- `pricing.rs` → `/pricing/configs/{version}`
- `pricing.rs` → `/pricing/audit-logs`
- `proxy_checks.rs` → `/proxy-checks`
- `proxy_checks.rs` → `/proxy-checks/{id}`
- `public.rs` → `/status`
- `public.rs` → `/status/{subdomain}`
- `public.rs` → `/heartbeat/{token}`
- `public.rs` → `/rum/collect`
- `rum_dashboard.rs` → `/dashboard/rum/sites`
- `rum_dashboard.rs` → `/dashboard/rum/sites/{site_id}/overview`
- `rum_dashboard.rs` → `/dashboard/rum/sites/{site_id}/pages`
- `rum_dashboard.rs` → `/dashboard/rum/sites/{site_id}/errors`
- `rum_dashboard.rs` → `/dashboard/rum/sites/{site_id}/browsers-devices`
- `rum_dashboard.rs` → `/dashboard/rum/sites/{site_id}/geo`
- `rum_dashboard.rs` → `/dashboard/rum/sites/{site_id}/trend`
- `rum_dashboard.rs` → `/dashboard/rum/sites/{site_id}/sessions`
- `status_pages.rs` → `/status-pages`
- `status_pages.rs` → `/status-pages/{id}`
- `status_pages.rs` → `/status-pages/{id}/logo`
- `status_pages.rs` → `/status-pages/{id}/custom-domain`
- `subscription.rs` → `/subscription/usage`
- `support.rs` → `/support/tickets`
- `support.rs` → `/support/tickets/{id}`
- `support.rs` → `/support/tickets/{id}/messages`
- `support.rs` → `/support/tickets/{id}/status`

