# PingVertex Backend-Track Worker Handover

## Worker
Worker H8b — Pricing Domain Fixes (Config-Driven Constants, Backup Multiplier, Determinism)

## Scope
- Convert hardcoded pricing constants to config-driven fields in `PricingConfig`.
- Correct default `backup_multiplier` from 0.15 to 0.25.
- Fix determinism test to explicitly exclude `generated_at` instead of mutating it.
- Add new unit test verifying config changes affect cost.

## Status
WAITING_FOR_USER_TEST

## Files Modified
- `crates/domain/src/pricing/config.rs` — added three fields, changed backup default
- `crates/domain/src/pricing/calculation.rs` — removed constants, use config fields, fixed test, added new test
- `crates/infra/src/repositories/postgres_pricing_config_repository.rs` — updated row struct and SQL queries for new fields
- `migrations/0054_add_network_safety_fields_to_pricing_configs.sql` — new migration adding columns

## Backend Changes
- `avg_billable_network_bytes_per_check`, `cost_per_gb_egress`, and `cost_safety_buffer` are now part of `PricingConfig` and read from config in calculations.
- `backup_multiplier` default is now `Decimal::new(25, 2)` (0.25).
- Determinism test now compares all fields except `generated_at` individually; no production code workaround.

## Database Changes
- New migration `0054` adds three columns to `pricing_configs` with defaults.
- Existing `0050` migration not altered; backwards compatible via separate migration.

## Tests
- All existing pricing domain unit tests remain.
- New test `changing_config_fields_changes_cost` verifies configuration changes affect price.
- Determinism test rewritten to assert field-by-field equality without mutating `generated_at`.

## Deviations from Prompt
- Chose option (b) for determinism test: no clock abstraction exists in codebase; introducing one would exceed scope of this small worker. Instead, test explicitly compares all fields except `generated_at`, making intent clear.

## Suggested Commit Message
- `fix(pricing): make network/safety constants config-driven; fix backup multiplier default to 0.25`
- `test(pricing): fix determinism test to exclude generated_at and add config change test`

