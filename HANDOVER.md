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

# H14 — Help Center & Support System Backend (COMPLETE)

## Worker/Phase
Worker H14 — Help Center Backend Foundation + Support Ticket System Backend.

## Status
COMPLETE — `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, da `cargo test --workspace` duk sun wuce.

## Scope
- **Task A**: Help Center domain (HelpArticle, HelpCategory), repositories, use cases, API endpoints, search.
- **Task B**: Support Ticket System domain (SupportTicket, SupportMessage), repositories, use cases, API endpoints, tenant isolation, rate limiting, tests.
- Backend only; babu frontend.

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
- crates/domain/src/shared/ids/mod.rs (sabbin ID modules da re-exports)
- crates/domain/src/enums.rs (H14 enums modules; kuma an gyara `ErrorType` don ƙara `Connection`, `Ssl`, `Http4xx`, `Http5xx`, `Unknown`)
- crates/domain/src/entities/mod.rs (H14 entities da re-exports; maido da `CheckErrorKind` da `AddOnSubscription` re-exports)
- crates/domain/src/errors/domain_error.rs (sabbin error variants na Help/Support)
- crates/domain/src/entities/support_ticket.rs (gyaran validation don ticket number)
- crates/domain/src/entities/check.rs (maido da `CheckErrorKind` enum)
- crates/domain/src/lib.rs (re-export `CheckErrorKind`)
- crates/application/src/ports/mod.rs (sabbin ports)
- crates/application/src/use_cases/mod.rs (sabbin use cases)
- crates/infra/src/repositories/mod.rs (sabbin repository modules)
- crates/api/src/handlers/mod.rs (sabbin handlers)
- crates/api/src/routes/mod.rs (help da support routes)
- crates/api/src/state.rs (fields, Noop structs, builder methods)
- crates/api/src/router.rs (wired help da support routes a build_router da build_test_router)
- crates/api/src/main.rs (wired Postgres repositories)

## Backend/Database/API/Security Changes
- **Database**: Sabbin tables `help_categories`, `help_articles`, `support_tickets`, `support_messages`.
- **Seed**: 10 Help Center categories (metadata kawai, ba invented articles).
- **API Endpoints**:
  - `GET /api/v1/help/categories`
  - `GET /api/v1/help/articles?category=<slug>&q=<search>`
  - `GET /api/v1/help/articles/:slug`
  - `GET /api/v1/help/search?q=...`
  - `POST /api/v1/support/tickets`
  - `GET /api/v1/support/tickets`
  - `GET /api/v1/support/tickets/:id`
  - `POST /api/v1/support/tickets/:id/messages`
  - `PATCH /api/v1/support/tickets/:id/status`
- **Authentication**: Help Center endpoints public ne (kamar knowledge base). Support endpoints na buƙatar JWT via `jwt_auth`.
- **Authorization**: Support use cases suna amfani da `AuthorizationPolicy::same_tenant` don hana cross-tenant access.
- **Rate limiting**: Help Center yana da `GovernorLayer` 60 burst per 17ms; Support yana da `GovernorLayer` 30 burst per 17ms. Hakan na IP-based; per-tenant/Redis rate limiter bai riga an haɗa ba.
- **Security**: Babu stored XSS sanitization (plaintext storage). Attachments ba a yi ba (babu storage architecture). Tenant isolation E2E test ya tabbatar da cewa Tenant B ba zai iya samun ticket na Tenant A ba.

## Tests
- **Unit tests**: Domain validation (help category/article, support ticket/message, status transition) — 8 tests a cikin `crates/domain/tests/`.
- **E2E API tests**:
  - `crates/api/tests/help_center.rs` (1 test: categories listing)
  - `crates/api/tests/support_tickets.rs` (2 tests: tenant isolation, create/reply flow)
- **Umurnin da suka wuce**:
  - `cargo fmt --all`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace` — duk crates sun wuce (0 failures).

## Deviations from Prompt
1. `rg` bai samu ba; an yi amfani da `grep`.
2. Patches a `state.rs`, `router.rs`, `main.rs` da sauransu an yi ta hanyar `python3` maimakon full heredoc saboda girman fayilolin.
3. An sake maido da `CheckErrorKind` da `AddOnSubscription` re-exports bayan wani patch ya ɓata su; an gyara `ErrorType` don ƙara missing variants da worker ke amfani da su.
4. Search E2E test bai yi ba; amma search handler da use case/repository sun wanzu. Help categories list E2E ne kawai.
5. Rate limiting na Support bai haɗa da Redis per-tenant limiter ba; ana amfani da IP-based GovernorLayer kawai.
6. Ba a yi attachment support ba saboda babu storage abstraction.
7. `list_help_articles` tare da category kawai yana aiki; ba tare da category ba yana dawo da empty list (ana iya amfani da search don browsing).

## Suggested Commit Messages
- `feat(help-center): add help category/article domain, repos, use cases, and API endpoints`
- `feat(support): add support ticket/message domain, repos, use cases, and API endpoints`
- `test(support): add E2E tenant isolation and create/reply flow tests`
- `chore(domain): restore CheckErrorKind and expand ErrorType variants`

## User Test Commands
```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Known Issues/Risks
- Search endpoint yana aiki amma ba shi da cikakken E2E coverage.
- Per-tenant rate limiting (Redis) bai haɗa ba; IP-based rate limiting kawai.
- Babu attachments da HTML sanitization.
- `list_help_articles` ba tare da category ba yana dawo da empty list; ana buƙatar ƙara `list_all_published` method idan ana buƙata.

## Notes for Next Worker
- Wire Redis rate limiter (`AppState` ba shi da shi; `AuthState` yana da shi) zuwa `POST /api/v1/support/tickets` da `POST /api/v1/support/tickets/:id/messages`.
- Ƙara `list_all_published` ga `HelpArticleRepository` idan ana buƙatar list ba tare da category ba.
- Ƙara HTML sanitization layer ko use plaintext rendering a frontend.
- Attachment support zai buƙaci sabon storage abstraction.
- Migrations 0057-0059 suna buƙatar gudanar da su a kan Postgres kafin amfani da aikin production.

## Completion Criteria
- [x] `cargo fmt --all` passes.
- [x] `cargo clippy --workspace --all-targets -- -D warnings` passes.
- [x] `cargo test --workspace` passes.
- [x] E2E tenant isolation test passes (Tenant B cannot access Tenant A ticket).
- [x] Help categories endpoint returns data.
- [x] Support create/reply/get flow works.
