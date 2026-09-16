#!/usr/bin/env bash
#
# setup-opensource.sh — PingVertex
#
# Yana sanya duk fayilolin al'umma (README, LICENSE, CONTRIBUTING, SECURITY,
# CODE_OF_CONDUCT, templates na .github, .cargo/config.toml mai tsafta, CHANGELOG,
# CODEOWNERS) a wurarensu na gaskiya a cikin repo.
#
# AMFANI:
#   chmod +x setup-opensource.sh
#   ./setup-opensource.sh
#
#   ./setup-opensource.sh --owner sunanka \
#       --security-email security@domain.com \
#       --conduct-email conduct@domain.com --yes
#
#   ./setup-opensource.sh --dry-run     # duba abin da zai faru, ba tare da rubutawa ba
#
# Duk fayil ɗin da ya riga ya kasance za a yi masa BACKUP kafin a sauya shi.
#
set -euo pipefail

OWNER=""
SECURITY_EMAIL=""
CONDUCT_EMAIL=""
COPYRIGHT_NAME="Abubakar Hassan Babayo"
COPYRIGHT_YEAR="2026"
ASSUME_YES=false
DRY_RUN=false
FORCE=false
WITH_CHECKLIST=true

BACKUP_DIR=".backup-opensource-$(date +%Y%m%d-%H%M%S)"
WRITTEN=()
SUBST_FILES=()
BACKED_UP=0

c_red=''; c_grn=''; c_ylw=''; c_blu=''; c_dim=''; c_off=''
if [ -t 1 ]; then
  c_red=$'\033[31m'; c_grn=$'\033[32m'; c_ylw=$'\033[33m'
  c_blu=$'\033[34m'; c_dim=$'\033[2m';  c_off=$'\033[0m'
fi

info() { printf '%s\n' "$*"; }
ok()   { printf '%s✔%s %s\n' "$c_grn" "$c_off" "$*"; }
warn() { printf '%s!%s %s\n' "$c_ylw" "$c_off" "$*"; }
err()  { printf '%s✘%s %s\n' "$c_red" "$c_off" "$*" >&2; }
die()  { err "$*"; exit 1; }
head2(){ printf '\n%s%s%s\n' "$c_blu" "$*" "$c_off"; }

usage() {
  cat <<'USAGE'
setup-opensource.sh — sanya fayilolin al'umma na PingVertex

Zaɓuɓɓuka:
  --owner NAME              Sunan GitHub (zai maye gurbin OWNER)
  --security-email ADDR     Imel don rahoton tsaro
  --conduct-email ADDR      Imel don rahoton hali
  --copyright-name NAME     Sunan mai haƙƙin mallaka (default: Abubakar Hassan Babayo)
  --copyright-year YEAR     Shekara (default: 2026)
  --no-checklist            Kada a rubuta CHECKLIST-HAUSA.md
  -y, --yes                 Kada a yi tambaya (sai an ba da --owner)
  -n, --dry-run             Nuna kawai, kada a rubuta komai
  -f, --force               Ci gaba ko da ba a cikin root ɗin repo ba
  -h, --help                Wannan taimakon
USAGE
}

while [ $# -gt 0 ]; do
  case "$1" in
    --owner)            OWNER="${2:-}"; shift 2 ;;
    --security-email)   SECURITY_EMAIL="${2:-}"; shift 2 ;;
    --conduct-email)    CONDUCT_EMAIL="${2:-}"; shift 2 ;;
    --copyright-name)   COPYRIGHT_NAME="${2:-}"; shift 2 ;;
    --copyright-year)   COPYRIGHT_YEAR="${2:-}"; shift 2 ;;
    --no-checklist)     WITH_CHECKLIST=false; shift ;;
    -y|--yes)           ASSUME_YES=true; shift ;;
    -n|--dry-run)       DRY_RUN=true; shift ;;
    -f|--force)         FORCE=true; shift ;;
    -h|--help)          usage; exit 0 ;;
    *) die "Zaɓi da ba a sani ba: $1  (gwada --help)" ;;
  esac
done

# --- Tabbatar da cewa muna cikin root ɗin project ------------------------------

if [ ! -f Cargo.toml ] && [ ! -d .git ]; then
  if $FORCE; then
    warn "Ba a ga Cargo.toml ko .git ba, amma an ba da --force. Ana ci gaba a: $PWD"
  else
    err "Wannan ba kamar root ɗin repo ba ne (babu Cargo.toml ko .git a nan)."
    err "Shiga cikin folder ɗin project sannan ka sake gudanarwa, ko ka yi amfani da --force."
    exit 1
  fi
