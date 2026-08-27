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
