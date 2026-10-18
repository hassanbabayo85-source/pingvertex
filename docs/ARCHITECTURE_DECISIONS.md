# Architecture Decisions (ADR)

This file records the project's significant architectural decisions.

It is not general documentation.

Its purpose is to:
- Remember why a particular approach was chosen.
- Avoid repeating past mistakes.
- Preserve the original design rationale as the project grows.

---

## ADR-0001 — Clean Architecture

Decision:
- Domain ba zai dogara da Database ba.
- Domain ba zai dogara da HTTP framework ba.
- Domain ba zai dogara da Email provider ba.

Reason:
Business logic ya kasance independent.

Status:
Accepted.

---

## ADR-0002 — Value Objects

Decision:

We do not use primitive types where a business rule applies.

Example:

String  -> MonitorName
String  -> Url
u32     -> CheckInterval
u32     -> Timeout
u16     -> ExpectedStatus

Reason:

Validation ta kasance a wuri guda.

Status:

Accepted.

---

## ADR-0003 — Repository Pattern

Decision:

Domain zai yi magana da Traits kawai.

Infrastructure zai yi implementation.

Reason:

Easier testing.
Easier to swap the database.

Status:

Accepted.


---

## ADR-0004 — Avoid Builder Macros

Decision:

We do not use builder crates (derive_builder, typed-builder, bon, and similar).

We write builders by hand when one is needed.

Reason:

- Fewer dependencies
- Easier debugging
- Full control over the API
- Better compile times

Status:

Accepted.

