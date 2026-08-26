#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(pwd)"
BACKUP_DIR=".architecture-security-backup-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$BACKUP_DIR"
echo "Creating backup in $BACKUP_DIR"

mkdir -p "$BACKUP_DIR/$(dirname crates/api/src/handlers/current_session.rs)"
if [ -e "crates/api/src/handlers/current_session.rs" ]; then cp -a "crates/api/src/handlers/current_session.rs" "$BACKUP_DIR/crates/api/src/handlers/current_session.rs"; fi
mkdir -p "$(dirname "crates/api/src/handlers/current_session.rs")"
cat > 'crates/api/src/handlers/current_session.rs' <<'EOF_crates_api_src_handlers_current_session_rs'
use axum::{extract::{Extension, State}, Json};
use serde::Serialize;

use application::ports::UserRepository;
use domain::shared::types::AuthenticatedUser;

use crate::{auth_state::AuthState, error::ApiError};

#[derive(Debug, Serialize)]
pub struct CurrentSessionResponse {
    pub authenticated: bool,
    pub user: CurrentSessionUser,
}

#[derive(Debug, Serialize)]
pub struct CurrentSessionUser {
    pub id: String,
    pub email: String,
    pub username: String,
    pub email_verified: bool,
    pub tenant_id: String,
    pub role: domain::shared::types::Role,
}

/// `GET /api/v1/auth/me`
///
/// The server is the source of truth for browser authentication. The JWT is
/// intentionally kept in an HttpOnly cookie and is never exposed to
/// JavaScript. This endpoint validates the session through `jwt_auth` and
/// returns only the non-sensitive identity information required by the UI.
pub async fn current_session(
    State(state): State<AuthState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
) -> Result<Json<CurrentSessionResponse>, ApiError> {
    let user_id = authenticated_user.user_id();
    let user = state
        .user_repository
        .find_by_id(&user_id)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to load current user", err))?
        .ok_or_else(|| ApiError::unauthorized("invalid session"))?;

    Ok(Json(CurrentSessionResponse {
        authenticated: true,
        user: CurrentSessionUser {
            id: user.id().to_string(),
            email: user.email().to_string(),
            username: user.username().to_string(),
            email_verified: user.is_email_verified(),
            tenant_id: authenticated_user.tenant_context().tenant_id().to_string(),
            role: authenticated_user.role(),
        },
    }))
}
EOF_crates_api_src_handlers_current_session_rs

mkdir -p "$BACKUP_DIR/$(dirname crates/api/src/handlers/mod.rs)"
if [ -e "crates/api/src/handlers/mod.rs" ]; then cp -a "crates/api/src/handlers/mod.rs" "$BACKUP_DIR/crates/api/src/handlers/mod.rs"; fi
mkdir -p "$(dirname "crates/api/src/handlers/mod.rs")"
cat > 'crates/api/src/handlers/mod.rs' <<'EOF_crates_api_src_handlers_mod_rs'
pub mod acknowledge_incident;
pub mod api_key_types;
pub mod change_password;
pub mod common;
pub mod confirm_totp;
pub mod create_api_flow;
pub mod create_api_key;
pub mod create_escalation_policy;
pub mod create_maintenance_window;
pub mod create_monitor;
pub mod create_notification_channel;
pub mod create_status_page;
pub mod current_session;
pub mod delete_api_flow;
pub mod delete_api_key;
pub mod delete_monitor;
pub mod delete_notification_channel;
pub mod delete_status_page;
pub mod disable_monitor;
pub mod disable_totp;
pub mod enable_monitor;
pub mod enable_totp;
pub mod external_checks;
pub mod external_incidents;
pub mod external_monitors;
pub mod external_status_pages;
pub mod feature_types;
pub mod flow_types;
pub mod forgot_password;
pub mod get_api_flows;
pub mod get_monitor;
pub mod get_proxy_check_job;
pub mod get_public_status;
pub mod get_public_status_page;
pub mod health;
pub mod heartbeat;
pub mod list_api_keys;
pub mod list_checks;
pub mod list_incidents;
pub mod list_monitor_checks;
pub mod list_monitors;
pub mod list_notification_channels;
pub mod list_status_pages;
pub mod login;
pub mod logout;
pub mod proxy_check_types;
pub mod public_status_page;
pub mod register;
pub mod remove_custom_domain;
pub mod request_custom_domain;
pub mod resend_verification;
pub mod reset_password;
pub mod submit_proxy_check_job;
pub mod test_notification_channel;
pub mod types;
pub mod update_api_flow;
pub mod update_monitor;
pub mod update_status_page;
pub mod verify_email;
pub mod verify_totp;

