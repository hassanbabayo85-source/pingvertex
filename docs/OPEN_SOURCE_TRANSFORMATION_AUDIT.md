# PingVertex — Open-Source Transformation Audit

**Status:** Phase Zero complete — awaiting sign-off before code modification.
**Repo:** https://github.com/hassanbabayo85-source/pingvertex
**Branch of record:** `main` @ `7431ed4`
**WIP preserved on:** `wip/pre-open-source-20261107`
**Audit date:** 2026-11-07
**Auditor:** AI assistant (with terminal-verified evidence only — no paths guessed)

---

## 1. Executive Summary

PingVertex is a Rust/Axum/Leptos/PostgreSQL/SQLx uptime monitoring platform that
currently ships a full commercial billing system (Paystack subscriptions, wallet
PAYG, checkout sessions, invoices, add-ons, financial audit ledger, entitlement
enforcement baked into core monitoring use cases).

The transformation goal is to become a **self-hostable, community-driven
open-source platform with no payment gateway, no wallet, no subscription, no
invoice, no checkout, and no commercial pricing**. Administrators configure
resource limits (max monitors, min interval, max agents, retention, etc.)
directly. Anyone wishing to run PingVertex as a SaaS re-implements billing
themselves as a downstream extension.

**This audit is the gate for all deletion / modification.** No source file has
been touched.

---

## 2. Current Repository Shape

- **Cargo workspace:** single root `Cargo.toml` with members:
  `crates/{domain,application,infra,api,worker,shared,agent}` and
  `apps/leptos-dashboard`.
- **Migrations:** `migrations/` — **131** SQL files (0001..0131 on `main`;
  0132 exists only on the WIP branch).
- **Backup tree on disk:** `.project-cleanup-backup-20260916-122122/` — 3.8 MB,
  contains historical `migrations/`, `crates/domain/Cargo.toml`, `docs/`, etc.
  **Not present in git index** — it is a local-only backup. Must be excluded
  from the published repo (§16) but can be kept locally until Phase 14.
- **Secrets posture:** confirmed `NOT` tracked in git:
  `.env`, `.env.backup.*`, `.metrics_token`, `cookies*.txt`, `certs/`, `secrets/`.
  Only `.env.example` is tracked. **No git-history secret leak detected.**
- **CI:** `.github/workflows/{ci.yml,leptos-frontend.yml}`, `dependabot.yml`,
  `CODEOWNERS`, `PULL_REQUEST_TEMPLATE.md`, issue templates.
- **Known tooling issue:** `find . -name '*.rs' | xargs wc -l` and
  `git ls-files '*.rs' | xargs wc -l` both abort with `Input/output error` on
  one or more inodes. LOC totals are therefore unmeasured. Non-blocking for the
  transformation; to be diagnosed in Phase 11 (Testing & Quality Gates).

---

## 3. Billing Footprint — Keyword Density (main tree, `*.rs`)

| Keyword | Files mentioning |
|---|---|
| `billing` | 154 |
| `subscription` | 115 |
| `wallet` | 99 |
| `entitlement` | 78 |
| `checkout` | 70 |
| `payment` | 67 |
| `payg` | 63 |
| `invoice` | 28 |
| `paystack` | 19 |
| `ledger` | 9 |

High counts do **not** imply high deletion scope — most files are *mixed*
(monitoring core that merely calls an entitlement check). Removal must be
surgical.

---

## 4. File Classification

### 4.1 Billing-only — SAFE TO DELETE (main tree)

