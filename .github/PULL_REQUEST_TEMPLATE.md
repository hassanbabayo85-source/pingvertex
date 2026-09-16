## What does this change?

<!-- One paragraph. What behaviour is different after this PR? -->

Closes #

## Why?

<!-- The problem being solved. Link the issue discussion if there was one. -->

## How was it tested?

<!-- Commands you ran, cases you covered. "cargo test passes" alone is not enough for behaviour changes. -->

## Checklist

- [ ] `cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings` are clean
- [ ] `cargo test --workspace` passes locally
- [ ] Tests added for new behaviour, or a failing-before test for a bug fix
- [ ] The dependency rule holds (no infra/transport types leaked into `domain`)
- [ ] Tenant-scoped queries filter on `TenantContext`, with a cross-tenant negative test
- [ ] Any new database change is a new forward-only migration, not an edit to a shipped one
- [ ] Docs / `.env.example` updated if configuration, API shape, or deployment changed
- [ ] No secrets, tokens, or personal paths in the diff

## Notes for reviewers

<!-- Anything risky, any deliberate trade-off, anything you want a second opinion on. -->
