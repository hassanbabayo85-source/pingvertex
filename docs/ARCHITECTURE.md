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