#### API layer
- `crates/api/src/handlers/add_on_billing.rs`
- `crates/api/src/handlers/admin_pricing.rs`
- `crates/api/src/handlers/billing_allowance.rs`
- `crates/api/src/handlers/billing_finalization.rs`
- `crates/api/src/handlers/billing_types.rs`
- `crates/api/src/handlers/billing_webhook.rs`
- `crates/api/src/handlers/invoices.rs`
- `crates/api/src/handlers/payg_preview.rs`
- `crates/api/src/handlers/payg_pricing.rs`
- `crates/api/src/handlers/payg_retired.rs`
- `crates/api/src/handlers/paystack_webhook.rs`
- `crates/api/src/handlers/subscription_billing.rs`
- `crates/api/src/handlers/update_subscription_usage.rs`
- `crates/api/src/handlers/wallet.rs`
- `crates/api/src/routes/admin.rs` (admin pricing sub-router; keep other admin handlers)
- `crates/api/src/routes/billing.rs`
- `crates/api/src/routes/pricing.rs`
- `crates/api/src/routes/subscription.rs`
- `crates/api/src/routes/wallet.rs`

#### Application layer
- `crates/application/src/billing_authority.rs`
- `crates/application/src/commands/{begin_grace_period,cancel_subscription,expire_subscription,finalize_scheduled_cancellation,initiate_checkout,verify_checkout}.rs`
- `crates/application/src/ports/{billing_audit_log_repository,checkout_session_repository,financial_audit_log_repository,invoice_repository,payg_pricing_repository,payment_provider,payment_repository,plan_price_repository,subscription_activation_repository,subscription_billing_record_repository,subscription_repository,subscription_usage_period_repository,subscription_usage_transaction,wallet_repository}.rs`
- `crates/application/src/use_cases/{add_on_subscription,begin_grace_period,billing_support,billing_webhook_support,calculate_payg_preview,calculate_subscription_cycle_cost,cancel_subscription,change_subscription_plan,charge_for_usage,expire_subscription,finalize_scheduled_cancellation,generate_invoice,initiate_checkout,initiate_subscription_checkout,meter_usage,process_billing_webhook,subscription_lifecycle_support,update_invoice_status,update_subscription_usage,verify_addon_checkout,verify_checkout,verify_subscription_checkout}.rs`
- `crates/application/src/errors/plan_limit.rs` (replaced by admin-quota error in Phase 2)

#### Domain layer
- `crates/domain/src/billing_period.rs`
- `crates/domain/src/entities/{add_on_subscription,billing_audit_log_entry,billing_webhook_event,checkout_session,financial_audit_log,invoice,payg,plan_price,subscription_plan_change,subscription,subscription_usage_period,wallet}.rs`
- `crates/domain/src/enums/{billing_webhook_event_type,checkout_status,subscription_status}.rs`
- `crates/domain/src/payg.rs`
- `crates/domain/src/pricing/calculation.rs`
- `crates/domain/src/proration.rs`

#### Infrastructure layer
- `crates/infra/src/billing_config.rs`
- `crates/infra/src/repositories/{in_memory_billing_webhook_idempotency_repository,in_memory_checkout_session_repository,in_memory_invoice_repository,in_memory_plan_price_repository,in_memory_subscription_activation_repository,in_memory_subscription_billing_record_repository,in_memory_subscription_repository,in_memory_subscription_usage_period_repository,in_memory_subscription_usage_transaction,in_memory_wallet_repository}.rs`
- `crates/infra/src/repositories/{postgres_add_on_subscription_repository,postgres_billing_audit_log_repository,postgres_billing_webhook_idempotency_repository,postgres_checkout_session_repository,postgres_invoice_repository,postgres_payg_pricing_repository,postgres_payment_repository,postgres_plan_price_repository,postgres_subscription_activation_repository,postgres_subscription_billing_record_repository,postgres_subscription_plan_change_repository,postgres_subscription_repository,postgres_subscription_usage_period_repository,postgres_subscription_usage_transaction,postgres_wallet_repository}.rs`
- `crates/infra/src/services/{disabled_billing_provider,fake_billing_provider,paystack_billing_provider}.rs`

#### Worker layer
- `crates/worker/src/reconciliation_worker.rs`
- `crates/worker/src/subscription_expiry_worker.rs` (WIP branch only — will not appear on main)

#### Frontend
- `apps/leptos-dashboard/src/api/{billing,wallet}.rs`
- `apps/leptos-dashboard/src/pages/{add_ons,billing,invoices,pricing,transactions,usage,wallet}.rs`