// B8 — membership management:
pub mod add_member;
pub mod change_member_role;
pub mod list_members;
pub mod members_types;
pub mod remove_member;

// B9 — tenant invitations:
pub mod accept_invitation;
pub mod create_invitation;
pub mod invitation_types;
pub mod list_invitations;
pub mod revoke_invitation;

pub use acknowledge_incident::*;
pub use api_key_types::*;
pub use change_password::*;
pub use create_api_flow::*;
pub use create_api_key::*;
pub use create_escalation_policy::*;
pub use create_maintenance_window::*;
pub use create_monitor::*;
pub use create_notification_channel::*;
pub use create_status_page::*;
pub use current_session::*;
pub use delete_api_flow::*;
pub use delete_api_key::*;
pub use delete_monitor::*;
pub use delete_notification_channel::*;
pub use delete_status_page::*;
pub use disable_monitor::*;
pub use enable_monitor::*;
pub use external_checks::*;
pub use external_incidents::*;
pub use external_monitors::*;
pub use external_status_pages::*;
pub use feature_types::*;
pub use forgot_password::*;
pub use get_api_flows::*;
pub use get_monitor::*;
pub use get_proxy_check_job::*;
pub use get_public_status::*;
pub use get_public_status_page::*;
pub use health::*;
pub use heartbeat::*;
pub use list_api_keys::*;
pub use list_checks::*;
pub use list_incidents::*;
pub use list_monitor_checks::*;
pub use list_monitors::*;
pub use list_notification_channels::*;
pub use list_status_pages::*;
pub use public_status_page::*;
pub use register::*;
pub use remove_custom_domain::*;
pub use request_custom_domain::*;
pub use resend_verification::*;
pub use reset_password::*;
pub use submit_proxy_check_job::*;
pub use test_notification_channel::*;
pub use update_api_flow::*;
pub use update_monitor::*;
pub use update_status_page::*;
pub use verify_email::*;
pub mod upload_status_page_logo;
pub use upload_status_page_logo::*;

pub use add_member::*;
pub use change_member_role::*;
pub use list_members::*;
pub use members_types::*;
pub use remove_member::*;

pub use accept_invitation::*;
pub use create_invitation::*;
pub use invitation_types::*;
pub use list_invitations::*;
pub use revoke_invitation::*;

// B14 — Billing Provider Abstraction & Checkout:
pub mod billing_types;
pub mod initiate_checkout;
pub mod verify_checkout;
pub use billing_types::*;
pub use initiate_checkout::*;
pub use verify_checkout::*;

// B15 — Billing Webhooks:
pub mod billing_webhook;
pub use billing_webhook::*;
EOF_crates_api_src_handlers_mod_rs

mkdir -p "$BACKUP_DIR/$(dirname crates/api/src/routes/auth.rs)"
if [ -e "crates/api/src/routes/auth.rs" ]; then cp -a "crates/api/src/routes/auth.rs" "$BACKUP_DIR/crates/api/src/routes/auth.rs"; fi
mkdir -p "$(dirname "crates/api/src/routes/auth.rs")"
cat > 'crates/api/src/routes/auth.rs' <<'EOF_crates_api_src_routes_auth_rs'
use axum::{Router, routing::{get, post}};

use crate::handlers::{
    change_password::change_password, confirm_totp::confirm_totp,
    current_session::current_session, disable_totp::disable_totp,
    enable_totp::enable_totp, forgot_password::forgot_password, login::login,
    logout::logout, register::register, resend_verification::resend_verification,
    reset_password::reset_password, verify_email::verify_email, verify_totp::verify_totp,
};

pub fn router() -> Router<crate::auth_state::AuthState> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/verify-email", post(verify_email))
        .route("/resend-verification", post(resend_verification))
        .route("/forgot-password", post(forgot_password))
        .route("/reset-password", post(reset_password))
}

pub fn totp_verify_router() -> Router<crate::auth_state::AuthState> {
    Router::new().route("/totp/verify", post(verify_totp))
}

/// Authentication-protected account endpoints. `jwt_auth` is applied by the
/// top-level router before this router is mounted.
pub fn protected_router() -> Router<crate::auth_state::AuthState> {
    Router::new()
        .route("/me", get(current_session))
        .route("/totp/enable", post(enable_totp))
        .route("/totp/confirm", post(confirm_totp))
        .route("/totp/disable", post(disable_totp))
        .route("/password/change", post(change_password))
}
EOF_crates_api_src_routes_auth_rs