fi

if [ -f Cargo.toml ] && ! grep -qi 'pingvertex\|\[workspace\]' Cargo.toml 2>/dev/null; then
  warn "Cargo.toml ɗin da ke nan bai yi kama da na PingVertex workspace ba. Ci gaba da hankali."
fi

# --- Tambayoyi ----------------------------------------------------------------

ask() {
  # ask VARNAME "tambaya" "default"
  local __var="$1" __q="$2" __def="${3:-}" __ans=""
  if $ASSUME_YES; then
    printf -v "$__var" '%s' "${!__var:-$__def}"
    return
  fi
  if [ -n "${!__var:-}" ]; then return; fi
  if [ -n "$__def" ]; then
    read -r -p "$__q [$__def]: " __ans || true
    __ans="${__ans:-$__def}"
  else
    read -r -p "$__q: " __ans || true
  fi
  printf -v "$__var" '%s' "$__ans"
}

head2 "PingVertex — saitin fayilolin open source"
info "${c_dim}Folder: $PWD${c_off}"

if [ -z "$OWNER" ] && [ -d .git ]; then
  _url="$(git config --get remote.origin.url 2>/dev/null || true)"
  case "$_url" in
    *github.com[:/]*)
      _guess="${_url#*github.com}"; _guess="${_guess#[:/]}"; _guess="${_guess%%/*}"
      [ -n "$_guess" ] && [ "$_guess" != "$_url" ] && OWNER="$_guess"
      ;;
  esac
fi

ask OWNER          "Sunan GitHub ɗinka (username ko org)" "${OWNER:-}"
[ -n "$OWNER" ] || die "Ana buƙatar sunan GitHub. Yi amfani da --owner."

ask SECURITY_EMAIL "Imel na rahoton tsaro"  "security@example.com"
ask CONDUCT_EMAIL  "Imel na rahoton hali"   "conduct@example.com"
ask COPYRIGHT_NAME "Sunan mai haƙƙin mallaka (LICENSE)" "$COPYRIGHT_NAME"

$DRY_RUN && warn "DRY RUN — ba za a rubuta komai ba."

# --- Rubuta fayil -------------------------------------------------------------

backup_if_exists() {
  local path="$1"
  [ -e "$path" ] || return 0
  if $DRY_RUN; then
    info "  ${c_dim}(zai yi backup ɗin $path)${c_off}"
    return 0
  fi
  mkdir -p "$BACKUP_DIR/$(dirname "$path")"
  cp -p "$path" "$BACKUP_DIR/$path"
  BACKED_UP=$((BACKED_UP + 1))
}

# write_file <path> [nosubst]   -- yana karanta abun ciki daga stdin
write_file() {
  local path="$1" nosubst="${2:-}"
  local dir; dir="$(dirname "$path")"
  local existed="sabo"
  [ -e "$path" ] && existed="an sauya"

  backup_if_exists "$path"

  if $DRY_RUN; then
    cat >/dev/null
    info "  ${c_dim}zai rubuta${c_off} $path ${c_dim}($existed)${c_off}"
  else
    [ "$dir" = "." ] || mkdir -p "$dir"
    cat > "$path"
    printf '  %s %s %s(%s)%s\n' "$c_grn✔$c_off" "$path" "$c_dim" "$existed" "$c_off"
  fi

  WRITTEN+=("$path")
  [ "$nosubst" = "nosubst" ] || SUBST_FILES+=("$path")
}

# sed mai aiki a Linux da macOS duka
sed_inplace() {
  local expr="$1" file="$2" tmp
  tmp="$(mktemp "${TMPDIR:-/tmp}/pv.XXXXXX")"
  sed "$expr" "$file" > "$tmp"
  cat "$tmp" > "$file"
  rm -f "$tmp"
}

esc() { printf '%s' "$1" | sed -e 's/[\\&|]/\\&/g'; }
head2 "1/3 · Ana rubuta fayilolin al'umma"

write_file README.md <<'PV_README'
<div align="center">

# PingVertex

**Uptime monitoring that tells you *why* it broke, not just *that* it broke.**