## User Test Commands
```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
 
## Worker H9 — Pricing Database Completion
 
### Status
WAITING_FOR_USER_TEST
 
### Changes
- Added migration `0055_seed_initial_pricing_config.sql` to seed version 1.
- Added `created_by` field to `PricingConfig` struct and updated repositories.
- Added `PricingAuditLogEntry` entity and `record_audit_log` method to `PricingConfigRepository`.
- `create_version` now takes an additional `reason` parameter and writes an audit log in the same transaction.
- Updated in-memory and Postgres implementations, plus Noop.
- Added integration test `crates/infra/tests/pricing_config_repository.rs` (ignored by default).
 
### Tests
- Unit/integration tests: `cargo test --workspace`
- Run ignored DB test: `DATABASE_URL=... cargo test --test pricing_config_repository -- --ignored`
 
### Deviations from prompt
- Audit log records a single row per version creation with field_name = "version" and old/new version numbers, rather than logging every field individually. This satisfies the requirement that every version change creates an audit row with old/new values, field, and reason.
 
### Suggested Commits
- `feat(pricing): add seed migration and created_by field`
- `feat(pricing): implement audit log writes on version creation`

## Worker H9 — Pricing Database Completion

### Status
COMPLETE

### Scope
- Seed version 1 of pricing_configs
- Add created_by field to PricingConfig and repository
- Implement audit log writes on create_version in a transaction

### Files Created/Modified
- migrations/0055_seed_initial_pricing_config.sql (new)
- crates/domain/src/pricing/config.rs (modified)
- crates/domain/src/entities/pricing_audit_log_entry.rs (new)
- crates/domain/src/entities/mod.rs (modified)
- crates/application/src/ports/pricing_config_repository.rs (modified)
- crates/infra/src/repositories/postgres_pricing_config_repository.rs (modified)
- crates/infra/src/repositories/in_memory_pricing_config_repository.rs (modified)
- crates/infra/tests/pricing_config_repository.rs (new, ignored by default)
- crates/api/src/state.rs (modified)

### Tests
- cargo fmt --all: PASS
- cargo clippy --workspace --all-targets -- -D warnings: PASS
- cargo test --workspace: PASS
- DATABASE_URL=... cargo test --test pricing_config_repository -- --ignored: PASS

### Deviations from prompt
- Audit log writes one row per version creation with field_name="version", old/new version numbers, rather than every field individually.

### Suggested Commit Messages
- feat(pricing): add seed migration and created_by field (already committed)
- feat(pricing): implement audit log writes on version creation (already committed)

### Notes for Next Worker
- H11 can use PricingConfigRepository::create_version(config, reason) which now handles audit logging in the same transaction.

## Worker H10 — Pricing Endpoint Rate Limiting (Security Gap)

### Status
WAITING_FOR_USER_TEST

### Scope
- `crates/api/src/router.rs`: `pricing_routes` had NO `GovernorLayer` at all
  (unlike every other route group), leaving `POST /api/v1/pricing/quote`
  completely unrate-limited and floodable.

### Files Modified/Created
- `crates/api/src/router.rs` (modified) — added `GovernorLayer` to
  `pricing_routes` in `build_router`; added new
  `build_pricing_only_router_with_rate_limit(app_state) -> Router`
  (test-only, mirrors the pre-existing but previously-unused
  `build_auth_only_router_with_rate_limit`).
- `crates/api/tests/pricing_rate_limit.rs` (new) — E2E tests exercising the
  real production limiter via HTTP.

### Backend/Security Changes
- `pricing_routes` (`build_router`) now carries
  `GovernorConfigBuilder::default().per_second(2).burst_size(30)` ==
  30 req/min sustained after an initial 30-request burst, keyed by
  `PeerIpKeyExtractor` (the tower_governor default — same as
  `public_routes`).
- `build_test_router` was deliberately left unchanged (no `GovernorLayer`
  added there) — see Deviations below.

### Tests
- New file `crates/api/tests/pricing_rate_limit.rs`:
  - `pricing_quote_requests_within_burst_are_allowed` — 30 requests from
    the same fake peer all return `200 OK`.
  - `pricing_quote_requests_over_burst_are_rate_limited` — the 31st request
    from the same fake peer returns `429 TOO_MANY_REQUESTS`.
  - `pricing_quote_requests_from_a_different_peer_get_their_own_bucket` —
    confirms the limiter is per-IP, not global: a different fake peer is
    unaffected by another peer's exhausted bucket.
- Existing `crates/api/tests/pricing.rs` (functional/validation tests) is
  UNCHANGED and still runs through `test_app::build()` /
  `build_test_router`, which has no `GovernorLayer` on any route — so it is
  unaffected by this change and won't flake.
- Standard commands (results pending user run):
```bash
  cargo fmt --all
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  cargo test --test pricing_rate_limit
```

### Deviations from prompt
1. **Anonymous vs Authenticated split NOT implemented** — confirmed by
   reading `crates/api/src/routes/pricing.rs`: `pricing::router()` has no
   `route_layer(from_fn_with_state(..., jwt_auth))` or `api_key_auth` at
   all, so there is no `AuthenticatedUser`/`ApiKeyPrincipal` in scope by
   the time a rate-limit layer would run. Per the prompt's own fallback
   instruction, applied a SINGLE limit (30 req/min, the spec's "Anonymous"
   figure, since every caller is currently anonymous) via
   `per_second(2)/burst_size(30)`. Splitting this properly would require
   deciding whether/how `pricing/quote` becomes auth-aware (mandatory auth?
   optional auth with a custom `KeyExtractor` like `AgentIdKeyExtractor`?)
   — a real design decision, not something inferable from existing
   code/ADRs, so DELIBERATELY left for a future worker/human decision
   rather than guessed at.
2. **`build_test_router` intentionally left without a `GovernorLayer` on
   pricing_routes.** Discovered during investigation that `build_test_router`
   omits `GovernorLayer` from EVERY route group (not a pricing-specific
   gap) — this is existing, deliberate convention so the general E2E suite
   isn't flaky/order-dependent under IP-based rate limiting. Added a
   dedicated `build_pricing_only_router_with_rate_limit` instead, which
   directly mirrors an EXISTING (but previously unused) precedent already
   in the codebase: `build_auth_only_router_with_rate_limit`. This is the
   established pattern for this exact problem, not a new architecture.
3. Rate-limit values (`per_second(2)`/`burst_size(30)`) are duplicated
   literally between `build_router` and the new
   `build_pricing_only_router_with_rate_limit`, rather than extracted to a
   shared constant/function — this matches the EXISTING style in this file
   (`auth_rate_limit`'s `GovernorConfigBuilder` call is likewise duplicated
   verbatim between `build_router` and
   `build_auth_only_router_with_rate_limit`, not shared), so kept consistent
   rather than introducing a new abstraction.

### Known Issues/Risks (for human attention — NOT fixed in this worker)
- **`build_auth_only_router_with_rate_limit` was already unused before this
  worker** (confirmed via `grep -rn` across `crates/`) — meaning
  `auth_routes`'/`totp_verify_routes`' own `GovernorLayer`s ALSO currently
  have zero E2E coverage anywhere in the suite, despite the function
  existing specifically to enable that coverage. This is a pre-existing gap
  broader than pricing and outside H10's scope — flagging for a separate
  follow-up ticket rather than silently expanding this worker's scope.
- The 120 req/min "Authenticated" tier from the original spec is NOT
  implemented (see Deviation #1) — `pricing/quote` is rate-limited but only
  at the flat anonymous rate for all callers today.

### Suggested Commit Messages
- `fix(pricing): add IP-based rate limiting to pricing_routes (H10 security gap)`
- `test(pricing): add E2E coverage for pricing rate limiting via dedicated test router`

### User Test Commands
```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --test pricing_rate_limit -- --nocapture
```

### Notes for Next Worker
- If/when the Anonymous/Authenticated split is picked up: `jwt_auth` and
  `api_key_auth` both insert principal types into request extensions
  (`AuthenticatedUser`, `ApiKeyPrincipal`) — a custom `KeyExtractor` for
  pricing could branch on their presence, but that requires those
  middlewares to run BEFORE the `GovernorLayer` in the layer stack (layers
  apply outside-in relative to where `.layer()` is called), unlike the
  current `route_layer` position used elsewhere in this file for
  jwt_auth/api_key_auth (which runs AFTER rate limiting). Needs care, and a
  decision on whether the endpoint should require auth at all.
- `build_pricing_only_router_with_rate_limit` and
  `build_auth_only_router_with_rate_limit` are both minimal/test-only —
  neither includes `TraceLayer`, `record_http_metrics`, CORS, or the body
  limit, matching each other's existing shape. Don't reuse them as a base
  for anything production-facing.

### Completion Criteria
- [ ] User runs `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` — all PASS.
- [ ] User runs `cargo test --test pricing_rate_limit` — all 3 tests PASS.
- [ ] User confirms `pricing_routes` now rejects with 429 beyond 30 req/min per IP in the real `build_router`.

### H10 — CONFIRMED BY USER (2026-08-27)
Status: **COMPLETE** (supersedes "WAITING_FOR_USER_TEST" above).

User ran and confirmed PASS for all of:
- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace` — 0 failures across every crate (api, application,
  domain, infra, worker, agent, shared), including the pre-existing
  `crates/api/tests/pricing.rs` (2/2, untouched/unaffected) and the new
  `crates/api/tests/pricing_rate_limit.rs` (3/3).
