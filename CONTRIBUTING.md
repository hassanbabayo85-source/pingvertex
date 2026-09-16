# Contributing to PingVertex

Thanks for taking the time to contribute. This document explains how to get the project running, the rules a change has to respect, and what happens after you open a pull request.

## Before you write code

- **Bugs:** open an issue with reproduction steps. Small, obvious fixes can go straight to a PR.
- **Features:** open an issue first and describe the problem you are solving. This avoids work that gets rejected on scope grounds.
- **Security issues:** do not open an issue. See [SECURITY.md](SECURITY.md).

## Development setup

Requirements: Docker + Compose v2, Rust 1.85 or newer, `sqlx-cli`, and `trunk` for frontend work.

```bash
git clone https://github.com/hassanbabayo85-source/pingvertex.git
cd pingvertex
cp .env.example .env

docker compose up -d postgres redis   # dependencies only
sqlx migrate run

cargo build --workspace
cargo test --workspace
```

Integration tests need the Compose Postgres on port 5433. Set `DATABASE_URL` accordingly:

```bash
export DATABASE_URL="postgres://postgres:postgres@localhost:5433/uptime_monitor_test"
```

Frontend:

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk
cd apps/leptos-dashboard && trunk serve
```

## Architectural rules

These are not style preferences — a PR that breaks them will be asked for changes.

1. **The dependency rule holds.** `domain` depends on nothing in this repo. `application` depends on `domain` and its own ports. `infra` implements ports. `api`, `worker`, and `agent` are delivery layers. No SQL, HTTP, or framework types in `domain`.
2. **Every tenant-scoped query is scoped.** Any repository method touching tenant data takes a `TenantContext` and filters on it. Authorization tests must include a cross-tenant negative case.
3. **User-supplied URLs go through the SSRF guard.** No exceptions, no "internal-only" shortcuts.
4. **Database changes are forward-only migrations** in `migrations/`, numbered sequentially, never edits to an existing migration file that has shipped.
5. **Retries are idempotent.** Anything the scheduler or outbox can run twice must be safe to run twice.

## Before you push

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs the same commands plus `cargo audit` and `cargo deny`. A red CI blocks review.

## Commit messages

[Conventional Commits](https://www.conventionalcommits.org/):

```
feat(api): add pause endpoint for monitors
fix(worker): stop double-counting retries in incident evaluation
docs(readme): correct the Grafana port
```

Scope is usually the crate name.

## Pull requests

- One logical change per PR. Split refactors out from behaviour changes.
- Fill in the PR template; link the issue it closes.
- New behaviour comes with tests. Bug fixes come with a test that fails before the fix.
- Update the docs in the same PR if you changed configuration, API shape, or deployment steps.

Maintainers review on a best-effort basis. If a PR is quiet for a week, a polite ping is fine.

## Documentation

`docs/` holds the architecture write-up and ADRs. If you make a structural decision, add an ADR rather than burying the reasoning in a commit message.

## License

By contributing, you agree that your contributions are licensed under the MIT License that covers this project.
