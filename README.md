<div align="center">

# PingVertex

**Uptime monitoring that tells you *why* it broke, not just *that* it broke.**

[![CI](https://github.com/hassanbabayo85-source/pingvertex/actions/workflows/ci.yml/badge.svg)](https://github.com/hassanbabayo85-source/pingvertex/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)

[Quick start](#quick-start) · [Architecture](docs/ARCHITECTURE.md) · [Contributing](CONTRIBUTING.md) · [Security](SECURITY.md)

</div>

---

## What is PingVertex?

PingVertex is a self-hostable, multi-tenant uptime monitoring platform written in Rust. Most monitors tell you a service is down and stop there. PingVertex is built around diagnostics: when a check fails, it captures enough context to explain the failure — DNS, TLS, redirects, response timing, and the state of the incident over time.

It runs as a single Docker Compose stack on one machine, or as distributed agents checking from multiple regions.

> **Status:** early. The API surface and database schema still change between releases. Self-host at your own pace and read the migration notes before upgrading.

## Features

| | |
|---|---|
| **Monitoring** | HTTP/HTTPS checks, configurable intervals, retries, custom headers and bodies, keyword and status-code assertions |
| **Multi-step API flows** | Chain requests, pass values between steps, assert on each response |
| **Incidents** | Automatic incident open/close, flap protection, durable outbox so alerts survive a process crash |
| **Notifications** | Pluggable channels driven by the incident outbox |
| **Status pages** | Public, per-tenant status pages on their own subdomain |
| **Real User Monitoring** | Lightweight browser SDK (`sdk/rum`) collecting page views, JS errors, and resource errors |
| **Distributed agents** | Remote checkers authenticated with mTLS |
| **Multi-tenancy** | Tenants, members, invitations, role-based access, per-tenant data isolation enforced at the repository layer |
| **API access** | Scoped API keys for automation |
| **Billing** | Wallet, pay-as-you-go metering, plan/pricing primitives |
| **Observability** | Prometheus metrics, Grafana dashboards, Alertmanager rules shipped in the stack |
| **Audit trail** | Append-only audit log of tenant-level actions |

## Architecture

PingVertex is a Cargo workspace laid out along clean-architecture lines. Dependencies point inward only: `domain` knows nothing about HTTP, SQL, or transport.

```
crates/
├── domain/        Entities, value objects, policies, repository traits — zero I/O
├── application/   Use cases, commands, queries, DTOs, ports
├── infra/         PostgreSQL repositories, external services, port implementations
├── api/           Axum HTTP API — handlers, routes, middleware
├── worker/        Scheduler, check execution, incident evaluation, notification dispatch
├── agent/         Remote check agent (mTLS)
└── shared/        Cross-cutting helpers (SSRF guard, cookies, crypto)

apps/
└── leptos-dashboard/   Leptos 0.7 (CSR) WebAssembly dashboard
```

Full write-up in [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md); design rationale in [`docs/adr/`](docs/adr/).

**Stack:** Rust (edition 2024) · Axum · SQLx · PostgreSQL 16 · Redis 7 · Leptos + Trunk · Prometheus/Grafana/Alertmanager · Docker Compose

## Quick start

### Requirements

- Docker and Docker Compose v2
- Rust 1.85+ *(only if you want to build outside Docker)*

### Run the stack

```bash
git clone https://github.com/hassanbabayo85-source/pingvertex.git
cd pingvertex

cp .env.example .env     # edit the values before doing anything real
docker compose up -d --build
```

Once containers are healthy:

| Service | URL |
|---|---|
| API | http://127.0.0.1:3000 |
| Dashboard | served by the frontend container |
| Worker metrics | http://127.0.0.1:3001 |
| Grafana | http://127.0.0.1:3002 |
| Prometheus | http://127.0.0.1:9090 |
| Alertmanager | http://127.0.0.1:9093 |
| PostgreSQL | 127.0.0.1:5433 |

Every port is bound to loopback by default — nothing is exposed to your network without an explicit change.

Check it is alive:

```bash
curl http://127.0.0.1:3000/api/v1/health
```

### Local development

```bash
# Backend
cargo build --workspace
cargo test --workspace          # needs the Postgres from docker compose
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all

# Frontend
cd apps/leptos-dashboard
trunk serve                     # cargo install trunk && rustup target add wasm32-unknown-unknown
```

Migrations run automatically via the `migrate` service. To run them by hand:

```bash
cargo install sqlx-cli --no-default-features --features postgres
sqlx migrate run
```

## Deployment

**Never deploy with `docker-compose.yml` alone** — it carries local-dev defaults. Production layers a second file that refuses to start when required secrets are missing:

```bash
docker compose -f docker-compose.yml -f docker-compose.prod.yml up -d
```

Before going live, read [`SECURITY_HARDENING.md`](SECURITY_HARDENING.md). The database bootstraps three least-privilege roles (`pingvertex_migrator`, `pingvertex_app`, `pingvertex_worker`); the runtime never uses the superuser account.

## Configuration

All configuration is environment-based. [`.env.example`](.env.example) documents every variable with its security implications. The ones that matter most:

| Variable | Purpose |
|---|---|
| `POSTGRES_APP_PASSWORD`, `POSTGRES_WORKER_PASSWORD`, `POSTGRES_MIGRATOR_PASSWORD` | Least-privilege runtime roles. Generate with `openssl rand -hex 32` |
| `METRICS_TOKEN` | Guards the worker metrics endpoint. No insecure default |
| `ENVIRONMENT` | Gates development-only behaviour |
| `PINGVERTEX_SSRF_ALLOW_PRIVATE` | Allows checks against private IPs. **Development only** — the app refuses to boot with this enabled outside dev |
| `GOOGLE_CLIENT_ID`, `APPLE_CLIENT_ID` | OAuth sign-in |

## Security

User-supplied URLs are the core attack surface of any monitoring tool, so PingVertex validates every target against SSRF and DNS-rebinding checks before a request leaves the process. JWTs live only in `Secure; HttpOnly; SameSite=Strict` cookies — never in `localStorage`. Proxy credentials are encrypted at rest. `cargo audit` and `cargo deny` run in CI.

Found a vulnerability? Please follow [SECURITY.md](SECURITY.md) — do not open a public issue.

## Contributing

Contributions are welcome. [CONTRIBUTING.md](CONTRIBUTING.md) covers the dev setup, the architectural rules a PR has to respect, and the review process. Good first issues are labelled [`good first issue`](https://github.com/hassanbabayo85-source/pingvertex/labels/good%20first%20issue).

Everyone participating is expected to follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## License

MIT — see [LICENSE](LICENSE).