### 4.2 Mixed files — SURGICAL EDIT (remove billing branches, keep core)

| File | What to remove | What to keep |
|---|---|---|
| `crates/api/src/handlers/create_monitor.rs` | entitlement / plan-limit check | admin-quota check |
| `crates/api/src/handlers/bulk_monitor_{action,import_confirm,import_preview}.rs` | PAYG gating | bulk logic + admin-quota |
| `crates/api/src/handlers/{create_api_key,create_invitation,add_member,accept_invitation,create_status_page}.rs` | plan-based entitlement | tenant membership + admin-quota |
| `crates/api/src/handlers/one_time_check.rs` | PAYG pricing / wallet debit | the check itself |
| `crates/api/src/handlers/{get_monitor,list_monitors,update_monitor}.rs` | plan-limit hooks | monitor CRUD |
| `crates/api/src/handlers/{platform_admin,recurring_proxy_monitors,rum_ingest}.rs` | billing refs | their primary behaviour |
| `crates/api/src/{auth_state,error,main,router,state}.rs` | billing router wiring, billing error variants | router core, auth |
| `crates/api/src/routes/internal_agents.rs` | billing touchpoints | agent routes |
| `crates/worker/src/{config,durable_scheduler,lib,main}.rs` | billing config, entitlement gate | scheduler core |
| `crates/worker/src/{bulk_monitor_action_worker,bulk_monitor_import_worker,proxy_check_worker,proxy_checker,retention_worker}.rs` | billing gating | worker logic |
| `crates/application/src/use_cases/{create_monitor,create_status_page,create_api_key,create_invitation,add_member,accept_invitation,update_monitor,preview_bulk_monitor_import,create_recurring_proxy_monitor,update_recurring_proxy_monitor,get_recurring_proxy_monitor,list_recurring_proxy_monitors,verify_totp_and_login}.rs` | billing calls | core use case |
| `crates/application/src/queries/resolve_entitlements.rs` | commercial entitlement | **rename** to `resolve_resource_limits.rs`, keep admin-config-based lookup |
| `crates/application/src/ports/{agent_lease_repository,id_generator,monitor_repository,notification_channel_repository,notification_service,recurring_proxy_monitor_repository,scheduling_repository,tenant_membership_repository}.rs` | billing trait methods | non-billing methods |
| `crates/application/src/{errors/mod,lib}.rs`, `{commands,dto,ports,queries,use_cases}/mod.rs` | billing module exports | remaining exports |
| `crates/domain/src/entities/{monitor,one_time_check,recurring_proxy_monitor,tenant,mod}.rs` | billing fields / imports | core entity fields |
| `crates/domain/src/entities/plan.rs` | commercial plan model — replace with admin-config resource policy | — |
| `crates/domain/src/entities/plan_entitlements.rs` | commercial entitlements — replace with resource-limit type | — |
| `crates/domain/src/entitlements.rs` | commercial entitlement — replace with `resource_limits.rs` | — |
| `crates/domain/src/enums/plan_tier.rs` | commercial tier — remove | — |
| `crates/domain/src/money.rs` | keep if used outside billing; remove if unused | (verify) |
| `crates/domain/src/entities/{tenant_retention_settings,tenant_alert_allowance}.rs` | (already admin-config — verify no commercial coupling) | **KEEP** |
| `crates/domain/src/{lib,enums,errors/domain_error}.rs`, `shared/ids/mod.rs`, `shared/types/role.rs`, `value_objects/subdomain.rs` | billing symbols | core |
| `crates/infra/src/lib.rs`, `repositories/mod.rs`, `services/mod.rs` | billing module wiring | other wiring |
| `crates/infra/src/repositories/{in_memory_monitor_repository,in_memory_recurring_proxy_monitor_repository,postgres_agent_lease_repository,postgres_monitor_repository,postgres_recurring_proxy_monitor_repository,postgres_scheduling_repository,postgres_dashboard_repository}.rs` | billing queries / joins | core |
| `crates/infra/src/repositories/{in_memory_entitlement_resolver,postgres_entitlement_resolver}.rs` | commercial entitlement → rename to `*_resource_limit_resolver` | resolver shape |
| `crates/infra/src/services/{notification_dispatcher,smtp_email_notification_service}.rs` | billing templates / hooks | notification core |
| `apps/leptos-dashboard/src/api/{mod,monitoring}.rs` | billing api clients export | monitoring clients |
| `apps/leptos-dashboard/src/components/{layout,navigation}.rs` | billing nav links | layout |
| `apps/leptos-dashboard/src/pages/{audit,bulk_import,monitor_form,monitors,platform_admin,support,mod}.rs` | billing UI | page core |
| `apps/leptos-dashboard/src/routes/mod.rs` | 6 billing routes | all other routes |