mkdir -p "$BACKUP_DIR/$(dirname docker-compose.yml)"
if [ -e "docker-compose.yml" ]; then cp -a "docker-compose.yml" "$BACKUP_DIR/docker-compose.yml"; fi
mkdir -p "$(dirname "docker-compose.yml")"
cat > 'docker-compose.yml' <<'EOF_docker-compose_yml'
services:
  postgres:
    image: postgres:16
    container_name: uptime-monitor-postgres
    restart: unless-stopped
    environment:
      POSTGRES_USER: postgres
      # Dev-only default. Override via `POSTGRES_PASSWORD=... docker compose up`,
      # or (better) set it in `.env` — see docker-compose.prod.yml for a
      # production override that refuses to start without one.
      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD:-postgres}
      POSTGRES_DB: uptime_monitor
    ports:
      # Bound to localhost only: reachable from the host machine (e.g.
      # `psql -h localhost -p 5433`) but never from the network — a bare
      # `- "5433:5432"` publishes on every interface (0.0.0.0), which is
      # what let this be reached directly from outside the host in the
      # first place.
      - "127.0.0.1:5433:5432"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U postgres -d uptime_monitor"]
      interval: 10s
      timeout: 5s
      retries: 10
    volumes:
      - postgres_data:/var/lib/postgresql/data

  migrate:
    container_name: uptime-monitor-migrate
    working_dir: /app
    depends_on:
      postgres:
        condition: service_healthy
    environment:
      DATABASE_URL: postgres://postgres:${POSTGRES_PASSWORD:-postgres}@postgres:5432/uptime_monitor
    volumes:
      - .:/app
    command: >
      bash -c "
      export PATH=/usr/local/cargo/bin:/root/.cargo/bin:$PATH &&
      apt-get update &&
      apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates &&
      cargo install sqlx-cli --no-default-features --features postgres &&
      sqlx database create || true &&
      sqlx migrate run --source migrations
      "

  api:
    build:
      context: .
      dockerfile: Dockerfile.api
    container_name: uptime-monitor-api
    restart: unless-stopped
    depends_on:
      postgres:
        condition: service_healthy
      migrate:
        condition: service_completed_successfully
    env_file:
      - .env
    environment:
      DATABASE_URL: postgres://postgres:${POSTGRES_PASSWORD:-postgres}@postgres:5432/uptime_monitor
      RUST_LOG: info
    ports:
      # B38: plain-HTTP port bound to localhost only, same reasoning as
      # postgres/prometheus/grafana/alertmanager below — this is now
      # reached through the `caddy` reverse proxy in
      # docker-compose.prod.yml (public 80/443) or an SSH tunnel for
      # local debugging, never directly from the network. The mTLS port
      # (3443) stays published on every interface below: that listener
      # is internet-facing BY DESIGN, since remote monitoring agents
      # connect to it directly (see crates/api/src/mtls.rs) and every
      # connection requires a valid client certificate.
      - "127.0.0.1:3000:3000"
      - "3443:3443"

  worker:
    build:
      context: .
      dockerfile: Dockerfile.worker
    container_name: uptime-monitor-worker
    restart: unless-stopped
    depends_on:
      postgres:
        condition: service_healthy
      migrate:
        condition: service_completed_successfully
    # Added for the notifications feature: the worker is what actually
    # sends email/webhook notifications when an incident opens or
    # resolves (see crates/worker/src/incident_manager.rs), so it needs
    # the same SMTP_* variables as the api service. Previously only `api`
    # read `.env` here — without this, the worker would silently run with
    # email notifications disabled (falls back to
    # `DisabledEmailNotificationService`, logs a warning) even if SMTP_*
    # was correctly set in `.env`.
    env_file:
      - .env
    environment:
      DATABASE_URL: postgres://postgres:${POSTGRES_PASSWORD:-postgres}@postgres:5432/uptime_monitor
      RUST_LOG: info
    ports:
      # B38: bound to localhost only — Prometheus scrapes this over the
      # internal Compose network regardless of host port publishing;
      # this binding only controls host-machine/network reachability.
      # Unlike the api's /metrics, this endpoint has no bearer-token
      # check of its own (see this worker's HANDOVER.md "Known Risks"),
      # so keeping it off the public interface is the only thing making
      # it "private" right now.
      - "127.0.0.1:3001:3001"


  prometheus:
    image: prom/prometheus:v2.54.1
    container_name: uptime-monitor-prometheus
    restart: unless-stopped
    depends_on:
      - api
      - worker
    volumes:
      - ./monitoring/prometheus.yml:/etc/prometheus/prometheus.yml:ro
      - ./monitoring/rules.yml:/etc/prometheus/rules.yml
    command:
      - --config.file=/etc/prometheus/prometheus.yml
    ports:
      # Localhost-only — see the postgres comment above. Prometheus has no
      # authentication of its own, so a bare host-wide publish exposes
      # every metric (monitor names, request patterns, user counts) to
      # anyone who can reach the host on this port.
      - "127.0.0.1:9090:9090"

  grafana:
    image: grafana/grafana-oss:11.2.0
    container_name: uptime-monitor-grafana
    restart: unless-stopped
    depends_on:
      - prometheus
    environment:
      GF_SECURITY_ADMIN_USER: admin
      # Dev-only default — change this (or use docker-compose.prod.yml,
      # which requires it to be set explicitly) before running anywhere
      # reachable outside your own machine. `admin`/`admin` is the first
      # thing any scanner tries.
      GF_SECURITY_ADMIN_PASSWORD: ${GF_SECURITY_ADMIN_PASSWORD:-admin}
    ports:
      - "127.0.0.1:3002:3000"

  alertmanager:
    image: prom/alertmanager:v0.27.0
    container_name: uptime-monitor-alertmanager
    restart: unless-stopped

    command:
      - "--config.file=/etc/alertmanager/alertmanager.yml"

    volumes:
      - ./monitoring/alertmanager.yml:/etc/alertmanager/alertmanager.yml

    ports:
      - "127.0.0.1:9093:9093"

  agent:
    build:
      context: .
      dockerfile: Dockerfile.agent
    container_name: uptime-monitor-agent
    restart: unless-stopped
    depends_on:
      api:
        condition: service_started
    env_file:
      - .env
    environment:
      AGENT_API_BASE_URL: https://api:3443/internal/v1
      RUST_LOG: info

