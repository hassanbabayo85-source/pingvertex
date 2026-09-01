# HANDOVER — L5 Node.js Frontend Removal & Final Verification

## Worker
L5 — Node.js Frontend Removal (STOPPED — functionality gap detected)

## Scope
Verify that all Next.js dashboard functionality exists in Leptos before removing `apps/dashboard/`. This worker found missing sections and therefore did **NOT** delete any Node.js code.

## Status
**BLOCKED — DO NOT REMOVE NODE.JS YET**

## Files Created/Modified
- `apps/leptos-dashboard/HANDOVER.md` — updated with gap analysis and next steps

## Frontend Changes
None (no destructive actions performed).

## Gap Analysis: Next.js vs Leptos

### ✅ Confirmed available in Leptos (from L1-L4)
- Authentication (login, register, logout, forgot/reset password, 2FA flow) — L2
- Protected routes (`RequireAuth`) — L2
- Monitors list/detail, create/edit forms, enable/disable, delete — L3
- Monitor checks, incidents, notification channels (basic) — L3
- Dashboard overview (aggregated stats) — L3
- RUM dashboard: sites, overview, Core Web Vitals, slow pages, JS errors, browser/device stats, sessions, trends, date filters — L4

### ❌ Missing from Leptos (still only in Next.js)
- **Status Pages**: create/list/edit/delete status pages, subdomain management, custom domain verification, public status page rendering
- **Incident management** beyond monitor-specific: global incident list/acknowledge/resolve (partially in monitor detail, but not standalone)
- **Billing/Subscription**: plan selection, checkout, invoices, usage, add-ons, pricing config (admin)
- **Settings**:
  - Profile management (name, email, password change)
  - Team/Members management (invite, roles, remove)
  - API Keys management (create/revoke/list)
  - Notification preferences (user-level)
  - Two-factor authentication setup (enable/disable, backup codes)
- **Help Center / Support**:
  - Help categories/articles
  - Support ticket creation/replies
- **Admin pages** (if present in Next.js): pricing configuration, billing webhook status, user management

## Why L5 Stopped
Per global rule: *"L5 KADA ta cire Node.js frontend sai bayan an tabbatar DUK functionality ya wanzu a Leptos."* The above gaps are substantial; removing `apps/dashboard/` now would break production features.

## Next Steps for Completion
1. Create additional Leptos workers/pages to implement the missing sections listed above.
2. For each missing feature, mirror the Next.js behavior and use existing backend APIs (do not modify backend).
3. After all features are implemented and verified, re-run L5 to perform:
   - Final comparison checklist
   - Removal of `apps/dashboard/`, related CI/Docker/deployment references
   - Full build & test verification

## Tests (this worker)
Not run (no code changes made). Previous L1-L4 builds/tests passed except database-dependent `monitor_quota_race` due to missing PostgreSQL credentials (unrelated to frontend).

## Deviations from Prompt
- L5 intentionally stopped before removing Node.js due to missing functionality. This is a conservative choice per Judgment Rule.

## Known Issues
- Missing Leptos pages for status pages, billing, settings, help center, support, and admin sections.
- `monitor_quota_race` tests fail due to PostgreSQL auth config (not frontend-related).

## Notes for Next Worker
- Do not delete `apps/dashboard/` until the gap list above is fully resolved.
- Verify all backend API endpoints for the missing features exist (they likely do from B/H/RUM tracks).
- Keep security controls (HttpOnly cookies, CSRF, etc.) intact in new Leptos pages.

## Completion Criteria for L5 (when re-run)
- All Next.js functionality replicated in Leptos.
- `apps/dashboard/` removed along with all CI/CD/Docker references.
- `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `wasm-pack build --target web` all pass.
