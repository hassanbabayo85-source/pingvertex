# TASK 4A — Resources without new entitlements (Part E)

Task 4A (Parts A–D) added three new tenant-scoped entitlements and
enforced them:

- `max_status_pages` (per-tenant, replaces the old hardcoded
  `MAX_STATUS_PAGES_PER_USER = 3`)
- `max_api_keys`
- `max_team_members` (owner counts as one)

This document covers the four resources Part E explicitly excludes
from new entitlements, and analyzes whether each is **truly bounded**
by its parent — because attachment to a parent does not automatically
bound the total count.

## Resources and their effective bounds

### 1. Custom domains (`status_pages.custom_domain`)

**Relationship:** one optional custom domain per status page, stored
as columns on the `status_pages` row itself (`custom_domain`,
`custom_domain_token`, `custom_domain_requested_at`,
`custom_domain_verified_at`).

**Bound:** truly bounded. A tenant's maximum custom-domain count is
exactly `max_status_pages` (per-tenant), because each status page can
hold at most one custom domain.

**Follow-up needed:** none.

### 2. Notification channels (`notification_channels.monitor_id`)

**Relationship:** each channel is attached to one monitor. A monitor
can have an arbitrary number of channels — there is **no per-monitor
cap** in the domain or the DB schema.

**Bound:** **NOT truly bounded**. Total channels per tenant =
`max_uptime_monitors × (unbounded channels per monitor)`. A Free tenant
with 20 monitors could create, in principle, an unbounded number of
channels and thereby cause an unbounded number of outbound requests on
every incident.

**Existing protection:** authorisation *is* enforced — 
`CreateNotificationChannelUseCase::execute` calls
`AuthorizationPolicy::same_tenant(...)` against the monitor, so a
tenant can only attach channels to its own monitors. But there is no
count cap.

**Follow-up needed:** **YES.** Add a per-monitor channel cap (or a
per-tenant channel cap) as a new entitlement in a follow-up task. This
is a defence-in-depth / abuse-prevention concern, not a billing
concern, so it can be added later without touching the pricing model.

### 3. Maintenance windows (`maintenance_windows.monitor_id`)

**Relationship:** each window is attached to one monitor. A monitor
can have an arbitrary number of windows.

**Bound:** **NOT truly bounded**, same reasoning as notification
channels.

**Existing protection:** authorisation enforced via
`AuthorizationPolicy::same_tenant` against the monitor in
`CreateMaintenanceWindowUseCase`.

**Follow-up needed:** **YES.** Add a per-monitor window cap (or
per-tenant cap). Same defence-in-depth reasoning.

### 4. Escalation policies (`escalation_policies`)

**Relationship:** a policy may have a `monitor_id` (per-monitor) or be
global (`monitor_id = NULL`, per-tenant). A tenant can create an
arbitrary number of both.

**Bound:** **NOT truly bounded**. Even if a per-monitor cap were added
for the scoped variant, the *global* variant has no parent resource to
bound it.

**Existing protection:** authorisation enforced for every referenced
monitor and channel via `AuthorizationPolicy::same_tenant` in
`CreateEscalationPolicyUseCase`.

**Follow-up needed:** **YES.** Add a per-tenant escalation-policy cap.

## Summary table

| Resource | Truly bounded by parent? | Authorisation enforced? | Follow-up task? |
|---|---|---|---|
| Custom domains | ✅ yes (`max_status_pages`) | ✅ yes | ❌ no |
| Notification channels | ❌ no (unbounded per monitor) | ✅ yes | ✅ yes |
| Maintenance windows | ❌ no (unbounded per monitor) | ✅ yes | ✅ yes |
| Escalation policies | ❌ no (unbounded per tenant, incl. global) | ✅ yes | ✅ yes |

## Recommended follow-up

A single follow-up task (not part of Task 4A) that introduces three
additional `plan_entitlements` keys:

- `max_notification_channels_per_monitor`
- `max_maintenance_windows_per_monitor`
- `max_escalation_policies_per_tenant`

with per-tier values derived from the existing v1 pricing sheet. The
schema, resolver, and use-case plumbing added in Task 4A already
supports this without further changes.

**This follow-up is NOT blocking** — the authorisation checks that are
already enforced prevent a tenant from using someone else's resources.
The concern is unbounded use of *its own* resources (abuse prevention
and cost containment on the notification/worker side).

## What is NOT changing in Task 4A

- Custom-domain, notification-channel, maintenance-window, and
  escalation-policy authorisation logic is **unchanged**.
- Any existing feature gates or limits on these resources are
  **preserved**.
- No customer resources are deleted, transferred, or migrated.

## Frontend (Part F) — deferred to Task 7

Task 4A asks the frontend to display "current usage, the plan limit,
and a useful explanation when a limit is reached." This requires:

1. A new backend endpoint (`GET /api/v1/entitlements`) exposing the
   current `Entitlements` snapshot plus current usage counts.
2. New UI sections on the usage page (`apps/leptos-dashboard/src/pages/usage.rs`)
   for status pages, API keys, and team members.

This work overlaps directly with Task 7 ("Frontend: Pricing, Billing,
Monitors and Proxy Monitoring"), which already includes a Usage page.
Adding it twice would create churn. **Decision: deferred to Task 7.**

Backend enforcement is authoritative — a limit is enforced by the API
regardless of whether the frontend displays it.