volumes:
  postgres_data:
EOF_docker-compose_yml

mkdir -p "$BACKUP_DIR/$(dirname docker-compose.e2e.yml)"
if [ -e "docker-compose.e2e.yml" ]; then cp -a "docker-compose.e2e.yml" "$BACKUP_DIR/docker-compose.e2e.yml"; fi
mkdir -p "$(dirname "docker-compose.e2e.yml")"
cat > 'docker-compose.e2e.yml' <<'EOF_docker-compose_e2e_yml'
# E2E test overlay for docker-compose.yml (Worker B39).
# - migrate: image added to satisfy compose validation, command no-ops.
# - api/worker: SMTP/BILLING configured for Mailpit/fake billing.
# - worker: PINGVERTEX_SSRF_ALLOW_PRIVATE=true (test-only).
services:
  migrate:
    image: alpine:3.20
    command: ["echo", "e2e skip migrate"]

  mock-target:
    image: nginx:alpine
    container_name: uptime-monitor-e2e-mock-target
    restart: "no"

  mailpit:
    image: axllent/mailpit:v1.20.3
    container_name: uptime-monitor-e2e-mailpit
    restart: "no"
    ports:
      - "127.0.0.1:8025:8025"

  api:
    environment:
      SMTP_HOST: mailpit
      SMTP_PORT: "1025"
      SMTP_USERNAME: ""
      SMTP_PASSWORD: ""
      SMTP_FROM_ADDRESS: alerts@e2e.invalid
      SMTP_FROM_NAME: PingVertex E2E
      SMTP_STARTTLS: "false"
      SMTP_ALLOW_EMPTY_CREDENTIALS: "true"
      BILLING_PROVIDER: fake
      BILLING_WEBHOOK_SECRET: ${BILLING_WEBHOOK_SECRET:-e2e-test-webhook-secret}

  worker:
    environment:
      INCIDENT_FAILURE_THRESHOLD: "1"
      INCIDENT_RECOVERY_THRESHOLD: "1"
      SMTP_HOST: mailpit
      SMTP_PORT: "1025"
      SMTP_USERNAME: ""
      SMTP_PASSWORD: ""
      SMTP_FROM_ADDRESS: alerts@e2e.invalid
      SMTP_FROM_NAME: PingVertex E2E
      SMTP_STARTTLS: "false"
      SMTP_ALLOW_EMPTY_CREDENTIALS: "true"
      PINGVERTEX_SSRF_ALLOW_PRIVATE: "true"
EOF_docker-compose_e2e_yml

mkdir -p "$BACKUP_DIR/$(dirname .github/workflows/ci.yml)"
if [ -e ".github/workflows/ci.yml" ]; then cp -a ".github/workflows/ci.yml" "$BACKUP_DIR/.github/workflows/ci.yml"; fi
mkdir -p "$(dirname ".github/workflows/ci.yml")"
cat > '.github/workflows/ci.yml' <<'EOF__github_workflows_ci_yml'
name: Rust CI

