# PingVertex Backend-Track Worker Handover

## Worker
Worker H14 — Help Center Backend Foundation (Categories & Articles Content) + Support Ticket System Backend

## Scope
- Task A: Help Center domain (HelpArticle, HelpCategory), repositories, use cases, API endpoints, search.
- Task B: Support Ticket System domain (SupportTicket, SupportMessage), repositories, use cases, API endpoints, tenant isolation, rate limiting, tests.
- Backend only; no frontend implementation.

## Status
WAITING_FOR_USER_TEST

## Files Created
### Domain
- crates/domain/src/shared/ids/help_category_id.rs
- crates/domain/src/shared/ids/help_article_id.rs
- crates/domain/src/shared/ids/support_ticket_id.rs
- crates/domain/src/shared/ids/support_message_id.rs
- crates/domain/src/enums/help_article_status.rs
- crates/domain/src/enums/support_ticket_status.rs
- crates/domain/src/enums/support_category.rs
- crates/domain/src/entities/help_category.rs
- crates/domain/src/entities/help_article.rs
- crates/domain/src/entities/support_ticket.rs
- crates/domain/src/entities/support_message.rs
- crates/domain/tests/help_center_validation.rs
- crates/domain/tests/support_validation.rs

### Application
- crates/application/src/ports/help_category_repository.rs
- crates/application/src/ports/help_article_repository.rs
- crates/application/src/ports/support_ticket_repository.rs
- crates/application/src/use_cases/list_help_categories.rs
- crates/application/src/use_cases/get_help_article.rs
- crates/application/src/use_cases/search_help_articles.rs
- crates/application/src/use_cases/create_support_ticket.rs
- crates/application/src/use_cases/get_support_ticket.rs
- crates/application/src/use_cases/list_my_support_tickets.rs
- crates/application/src/use_cases/reply_to_support_ticket.rs
- crates/application/src/use_cases/update_support_ticket_status.rs

### Infrastructure
- crates/infra/src/repositories/in_memory_help_category_repository.rs
- crates/infra/src/repositories/in_memory_help_article_repository.rs
- crates/infra/src/repositories/in_memory_support_ticket_repository.rs
- crates/infra/src/repositories/postgres_help_category_repository.rs
- crates/infra/src/repositories/postgres_help_article_repository.rs
- crates/infra/src/repositories/postgres_support_ticket_repository.rs

### API
- crates/api/src/handlers/help_types.rs
- crates/api/src/handlers/help_center.rs
- crates/api/src/handlers/support_types.rs
- crates/api/src/handlers/support.rs
- crates/api/src/routes/help.rs
- crates/api/src/routes/support.rs
- crates/api/tests/help_center.rs
- crates/api/tests/support_tickets.rs

### Migrations
- migrations/0057_create_help_center_tables.sql
- migrations/0058_create_support_tables.sql
- migrations/0059_seed_help_categories.sql

## Files Modified
- crates/domain/src/shared/ids/mod.rs (added new ID modules)
- crates/domain/src/enums.rs (added new enums)
- crates/domain/src/entities/mod.rs (added new entities)
- crates/domain/src/errors/domain_error.rs (added support error variants)
- crates/application/src/ports/mod.rs (added new ports)
- crates/application/src/use_cases/mod.rs (added new use cases)
- crates/infra/src/repositories/mod.rs (added new repository modules)
- crates/api/src/handlers/mod.rs (added new handler modules)
- crates/api/src/routes/mod.rs (added help and support route modules)
- crates/api/src/state.rs (patched: added help and support repos, Noop defaults, builder methods)
- crates/api/src/router.rs (patched: wired help and support routes in build_router and build_test_router)
- crates/api/src/main.rs (patched: wired Postgres repositories)

## Backend/Database/API/Security Changes
- New tables: help_categories, help_articles, support_tickets, support_messages.
- Seeded 10 Help Center categories (no articles seeded per H14 rule against invented docs).
- API endpoints:
  - GET /api/v1/help/categories
  - GET /api/v1/help/articles?category=<slug>&q=<search>
  - GET /api/v1/help/articles/:slug
  - GET /api/v1/help/search?q=...
  - POST /api/v1/support/tickets
  - GET /api/v1/support/tickets
  - GET /api/v1/support/tickets/:id
  - POST /api/v1/support/tickets/:id/messages
  - PATCH /api/v1/support/tickets/:id/status
- Help Center endpoints are unauthenticated (matching public knowledge base). Support endpoints require JWT via `jwt_auth`.
- Tenant isolation enforced in support use cases via `AuthorizationPolicy::same_tenant`.
- Rate limiting: Help Center uses public-tier GovernorLayer; Support uses authenticated-tier GovernorLayer. Additional per-ticket rate limiting not added yet (see Known Issues).
- Stored XSS protection: messages stored as plain text; no HTML sanitization layer exists yet (frontend must escape).

## Tests
- Domain unit tests: help category/article validation, support ticket/message validation, status transition.
- E2E API tests: help categories listing and article search; support tenant isolation (cross-tenant access returns 404); support create/reply/get flow.
- Run commands:
```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