### 4.3 Billing-only tests — DELETE (verify each is genuinely billing-only first)

44 test files matched billing keywords across `crates/api/tests/`,
`crates/application/tests/`, `crates/infra/tests/`. They will be enumerated
during Phase 1K and triaged one by one. **No test file will be deleted solely
because its name contains "billing"** — each will be inspected for non-billing
coverage it may also contain.

### 4.4 Migrations — DO NOT DELETE HISTORICAL FILES

Per master prompt §4.7: forward migrations only. Classification:

- **Billing-only (to be dropped by a new forward migration):**
  0038, 0039, 0040, 0041, 0042, 0049, 0052, 0053, 0056, 0074, 0078, 0079, 0080,
  0081, 0082, 0083, 0084, 0085, 0087, 0088, 0090, 0091, 0093, 0094, 0095,
  0096, 0097, 0098, 0099, 0100, 0101, 0103, 0104, 0105, 0106, 0107, 0109,
  0110, 0111, 0112, 0114, 0115, 0119, 0121, 0124, 0125, 0129, 0131, 0132(WIP)

- **Mixed — retain, inspect for billing additions to be undone by a new
  forward migration:**
  0059 (help centre seed — mentions billing category, retain),
  0073 (bootstrap_business_model — treat as billing),
  0086 (OAuth + test access — MIXED),
  0113 (restore_proxy_monitoring — MIXED),
  0116 (tenant_alert_allowances — admin-config, KEEP),
  0117 (retention_settings — MIXED with retention, KEEP core),
  0118 (RUM + payg — KEEP RUM core),
  0120 (least_privilege_roles — MIXED),
  0123 (pagination_indexes — MIXED),
  0126 (recurring_proxy_monitor_leases — KEEP),
  0127 (tenant_resource_entitlements — MIXED with resource policy),
  0128 (monitor_entitlement_suspension — MIXED with monitoring),
  0130 (retention_entitlement — MIXED with retention).

- **New forward migration(s) to be authored (not yet):**
  - `013x_drop_commercial_billing.sql` — drop billing tables, columns, indexes.
  - `013x_replace_entitlement_with_resource_limits.sql` — rename / re-key the
    entitlement tables to admin-config resource limits.

### 4.5 Config / deployment

- `docker-compose.prod.yml` — remove `BILLING_PROVIDER`,
  `BILLING_SECRET_KEY`, `BILLING_WEBHOOK_SECRET` from both `api` and `worker`.
- `docker-compose.e2e.yml` — remove `BILLING_PROVIDER=fake`,
  `BILLING_WEBHOOK_SECRET=…` and the "fake billing" comment.
- `.env.e2e.example` — remove `BILLING_PROVIDER`, `BILLING_WEBHOOK_SECRET`.
- `.env.example` — **already clean** (no BILLING vars). ✅
- `Cargo.toml` — **no billing deps detected** in any crate ✅; final audit in
  Phase 1H to confirm.

---

## 5. Risk Register