on:
  push:
    branches:
      - main
      - master
      - develop
  pull_request:

env:
  CARGO_TERM_COLOR: always

jobs:
  test:
    runs-on: ubuntu-latest

    services:
      postgres:
        image: postgres:16

        env:
          POSTGRES_USER: postgres
          POSTGRES_PASSWORD: postgres
          POSTGRES_DB: uptime_monitor_test

        ports:
          - 5432:5432

        options: >-
          --health-cmd "pg_isready -U postgres"
          --health-interval 10s
          --health-timeout 5s
          --health-retries 10

    env:
      DATABASE_URL: postgres://postgres:postgres@localhost:5432/uptime_monitor_test

    steps:
      - name: Checkout repository
        uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Cache Cargo
        uses: Swatinem/rust-cache@v2

      - name: Install sqlx-cli
        run: cargo install sqlx-cli --no-default-features --features postgres

      - name: Run database migrations
        run: sqlx migrate run

      - name: Check formatting
        run: cargo fmt --all -- --check

      - name: Run Clippy
        run: cargo clippy --workspace --all-targets -- -D warnings

      - name: Run Tests
        run: cargo test --workspace

  security:
    runs-on: ubuntu-latest
    steps:
      - name: Checkout repository
        uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Install cargo-audit
        run: cargo install cargo-audit --locked

      - name: Audit Rust dependencies
        run: cargo audit

      - name: Install cargo-deny
        run: cargo install cargo-deny --locked

      - name: Dependency policy check
        run: |
          if [ -f deny.toml ]; then cargo deny check; else echo "deny.toml not present; skipping policy check"; fi

      - name: Secret scan
        uses: gitleaks/gitleaks-action@v2
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
EOF__github_workflows_ci_yml

mkdir -p "$BACKUP_DIR/$(dirname docs/ARCHITECTURE.md)"
if [ -e "docs/ARCHITECTURE.md" ]; then cp -a "docs/ARCHITECTURE.md" "$BACKUP_DIR/docs/ARCHITECTURE.md"; fi
mkdir -p "$(dirname "docs/ARCHITECTURE.md")"
cat > 'docs/ARCHITECTURE.md' <<'EOF_docs_ARCHITECTURE_md'
# PingVertex Architecture

## System context

The platform is a multi-tenant uptime monitoring service. Browser clients call the API through the dashboard, while workers and mTLS-authenticated agents execute checks and persist results. PostgreSQL is the system of record; Redis is used for ephemeral coordination and caching when configured.

```text
Dashboard -> API -> Application -> Domain
                 -> Infrastructure -> PostgreSQL
Worker/Agents -> Check execution -> Results -> Incident evaluation -> Outbox -> Notifications
```

## Dependency rule

`domain` has no dependency on API, infrastructure, or transport. `application` depends on domain and port abstractions. `infra` implements ports. `api`, `worker`, and `agent` are delivery/runtime layers.

## Authentication

JWTs are stored only in Secure, HttpOnly, SameSite=Strict cookies. The dashboard never stores an access token or an authentication flag in localStorage. On startup it calls `GET /api/v1/auth/me`; the server validates the JWT and tenant membership and returns non-sensitive session metadata.

## Multi-tenancy

Every tenant-aware operation must receive an `AuthenticatedUser` containing a `TenantContext`. Repository queries must scope reads and writes to the tenant context. Authorization tests must include cross-tenant negative cases.

## Reliability

Schedulers use durable state and lease/claim patterns. Incident-triggered notifications use an outbox so database state and delivery intent remain durable across process failures. Retries must be idempotent.

## Security boundaries

- User-controlled network targets pass SSRF and DNS-rebinding checks.
- Proxy credentials are encrypted at rest.
- Internal agents authenticate through mTLS.
- Public, authenticated, API-key, and internal-agent routes are separate boundaries.
- Metrics should remain on internal networks or require the configured metrics authorization.

## Operational requirements

Production deployments should pin container image versions, run processes as non-root, use explicit CORS origins, rotate credentials, and run dependency/secret scanning in CI.
EOF_docs_ARCHITECTURE_md

echo "Patch applied successfully."
echo "Backup: $BACKUP_DIR"

if command -v cargo >/dev/null 2>&1; then
  echo "Running rustfmt..."
  cargo fmt --all
  echo "Running cargo check..."
  cargo check --workspace
else
  echo "cargo not found; patch applied, but Rust validation was skipped."
fi

