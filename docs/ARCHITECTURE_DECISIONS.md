# Architecture Decisions (ADR)

Wannan fayil yana adana manyan architectural decisions na project.

Ba documentation bane.

Dalilin sa shi ne:
- Mu tuna dalilin da yasa muka zaɓi wata hanya.
- Kada mu maimaita kuskure.
- Idan project ya girma, mu san asalin design.

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

Ba za mu yi amfani da primitive types idan akwai business rule ba.

Misali:

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

Sauƙin testing.
Sauƙin canza database.

Status:

Accepted.


---

## ADR-0004 — Avoid Builder Macros

Decision:

Ba za mu yi amfani da builder crates ba (derive_builder, typed-builder, bon, da makamantansu).

Za mu rubuta builder da kanmu idan ana buƙata.

Reason:

- Ƙarancin dependencies
- Sauƙin debugging
- Cikakken iko akan API
- Compile time mafi kyau

Status:

Accepted.