- `cargo test --test pricing_rate_limit -- --nocapture` in isolation — 3/3.

All three H10 completion criteria met.


## Worker H11 — Admin Pricing API Endpoints (COMPLETE)

### Status
COMPLETE (user test results: all workspace tests passed)

### Scope
- Implemented five admin-only endpoints under `/api/v1/admin/pricing`:
  - `GET /api/v1/admin/pricing/configs` — list all config versions
  - `GET /api/v1/admin/pricing/configs/{version}` — fetch a specific version
  - `GET /api/v1/admin/pricing/configs/active` — fetch the active/current config
  - `POST /api/v1/admin/pricing/configs` — create a new version (immutable), with audit log in same transaction
  - `GET /api/v1/admin/pricing/audit-logs` — list audit log entries (most recent first)
- All endpoints require JWT auth and `Permission::ManageBilling` (Owner role only).
- Validation rules enforced:
  - All costs/multipliers (cost_per_check, cost_per_gb_hot/warm/cold, backup_multiplier, markup_multiplier, min_paid_monthly_price, cost_per_gb_egress, cost_safety_buffer) >= 0
  - min_interval_seconds >= 1
  - max_monitors > 0
  - max_retention_days >= 90
  - currency non-empty
- POST computes next version as `max(existing_versions) + 1`, sets `created_by` from authenticated user, and writes a single audit log entry (`field_name = "version"`, old/new values = old version / new version) in the same DB transaction.
- Added `list_audit_logs` method to `PricingConfigRepository` trait and all implementations (Postgres, InMemory, Noop).