| # | Risk | Level | Mitigation |
|---|---|---|---|
| 1 | Removing entitlement gate silently disables quota enforcement | **High** | Phase 2 replaces with admin-config resource limits **before** entitlement removal is committed |
| 2 | Scheduler / proxy checks coupled to billing via `worker/src/config.rs` | **High** | Batch split: config + scheduler changes in same commit; `cargo check` between batches |
| 3 | Historical migrations drop data if applied destructively | **High** | New forward migration only; `DROP TABLE IF EXISTS … CASCADE` on confirmed-billing-only tables; no ALTER on monitoring tables |
| 4 | Frontend routes left dangling after page removal | **Medium** | Single commit per frontend page + route + nav update |
| 5 | Tests for billing-only features left in CI → red CI | **Medium** | Delete billing-only tests in same batch as their target code |
| 6 | `reconciliation_worker` shares code with non-billing reconciliation | **Medium** | Read full file before deletion; split if needed |
| 7 | `resolve_entitlements` referenced by scheduler / monitor use cases | **High** | Rename → `resolve_resource_limits`, in Phase 2 before Phase 1 commits touch it |
| 8 | Hidden billing env vars in systemd / Caddy config | **Low** | Grep `deploy/`, `scripts/`, `Caddyfile` in Phase 1J |
| 9 | `.project-cleanup-backup-*` accidentally included in ZIP | **Low** | Exclude at packaging time (Phase 14) |
| 10 | `xargs wc` I/O error blocks LOC reporting | **Low** | Non-blocking; diagnose in Phase 11 |

---

## 6. Recommended Execution Plan (Phase 1 — Billing Removal)

Each batch is one commit, ends with `cargo check --workspace 2>&1`, and is
verified by the user before the next batch.

| Batch | Scope | Depends on |
|---|---|---|
| 1A | Delete **frontend** billing pages / api / routes / nav | — |
| 1B | Delete **API** billing handlers / routes; update `router.rs` / `state.rs` / `main.rs` | — |
| 1C | Delete **application** billing use cases / commands / ports | 1B |
| 1D | Delete **domain** billing entities / enums / modules | 1C |
| 1E | Delete **infra** billing repositories / services; update `lib.rs` | 1D |
| 1F | Delete **worker** billing jobs; update `main.rs` / `lib.rs` / `config.rs` / `durable_scheduler.rs` | 1E |
| 1G | Surgical cleanup of remaining **mixed** files (monitors, tenants, retention, alert allowance) | 1F |
| 1H | Remove billing env vars from `docker-compose*.yml` / `.env.e2e.example` | 1G |
| 1I | Remove billing dependencies from `Cargo.toml` files | 1G |
| 1J | Delete **billing-only tests**; fix any remaining test compiles | 1B–1F |
| 1K | Author forward migration `013x_drop_commercial_billing.sql` | 1E |
| 1L | Replace entitlement → **admin-config resource limits** (§5 of master prompt) | 1K |
| 1M | Update `README.md`, `CHANGELOG.md`, `ARCHITECTURE.md`, `.env.example` (if needed) | 1L |
| 1N | Final quality gate: `cargo fmt --check`, `cargo check`, `cargo test`, `cargo clippy -D warnings` | 1M |
| 1O | Update CI workflows to drop billing-only jobs | 1N |

**Conservative order:** 1A → 1B → 1C → 1D → 1E → 1F → 1G → 1H → 1I → 1J → 1K
→ 1L → 1M → 1N → 1O.

**Rule of thumb for every batch:** if a batch would break compilation on its
own, it must be combined with the batch that repairs the call site. The batches
above are ordered to respect this — each batch leaves the workspace compiling
*at the end of the batch*, not mid-batch.

---

## 7. What This Audit Does NOT Do

- It does **not** modify any source file.
- It does **not** delete any test.
- It does **not** touch any migration.
- It does **not** alter `Cargo.toml`.
- It does **not** remove `reconciliation_worker` or any worker without further
  reading — those are flagged for Phase 1F.

---

## 8. Sign-Off Required

Proceed to Phase 1 (batch 1A — frontend) only after the user reviews this
document and explicitly approves the deletion plan.

Any deviation from this plan must be recorded as an amendment to this file
before it is executed.

---

*End of audit.*
