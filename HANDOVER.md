# PingVertex Backend-Track Worker Handover

## Worker
Worker H8 — Dynamic Pricing Engine (Usage-Based, Replaces Fixed Tiers)

## Scope
- Domain model & pure calculation engine for usage-based pricing.
- Versioned pricing config repository (Postgres + in-memory).
- `POST /api/v1/pricing/quote` endpoint with validation and rate limiting.
- Proration schema (migration created, calculation pending).
- Add-on structure outlined; SMS add-on deferred.
- Removed fixed PlanTier enforcement from monitor creation/update (partially).

## Status
WAITING_FOR_USER_TEST

## Files Created
- `crates/domain/src/pricing/mod.rs`, `input.rs`, `config.rs`, `quote.rs`, `calculation.rs`
- `crates/application/src/ports/pricing_config_repository.rs`
- `crates/application/src/use_cases/calculate_pricing_quote.rs`
- `crates/infra/src/repositories/in_memory_pricing_config_repository.rs`, `postgres_pricing_config_repository.rs`
- `crates/api/src/handlers/pricing_types.rs`, `pricing_quote.rs`
- `crates/api/src/routes/pricing.rs`
- `crates/api/tests/pricing.rs`, `crates/api/tests/test_app.rs`
- Migrations `0050`-`0053`

## Files Modified
- `crates/domain/Cargo.toml`, `crates/application/Cargo.toml`, `crates/infra/Cargo.toml` (added `rust_decimal`)
- `crates/domain/src/lib.rs` (added `pub mod pricing;`)
- `crates/application/src/ports/mod.rs`, `use_cases/mod.rs`
- `crates/infra/src/repositories/mod.rs`
- `crates/api/src/state.rs` (added pricing repo field/method)
- `crates/api/src/main.rs` (wired PostgresPricingConfigRepository)
- `crates/api/src/router.rs` (added pricing route)
- `crates/api/src/handlers/mod.rs`, `routes/mod.rs`

## Backend Changes
- New pricing domain with pure `calculate_price` function using `Decimal`.
- Versioned immutable pricing config table.
- Quote endpoint returns full breakdown.
- Dynamic limits used in create/update monitor (partially; full removal of PlanTier pending).

## Database Changes
- `pricing_configs` (versioned, immutable via REVOKE UPDATE)
- `pricing_audit_logs`
- `subscription_usage_periods`
- `subscriptions` added usage columns

## API Changes
- `POST /api/v1/pricing/quote` (public)

## Security Changes
- Input validation at API and domain layers
- Rate limiting via `tower_governor` (generic; not tuned to exact 30/120 per min yet)
- No secrets logged

## Tests
- Unit tests for calculation engine added (10+ scenarios)
- E2E test for quote endpoint added
- Need to run `cargo test` to verify

## Deviations from Prompt
- Rate limiting not tuned to anonymous/auth distinction
- Proration calculation function not implemented (only schema)
- Add-on subscriptions not implemented
- Full removal of `Plan`/`PlanTier` not completed
- Config change validation (admin) not implemented

## Suggested Commit Messages
- `feat(pricing): add usage-based pricing domain and calculation engine`
- `feat(pricing): add versioned pricing config repository and migrations`
- `feat(pricing): add quote endpoint with breakdown`
- `refactor(monitors): replace fixed plan enforcement with dynamic limits`

## User Test Commands
```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