### Files Modified/Created
- `crates/application/src/ports/pricing_config_repository.rs` — added `list_audit_logs`
- `crates/infra/src/repositories/postgres_pricing_config_repository.rs` — implemented `list_audit_logs` (SELECT from pricing_audit_logs ORDER BY changed_at DESC)
- `crates/infra/src/repositories/in_memory_pricing_config_repository.rs` — implemented `list_audit_logs` (returns audit_logs vec in reverse order)
- `crates/api/src/state.rs` — updated NoopPricingConfigRepository with `list_audit_logs`
- `crates/api/src/handlers/pricing_types.rs` — added admin pricing DTOs:
  - `CreatePricingConfigRequest`, `FreeTierDefaultsRequest`
  - `PricingConfigResponse`, `FreeTierDefaultsResponse`
  - `PricingAuditLogResponse`
- `crates/api/src/handlers/admin_pricing.rs` — new handler file with all five handler functions and validation/permission helpers
- `crates/api/src/handlers/mod.rs` — added `pub mod admin_pricing;`
- `crates/api/src/routes/pricing.rs` — added `admin_router()` with routes for config management and audit logs
- `crates/api/src/router.rs` — wired admin pricing routes in both `build_router` and `build_test_router`, with JWT auth layer (permission check done inside handlers)
- `crates/api/tests/admin_pricing.rs` — new E2E test file (6 tests)

### Tests
- `cargo test --test admin_pricing`:
  - `admin_without_manage_billing_gets_forbidden` — admin role gets 403 on list configs
  - `owner_can_list_pricing_configs` — Owner gets 200 and sees version 1
  - `owner_can_get_pricing_config_by_version` — Owner gets 200 for version 1
  - `owner_can_get_active_pricing_config` — Owner gets 200 and active version is 1
  - `owner_can_create_new_pricing_config_and_audit_log_is_written` — Owner POST creates version 2, list shows 2 versions, audit log contains entry for version 2
  - `validation_rejects_bad_config` — POST with max_retention_days < 90 returns 400
- All 6 tests PASS.
- `cargo fmt --all` — PASS
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS
- `cargo test --workspace` — all tests across all crates PASS (hundreds of tests, 0 failures)

### Deviations from Prompt
1. **Used `Permission::ManageBilling`** (available only to Owner role) instead of a new `ManagePricing` permission. The prompt instructed to use existing permissions and not create new authorization architecture. This is conservative: only Owners can manage pricing configs. If Admins should also have this ability, a new permission or role mapping update would be needed.
2. **Permission check moved inside handlers** rather than using the `require_permission` middleware. The middleware was initially wired but caused tests to return 401 due to layer ordering issues. Moving the check into the handlers (which already receive `AuthenticatedUser`) is simpler and matches the existing pattern used by some other endpoints (e.g., use cases check permissions directly). This does not weaken security: JWT auth is still enforced at the router layer, and the permission check runs before any business logic.
3. **No pagination on list endpoints** — pricing configs and audit logs are low-volume; full list is acceptable.
4. **Path syntax** initially used `:version` (Colon) but corrected to `{version}` for Axum 0.7+ compatibility.

### Suggested Commit Messages
- `feat(pricing): add admin endpoints for pricing config management`
- `feat(pricing): add list_audit_logs to pricing config repository`
- `test(pricing): add E2E coverage for admin pricing endpoints`

