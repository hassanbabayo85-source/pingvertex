# ADR-0001: Feature-Based Architecture

## Status

Accepted

---

## Context

The project originally organized code by technical layers:

- entities
- value_objects
- repositories
- services
- use_cases

This works well for small systems but becomes difficult to maintain
as multiple business domains are introduced.

The project is expected to include:

- Authentication
- Monitoring
- Incidents
- Notifications
- Teams
- Organizations
- API Keys
- Billing
- RBAC
- Audit Logs

A layer-based structure would spread each feature across many folders,
making ownership and maintenance harder.

---

## Decision

The project will gradually migrate to a feature-based architecture.

Example:

domain/
    monitor/
    auth/
    notification/
    shared/

application/
    monitor/
    auth/
    notification/

infra/
    monitor/
    auth/

api/
    monitor/
    auth/

---

## Benefits

- Better modularity
- Easier ownership
- Smaller modules
- Reduced coupling
- Simpler testing
- Easier future expansion

---

## Consequences

Migration will happen incrementally.

Each migration step must compile successfully.

No breaking changes are allowed between steps.

---

## Security Impact

Feature isolation reduces accidental coupling
between security-sensitive code and unrelated domains.

Authentication remains isolated from monitoring logic.

---

## Decision Date

2026-08-01