[![CI](https://github.com/OWNER/pingvertex/actions/workflows/ci.yml/badge.svg)](https://github.com/OWNER/pingvertex/actions/workflows/ci.yml)
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
git clone https://github.com/OWNER/pingvertex.git
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

Contributions are welcome. [CONTRIBUTING.md](CONTRIBUTING.md) covers the dev setup, the architectural rules a PR has to respect, and the review process. Good first issues are labelled [`good first issue`](https://github.com/OWNER/pingvertex/labels/good%20first%20issue).

Everyone participating is expected to follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## License

MIT — see [LICENSE](LICENSE).
PV_README

write_file LICENSE <<'PV_LICENSE'
MIT License

Copyright (c) 2026 Abubakar Hassan Babayo

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
PV_LICENSE

write_file CONTRIBUTING.md <<'PV_CONTRIB'
# Contributing to PingVertex

Thanks for taking the time to contribute. This document explains how to get the project running, the rules a change has to respect, and what happens after you open a pull request.

## Before you write code

- **Bugs:** open an issue with reproduction steps. Small, obvious fixes can go straight to a PR.
- **Features:** open an issue first and describe the problem you are solving. This avoids work that gets rejected on scope grounds.
- **Security issues:** do not open an issue. See [SECURITY.md](SECURITY.md).

## Development setup

Requirements: Docker + Compose v2, Rust 1.85 or newer, `sqlx-cli`, and `trunk` for frontend work.

```bash
git clone https://github.com/OWNER/pingvertex.git
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
PV_CONTRIB

write_file SECURITY.md <<'PV_SECURITY'
# Security Policy

## Supported versions

PingVertex is pre-1.0. Only the latest release on the `main` branch receives security fixes.

| Version | Supported |
|---|---|
| `main` (latest) | Yes |
| Older tags | No |

## Reporting a vulnerability

**Do not open a public GitHub issue for a security problem.**

Report privately through [GitHub Security Advisories](https://github.com/OWNER/pingvertex/security/advisories/new), or email **security@example.com** *(replace with a real address you monitor)*.

Please include:

- What the issue is and roughly how bad you think it is
- Steps to reproduce, or a proof of concept
- Affected version or commit
- Any suggested fix

**What to expect:** acknowledgement within 72 hours, an initial assessment within 7 days, and updates as the fix progresses. Once a fix ships, you will be credited in the advisory unless you prefer to stay anonymous.

Please give us reasonable time to ship a fix before disclosing publicly.

## Areas of particular interest

If you are looking for somewhere to start, these are the parts of PingVertex where a bug hurts most:

- **SSRF and DNS rebinding** — `crates/shared/src/ssrf.rs`. Users control the URLs being checked; anything that gets a request to a private address is serious.
- **Tenant isolation** — any path where data from one tenant reaches another.
- **Authentication and session handling** — JWT validation, cookie flags, OAuth audience checks.
- **Secret handling at rest** — proxy credential encryption, API key storage.
- **Agent mTLS** — certificate validation on the internal agent boundary.
- **Public ingest endpoints** — the RUM collector accepts unauthenticated traffic by design.

## Out of scope

- Findings that require the operator to have deliberately set a documented development-only flag (for example `PINGVERTEX_SSRF_ALLOW_PRIVATE=true`)
- Weak defaults in `docker-compose.yml`, which is documented as local-only and is not the production path
- Missing security headers on endpoints that serve no sensitive content
- Automated scanner output with no demonstrated impact
- Denial of service through raw traffic volume

## For operators

Before running PingVertex in production, read [`SECURITY_HARDENING.md`](SECURITY_HARDENING.md) and deploy with the production Compose overlay. Rotate the database role passwords and `METRICS_TOKEN` on a schedule, and keep the metrics endpoints off the public internet.
PV_SECURITY

write_file CODE_OF_CONDUCT.md <<'PV_COC'
# Code of Conduct

## Our standard

PingVertex is a project people contribute to in their spare time, often across very different countries, languages, and levels of experience. We want that to stay pleasant.

Participants are expected to:

- Be respectful and assume good faith, especially when someone's first language is not English
- Give technical criticism about the code, not the person who wrote it
- Accept that maintainers make the final call on scope, and that "no" is a legitimate answer
- Respect people's time: search before asking, give reproduction steps, don't demand free support

The following are not acceptable anywhere in the project — issues, pull requests, discussions, or private channels:

- Harassment, insults, or personal attacks
- Discriminatory language or imagery of any kind
- Sexual attention or advances
- Publishing anyone's private information without permission
- Sustained disruption of discussions
- Publicly disclosing a reported security vulnerability before it is fixed

## Scope

This applies in all project spaces and whenever someone is representing the project in public.

## Enforcement

Report problems to **conduct@example.com** *(replace with a real address you monitor)*. Reports are handled confidentially, and the reporter's identity is not shared with the person reported.

Maintainers may respond with a warning, a temporary ban, or a permanent ban, depending on severity and whether the behaviour continues. Maintainers who break these rules face the same consequences as anyone else.

## Attribution

Adapted from the spirit of the [Contributor Covenant](https://www.contributor-covenant.org/), version 2.1.
PV_COC

write_file .cargo/config.toml <<'PV_CARGOCFG'
# Workspace-wide Cargo configuration.
#
# Keep this file free of machine-specific paths. If you need to build to an
# external disk or a different target directory, set it locally instead:
#
#     export CARGO_TARGET_DIR=/path/on/your/machine/cargo-target
#
# or put it in `.cargo/config.local.toml` / your user-level
# `~/.cargo/config.toml`, neither of which is committed.

# Faster linking with `mold`. Install it (`apt install mold`, `brew install mold`)
# or comment this block out if you do not have it.
[target.x86_64-unknown-linux-gnu]
rustflags = ["-C", "link-arg=-fuse-ld=mold"]

# NOTE: DATABASE_URL is deliberately NOT set here.
# Integration tests read it from the environment. See CONTRIBUTING.md:
#
#     export DATABASE_URL="postgres://postgres:postgres@localhost:5433/uptime_monitor_test"
PV_CARGOCFG

write_file .github/PULL_REQUEST_TEMPLATE.md <<'PV_PRTPL'
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
PV_PRTPL

write_file .github/dependabot.yml <<'PV_DEPBOT'
version: 2
updates:
  - package-ecosystem: cargo
    directory: "/"
    schedule:
      interval: weekly
    open-pull-requests-limit: 5
    groups:
      minor-and-patch:
        update-types: ["minor", "patch"]

  - package-ecosystem: github-actions
    directory: "/"
    schedule:
      interval: monthly

  - package-ecosystem: docker
    directory: "/"
    schedule:
      interval: monthly
PV_DEPBOT

write_file .github/ISSUE_TEMPLATE/bug_report.yml <<'PV_BUGRPT'
name: Bug report
description: Something is broken or behaves differently than documented
labels: ["bug", "needs triage"]
body:
  - type: markdown
    attributes:
      value: |
        Thanks for the report. If this is a **security vulnerability**, stop here and follow [SECURITY.md](../../SECURITY.md) instead.

  - type: textarea
    id: what-happened
    attributes:
      label: What happened?
      description: What you expected, and what you got instead.
    validations:
      required: true

  - type: textarea
    id: reproduction
    attributes:
      label: Steps to reproduce
      placeholder: |
        1. Create a monitor with ...
        2. Wait for the first check
        3. Observe ...
    validations:
      required: true

  - type: dropdown
    id: component
    attributes:
      label: Which part?
      options:
        - API
        - Worker / scheduler
        - Agent
        - Dashboard (Leptos)
        - Database / migrations
        - RUM SDK
        - Docker / deployment
        - Not sure
    validations:
      required: true

  - type: input
    id: version
    attributes:
      label: Version or commit
      placeholder: "v0.1.0, or commit sha"
    validations:
      required: true

  - type: textarea
    id: environment
    attributes:
      label: Environment
      placeholder: |
        OS: Ubuntu 24.04
        Deployment: docker compose / bare cargo
        Rust: 1.85.0
        PostgreSQL: 16.6
    validations:
      required: true

  - type: textarea
    id: logs
    attributes:
      label: Relevant logs
      description: Redact secrets, tokens, and real hostnames before pasting.
      render: shell
PV_BUGRPT

write_file .github/ISSUE_TEMPLATE/feature_request.yml <<'PV_FEATREQ'
name: Feature request
description: Suggest a capability PingVertex does not have yet
labels: ["enhancement", "needs triage"]
body:
  - type: textarea
    id: problem
    attributes:
      label: What problem are you trying to solve?
      description: Describe the situation, not the solution. This is the part maintainers weigh most.
    validations:
      required: true

  - type: textarea
    id: proposal
    attributes:
      label: What would you like to see?
    validations:
      required: true

  - type: textarea
    id: alternatives
    attributes:
      label: What have you tried or considered instead?

  - type: dropdown
    id: area
    attributes:
      label: Which area?
      options:
        - Monitoring / checks
        - Incidents & notifications
        - Status pages
        - Real User Monitoring
        - Dashboard / UX
        - API & integrations
        - Billing / plans
        - Deployment & operations
        - Other
    validations:
      required: true

  - type: checkboxes
    id: contribution
    attributes:
      label: Would you be willing to work on this?
      options:
        - label: I would like to implement it myself
        - label: I can help test it
PV_FEATREQ

write_file .github/ISSUE_TEMPLATE/config.yml <<'PV_ISSUECFG'
blank_issues_enabled: false
contact_links:
  - name: Security vulnerability
    url: https://github.com/OWNER/pingvertex/security/advisories/new
    about: Report privately. Never open a public issue for a security problem.
  - name: Question or idea
    url: https://github.com/OWNER/pingvertex/discussions
    about: Usage questions, setup help, and open-ended ideas go to Discussions.
PV_ISSUECFG
if $WITH_CHECKLIST; then
write_file CHECKLIST-HAUSA.md nosubst <<'PV_CHECKLIST'
# PingVertex — Jerin Aikin Maida Aikin Open Source

Wannan fayil ɗin naka ne kaɗai (kada ka tura shi GitHub). Cire shi bayan ka gama.

---

## 1. Abubuwan da DOLE ne a gyara kafin a buɗe repo

- [ ] **`README.md` ba komai a ciki (0 bytes).** Yi amfani da wanda aka rubuta maka. Sauya kowane `OWNER` da ainihin sunan GitHub ɗinka.
- [ ] **`LICENSE` ba komai a ciki (0 bytes).** `Cargo.toml` na cewa MIT amma fayil ɗin babu komai — a doka, hakan yana nufin *babu izini ga kowa*. Sanya fayil ɗin MIT da aka shirya maka.
- [ ] **`.cargo/config.toml` yana ɗauke da hanyar kwamfutarka.**
  `target-dir = "/run/media/abubakar-hassan-babayo/2e9125.../cargo-target"`
  Wannan zai sa ginin kowane mai ba da gudummawa ya gaza, kuma yana fallasa sunan mai amfani da ID ɗin na'urarka. Yi amfani da sabon fayil ɗin da aka shirya.
- [ ] **`DATABASE_URL` a cikin `[env]` na `.cargo/config.toml`** — cire shi; a saka shi a matsayin environment variable a `CONTRIBUTING.md`.
- [ ] **Duba tarihin git (git history).** Repo ɗin yana kan GitHub tun kafin yanzu, don haka ko da yanzu babu sirri a cikin fayilolin, yana yiwuwa akwai a cikin tsofaffin commits:
  ```bash
  pipx install detect-secrets && detect-secrets scan --all-files
  # ko:
  docker run --rm -v "$PWD:/repo" zricethezav/gitleaks:latest detect -s /repo -v
  ```
  Idan an sami wani abu: yi amfani da `git filter-repo` ka goge shi, **sannan ka canza duk kalmomin sirrin da aka fallasa** (fallasa ta riga ta faru).

## 2. Docs — tsaftacewa

Waɗannan suna sa repo ya yi kama da wurin aiki na ciki, ba aikin open source ba:

- [ ] **`HANDOVER.md`** — bayanin mika aiki na ciki ("Worker: L9", "Worker 4"). Ba shi da ma'ana ga baƙi. Matsar zuwa `docs/internal/` ko ka goge shi gaba ɗaya.
- [ ] **`docs/GAP_*_CLOSURE.md`, `docs/PAYG_GAP_1_CLOSURE.md`, `docs/WORKER4_PAYG.md`, `docs/FRONTEND_BACKEND_CONTRACT_AUDIT_2026-09-03.md`** — takardun aiki na ciki. Ka matsar da su zuwa `docs/internal/` ko ka haɗa muhimman abubuwan cikin `docs/ARCHITECTURE.md`.
- [ ] **`docs/ROADMAP.md` babu komai a ciki.** Ko ka cike shi (mutane suna duba roadmap kafin su ba da gudummawa) ko ka goge shi.
- [ ] **ADR guda biyu suna da lamba `0001`** (`0001-rust.md` da `0001-feature-based-architecture.md`). Sake yi wa ɗaya lamba.
- [ ] **Ƙara `docs/README.md`** — ɗan ƙaramin index na abin da ke cikin kowane fayil.

Abin da ke da kyau kuma ya kamata ya zauna: `docs/ARCHITECTURE.md`, `docs/VISION.md`, `docs/adr/`, `SECURITY_HARDENING.md`, `docs/ARCHITECTURE_DECISIONS.md`, `sdk/rum/README.md`.

## 3. Harshe a cikin lambar

- [ ] `.gitignore` da `.repomixignore` suna da sharhi cikin Hausa ("Babban abu da ba a bukata", "Stray files da suka taɓa fitowa", "asalin dalilin cleanup ɗin da muka yi"). Ba laifi ba ne, amma ga aikin ƙasa da ƙasa ya fi kyau a canza zuwa Turanci. (Hausa a cikin README na daban — duba §7.)
- [ ] Bincika sunayen docs: `GAP_3_KASHI_A_CLOSURE.md` da sauransu suna amfani da "KASHI" — sake suna idan sun rage.

## 4. Fayilolin al'umma (an shirya maka su)

- [ ] `README.md`
- [ ] `LICENSE`
- [ ] `CONTRIBUTING.md`
- [ ] `SECURITY.md`
- [ ] `CODE_OF_CONDUCT.md`
- [ ] `.github/ISSUE_TEMPLATE/bug_report.yml`
- [ ] `.github/ISSUE_TEMPLATE/feature_request.yml`
- [ ] `.github/ISSUE_TEMPLATE/config.yml`
- [ ] `.github/PULL_REQUEST_TEMPLATE.md`
- [ ] `.github/dependabot.yml`
- [ ] `CHANGELOG.md` — ƙara shi da sakin farko (tsarin [Keep a Changelog](https://keepachangelog.com))
- [ ] `.github/CODEOWNERS` — layi ɗaya ya isa: `* @sunanka`

**A cikin duk fayilolin da aka shirya maka, sauya:**
- `OWNER` → sunan GitHub ɗinka
- `security@example.com` → adireshin imel na gaskiya da kake duba
- `conduct@example.com` → haka ma

## 5. Metadata na Cargo

A cikin `[workspace.package]` na `Cargo.toml`, ƙara:

```toml
repository = "https://github.com/OWNER/pingvertex"
homepage = "https://pingvertex.com"
documentation = "https://github.com/OWNER/pingvertex/tree/main/docs"
keywords = ["monitoring", "uptime", "observability", "sre"]
categories = ["web-programming", "command-line-utilities"]
readme = "README.md"
rust-version = "1.85"
```

Sannan a kowane `crates/*/Cargo.toml` ƙara `repository.workspace = true` da `description = "..."`.

- [ ] `apps/leptos-dashboard/Cargo.toml` na amfani da `edition = "2021"` amma sauran workspace ɗin `2024`. Daidaita ko rubuta dalili.

## 6. Saitin GitHub (a cikin gidan yanar gizon)

- [ ] **About:** rubuta bayani ɗan gajere + topics: `rust`, `monitoring`, `uptime`, `observability`, `self-hosted`, `axum`, `leptos`, `postgresql`
- [ ] Kunna **Discussions** (don tambayoyi, don kada issues ya cika da su)
- [ ] Kunna **Private vulnerability reporting** (Settings → Security)
- [ ] Kunna **Dependabot alerts** da **secret scanning**
- [ ] Kariyar branch akan `main`: buƙatar PR, buƙatar CI ya wuce
- [ ] Ƙirƙiri labels: `good first issue`, `help wanted`, `bug`, `enhancement`, `documentation`
- [ ] Yi tag ɗin sakin farko: `git tag -a v0.1.0 -m "First public release" && git push --tags`
- [ ] Ƙara wasu issues masu sauƙi kuma ka yi musu alamar `good first issue` — ba tare da su ba, babu mai ba da gudummawa da zai fara

## 7. README na Hausa (ƙarin daraja)

Babban README ya kasance Turanci don isa ga duniya. Amma ƙara `README.ha.md` da hanya a saman babban README:

```md
*[Hausa](README.ha.md)*
```

Wannan ba kasafai ake ganin sa ba kuma yana sa aikin ya fito.

## 8. Tunani kan lasisi — MIT vs AGPL

Wannan shawara ce ta kasuwanci, ba ta fasaha ba, don haka kai ne ka yanke:

**MIT** (abin da `Cargo.toml` ke cewa yanzu) — kowa zai iya ɗauka, ya canza, ya sayar da shi a matsayin SaaS na kansa, ba tare da ya mayar maka da komai ba. Yana sa karɓuwa ta fi sauƙi.

**AGPL-3.0** — duk wanda ya gudanar da canjin da ya yi a matsayin sabis akan yanar gizo dole ya buga lambar sa. Wannan shi ne abin da Grafana, Sentry, da Plausible suka zaɓa saboda dalili ɗaya da ya shafe ka: kana da `billing`, `wallet`, da `pricing` a cikin repo — wato aikin da za a iya yin gogayya da shi a matsayin SaaS.

**Open core** — babban abu MIT/Apache-2.0, sassan kasuwanci (billing, SSO, agents masu yawa) a repo daban mai lasisin kasuwanci.

Idan ka yi niyyar sayar da PingVertex a matsayin sabis wata rana, **AGPL-3.0 ya fi maka aminci**. Ka yanke shawara YANZU — canza lasisi bayan mutane sun ba da gudummawa yana buƙatar izinin kowannensu.

## 9. Bayan buɗewa

- [ ] Rubuta post ɗin sanarwa (dev.to / Hashnode) kan dalilin da ya sa ka gina shi
- [ ] Aika zuwa r/rust, r/selfhosted, Hacker News (Show HN)
- [ ] Ƙara zuwa [awesome-selfhosted](https://github.com/awesome-selfhosted/awesome-selfhosted)
- [ ] Ƙara badge ɗin CI da lasisi a README (an riga an sanya, sai dai gyara `OWNER`)

---

## Taƙaitaccen hukunci

Injin aikin ya riga ya kai matsayin open source: Rust workspace mai tsarin clean architecture, 1,018 fayilolin `.rs`, 124 migrations, CI da `cargo audit` + `cargo deny`, `.env.example` mai cikakken bayani, `.gitignore` mai ƙarfi, Docker Compose na dev da na production daban, tsaron SSRF, da mTLS. Wannan ya fi yawancin repos ɗin open source ƙarfi.

Abin da ya rage shi ne **rufin gida kawai** — README, LICENSE, da fayilolin al'umma. Aiki na kwana ɗaya ko biyu, ba fiye ba.
PV_CHECKLIST
fi

head2 "2/3 · Ƙarin fayilolin da jerin aikin ya buƙata"

if [ -e CHANGELOG.md ]; then
  info "  ${c_dim}—${c_off} CHANGELOG.md ya riga ya kasance, ba a taɓa shi ba"
else
write_file CHANGELOG.md <<'PV_CHANGELOG'
# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

### Changed

### Fixed

## [0.1.0] — __RELEASE_DATE__

First public release.

### Added

- HTTP/HTTPS monitoring with configurable intervals, retries, custom headers and bodies, keyword and status-code assertions
- Multi-step API flows with value passing between steps
- Automatic incident open/close with flap protection and a durable notification outbox
- Public per-tenant status pages
- Real User Monitoring browser SDK (`sdk/rum`)
- Distributed check agents authenticated with mTLS
- Multi-tenancy with members, invitations, and role-based access enforced at the repository layer
- Scoped API keys
- Wallet, pay-as-you-go metering, and plan/pricing primitives
- Prometheus metrics, Grafana dashboards, and Alertmanager rules in the Compose stack
- Append-only tenant audit log
- SSRF and DNS-rebinding protection on every user-supplied URL

[Unreleased]: https://github.com/OWNER/pingvertex/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/OWNER/pingvertex/releases/tag/v0.1.0
PV_CHANGELOG
fi

if [ -e .github/CODEOWNERS ]; then
  info "  ${c_dim}—${c_off} .github/CODEOWNERS ya riga ya kasance, ba a taɓa shi ba"
else
write_file .github/CODEOWNERS <<'PV_CODEOWNERS'
# Default owner for everything in the repo.
* @OWNER

# Security-sensitive areas — keep these reviewed by a maintainer.
/crates/shared/src/ssrf.rs   @OWNER
/crates/domain/              @OWNER
/migrations/                 @OWNER
/.github/workflows/          @OWNER
/SECURITY.md                 @OWNER
PV_CODEOWNERS
fi

# --- Sauya OWNER da imel -------------------------------------------------------

head2 "3/3 · Sauya OWNER da adiresoshin imel"

if $DRY_RUN; then
  info "  ${c_dim}zai sauya OWNER → $OWNER${c_off}"
  info "  ${c_dim}zai sauya security@example.com → $SECURITY_EMAIL${c_off}"
  info "  ${c_dim}zai sauya conduct@example.com → $CONDUCT_EMAIL${c_off}"
else
  _owner_e="$(esc "$OWNER")"
  _sec_e="$(esc "$SECURITY_EMAIL")"
  _con_e="$(esc "$CONDUCT_EMAIL")"
  _name_e="$(esc "$COPYRIGHT_NAME")"
  _year_e="$(esc "$COPYRIGHT_YEAR")"
  _date_e="$(date +%Y-%m-%d)"

  for f in "${SUBST_FILES[@]}"; do
    [ -f "$f" ] || continue
    sed_inplace "s|OWNER|$_owner_e|g" "$f"
    sed_inplace "s|security@example\.com|$_sec_e|g" "$f"
    sed_inplace "s|conduct@example\.com|$_con_e|g" "$f"
    sed_inplace "s|__RELEASE_DATE__|$_date_e|g" "$f"
  done

  # Cire sharhin "(replace with a real address you monitor)" idan an ba da imel na gaskiya
  if [ "$SECURITY_EMAIL" != "security@example.com" ] || [ "$CONDUCT_EMAIL" != "conduct@example.com" ]; then
    for f in SECURITY.md CODE_OF_CONDUCT.md; do
      [ -f "$f" ] && sed_inplace 's| \*(replace with a real address you monitor)\*||g' "$f"
    done
  fi

  # LICENSE: sunan mai haƙƙin mallaka
  if [ -f LICENSE ]; then
    sed_inplace "s|^Copyright (c) .*|Copyright (c) $_year_e $_name_e|" LICENSE
  fi

  ok "An sauya OWNER → $OWNER"
  ok "An sauya imel na tsaro → $SECURITY_EMAIL"
  ok "An sauya imel na hali → $CONDUCT_EMAIL"
  ok "LICENSE → Copyright (c) $COPYRIGHT_YEAR $COPYRIGHT_NAME"
fi

# --- Kada a tura CHECKLIST da backup zuwa GitHub -------------------------------

if ! $DRY_RUN && [ -d .git ]; then
  _excl=".git/info/exclude"
  mkdir -p "$(dirname "$_excl")"
  touch "$_excl"
  for pat in "CHECKLIST-HAUSA.md" ".backup-opensource-*"; do
    grep -qxF "$pat" "$_excl" 2>/dev/null || printf '%s\n' "$pat" >> "$_excl"
  done
  info "  ${c_dim}An ƙara CHECKLIST-HAUSA.md da folder ɗin backup a .git/info/exclude${c_off}"
fi

# --- Taƙaitawa -----------------------------------------------------------------

head2 "An gama"

if $DRY_RUN; then
  warn "DRY RUN — ba a canza komai ba. Cire --dry-run don yin aikin."
  exit 0
fi

info "Fayilolin da aka rubuta (${#WRITTEN[@]}):"
for f in "${WRITTEN[@]}"; do printf '  %s\n' "$f"; done

if [ "$BACKED_UP" -gt 0 ]; then
  printf '\n'
  warn "An yi backup ɗin tsofaffin fayiloli $BACKED_UP a: $BACKUP_DIR"
  info "  Idan komai yana da kyau, za ka iya goge shi: rm -rf '$BACKUP_DIR'"
fi

cat <<PV_NEXT

${c_blu}Abin da ya rage — da hannu:${c_off}

  1. Duba sauran fayilolin da suka kai ga leaka bayanan kwamfutarka:
       git grep -nI "run/media\\|\$HOME\\|/home/" -- '*.toml' '*.yml' '*.yaml' '*.json'

  2. Bincika tarihin git don sirri (secrets):
       pipx install detect-secrets && detect-secrets scan --all-files
       # ko
       docker run --rm -v "\$PWD:/repo" zricethezav/gitleaks:latest detect -s /repo -v

  3. Cargo.toml → [workspace.package]: ƙara repository / homepage / keywords / rust-version
     (duba §5 na CHECKLIST-HAUSA.md)

  4. docs/: matsar HANDOVER.md da docs/GAP_*.md zuwa docs/internal/ ko goge su,
     cike ko goge docs/ROADMAP.md, sake wa ADR 0001 na biyu lamba

  5. A GitHub: kunna Discussions, private vulnerability reporting, Dependabot alerts,
     secret scanning, branch protection akan main, sannan labels + issues na
     'good first issue'

  6. Yanke shawarar lasisi (MIT vs AGPL-3.0) kafin mutane su fara ba da gudummawa
     — duba §8 na CHECKLIST-HAUSA.md

${c_blu}Duba abin da ya canza:${c_off}
  git status
  git diff
  git add -A && git commit -m "docs: add open source community files"

PV_NEXT