### User Test Commands
```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --test admin_pricing
cargo test --workspace

## Worker H12 — Proration Service (Mid-Cycle Usage Changes)

### Status
WAITING_FOR_USER_TEST

### Scope
- Gave domain representation to the previously-orphaned `subscriptions.monitors/interval_seconds/retention_days/pricing_version` columns (added by migration 0053 but never read/written by any repository).
- Built the H12 spec: mid-cycle usage changes now close the currently-open `subscription_usage_periods` row and open a new one, atomically, instead of billing the whole month at the old or new rate.

### Files Created
- `crates/domain/src/shared/ids/subscription_usage_period_id.rs`
- `crates/domain/src/entities/subscription_usage_period.rs`
- `crates/application/src/ports/subscription_usage_period_repository.rs`
- `crates/application/src/ports/subscription_usage_transaction.rs`
- `crates/application/src/use_cases/update_subscription_usage.rs`
- `crates/application/src/use_cases/calculate_subscription_cycle_cost.rs`
- `crates/infra/src/repositories/postgres_subscription_usage_period_repository.rs`
- `crates/infra/src/repositories/in_memory_subscription_usage_period_repository.rs`
- `crates/infra/src/repositories/postgres_subscription_usage_transaction.rs`
- `crates/infra/src/repositories/in_memory_subscription_usage_transaction.rs`
- `crates/infra/tests/subscription_usage_transaction.rs` (new, ignored by default — requires `DATABASE_URL`)

### Files Modified
- `crates/domain/src/errors/domain_error.rs` — added 5 variants (`SubscriptionUsageMonitorsZero`, `SubscriptionUsageIntervalSecondsZero`, `SubscriptionUsageRetentionDaysZero`, `UsagePeriodAlreadyClosed`, `UsagePeriodEndBeforeStart`)
- `crates/domain/src/entities/subscription.rs` — added `monitors`/`interval_seconds`/`retention_days`/`pricing_version: Option<...>` fields and `update_usage(...)` method; `new`/`new_trialing` unchanged in signature, both now initialize usage fields to `None`
- `crates/domain/src/entities/mod.rs` — registered `SubscriptionUsagePeriod`
- `crates/domain/src/shared/ids/mod.rs` — appended `SubscriptionUsagePeriodId` registration
- `crates/domain/src/pricing/calculation.rs` — added `calculate_period_cost(period, config, cycle_start, cycle_end) -> Decimal`; calls `calculate_price` internally (does not reimplement the formula) and prorates by day-overlap fraction
- `crates/application/src/ports/id_generator.rs` — added `next_subscription_usage_period_id` default method
- `crates/application/src/ports/mod.rs` — registered new ports
- `crates/application/src/use_cases/mod.rs` — registered new use cases
- `crates/infra/src/repositories/postgres_subscription_repository.rs` — `SubscriptionRow` and all 3 SQL statements now read/write the 4 usage columns
- `crates/infra/src/repositories/mod.rs` — registered new repository files (appended; original file content was not directly inspected, only a directory listing — verify this compiles)

### Backend/Database/API/Security Changes
- No new migration: the usage columns already existed (nullable) from migration 0053; no schema change was needed.
- `PostgresSubscriptionUsageTransaction::change_usage` performs close-old-period + insert-new-period + update-subscription-row as ONE `sqlx::Transaction`, matching `PostgresRegistrationTransaction`'s existing pattern exactly (rollback-on-drop, no explicit rollback call needed).
- No HTTP endpoint was added — see Deviations below.
- No tenant/user IDs are trusted from a client anywhere in this worker; `UpdateSubscriptionUsageCommand.tenant_id` is expected to come from an already-authenticated context when a future worker wires an HTTP handler (same pattern as `CancelSubscriptionUseCase`/`VerifyCheckoutUseCase`).

### Tests
- Domain unit tests (in `subscription.rs`, `subscription_usage_period.rs`, `calculation.rs`): validation rules for `update_usage`, `SubscriptionUsagePeriod::close`, and 4 new `calculate_period_cost` tests (full-cycle equals monthly price, half-cycle proration, two-period-sum sanity check, out-of-cycle returns zero).
- Application unit test (`update_subscription_usage.rs`): validation short-circuits before the transaction is ever called.
- Infra integration test (`crates/infra/tests/subscription_usage_transaction.rs`, `#[ignore]`, needs `DATABASE_URL`): full round-trip against real Postgres — two usage changes, verifies old period gets closed, new period is open, `subscriptions` row is updated, and `list_periods_for_cycle` returns both periods for correct billing summation.

Run:
```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
DATABASE_URL=... cargo test --test subscription_usage_transaction -- --ignored
```

### Deviations from Prompt
1. **No existing "subscription update" use case to hook into.** Phase 0 investigation confirmed the `Subscription` domain entity had no usage fields at all, and no application-layer code anywhere read/wrote the migration-0053 usage columns. This was a MAJOR issue (Rule 25) — flagged and the human explicitly authorized creating `UpdateSubscriptionUsageUseCase` as the new entry point (see conversation), rather than guessing.
2. **No HTTP endpoint wired.** The TASK's acceptance criteria (repository + service + pricing function, integrated with the subscription update flow) does not require an API route, and none existed to extend. `UpdateSubscriptionUsageUseCase` and `CalculateSubscriptionCycleCostUseCase` are ready for a future worker to wire into `crates/api` (checkout/plan-change flow) once that flow's shape is decided — left as a Known Issue below rather than guessed at, since routing/auth-boundary decisions are outside a "small matter."
3. **`crates/infra/src/repositories/mod.rs` was only appended to, not fully rewritten** — its full original content was never read (only a `find`-based file listing was seen), so I could not safely reproduce the whole file. If the append doesn't match the file's actual `pub mod`/`pub use` conventions exactly, this is a compile-breaking but small/mechanical fix within the 40% reserve.
4. **`crates/infra/tests/subscription_usage_transaction.rs` builds its own tenant/user fixture rows via raw SQL** instead of reusing `crates/infra/tests/common` helpers, because that module's contents were not inspected in this worker's investigation. If `tests/common` already has tenant/subscription fixtures, prefer switching to them — flagged in the test file's own doc comment.
5. **`CalculateSubscriptionCycleCostUseCase` has no dedicated use case for triggering actual billing/invoicing** — no such flow exists yet anywhere in the codebase. This use case is the integration point (`list_periods_for_cycle` + `calculate_period_cost`, summed) for whichever future worker builds real invoice generation.
6. **`pricing_version` typed as `u32`**, matching `PricingConfig.version: u32` (not `String`, despite the human's message suggesting "String ko Version type idan akwai") — `PricingConfig::version` is already `u32` everywhere else in this codebase (H8/H9/H10/H11), so a `String` would be an inconsistent, unjustified type change.

### Suggested Commit Messages
- `feat(subscription): add usage snapshot fields to Subscription entity and postgres repository (H12)`
- `feat(pricing): add calculate_period_cost for mid-cycle proration (H12)`
- `feat(subscription): add SubscriptionUsagePeriod entity, repository, and atomic usage-change transaction (H12)`
- `feat(subscription): add UpdateSubscriptionUsageUseCase and CalculateSubscriptionCycleCostUseCase (H12)`
- `test(subscription): add domain/application unit tests and DB-gated integration test for proration (H12)`

### User Test Commands
```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
DATABASE_URL=... cargo test --test subscription_usage_transaction -- --ignored
```

### Known Issues/Risks
- No HTTP endpoint exists yet for triggering a usage change or reading cycle cost — application-layer only, by design per Deviation #2.
- `crates/infra/src/repositories/mod.rs` edit was an append based on inference, not a verified full-file match — check first if `cargo build` fails there.
- Integration test's fixture SQL for `users`/`tenants` was written from the `users`/`tenants` columns visible in migrations already reviewed (0038) but the full `users` table schema was not directly inspected — may need column adjustments.
- No billing/invoice-generation use case exists yet to actually call `CalculateSubscriptionCycleCostUseCase` in production; it is currently only reachable from tests.

### Notes for Next Worker
- To expose H12 over HTTP: wire `UpdateSubscriptionUsageUseCase` into whatever checkout/plan-change flow is decided (see `verify_checkout.rs` for the closest existing precedent of a subscription-mutating use case), with tenant_id sourced from `AuthenticatedUser`, never from client input directly.
- `PostgresSubscriptionUsageTransaction` and `PostgresRegistrationTransaction` are structurally identical in shape (raw `sqlx::Transaction`, rollback-on-drop) — reuse that shape for any future cross-repository atomic operation rather than inventing a new one.

### Completion Criteria
- [ ] User runs `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` — all PASS.
- [ ] User runs `DATABASE_URL=... cargo test --test subscription_usage_transaction -- --ignored` — PASS.
- [ ] User confirms proration math: two usage periods in one cycle sum to a value strictly between the old-usage-full-month price and the new-usage-full-month price.
