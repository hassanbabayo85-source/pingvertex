# Testing PingVertex

Tests without external services:

    cargo test -p domain -p shared -p application -p api

Full suite (Postgres needed):

    docker compose up -d postgres
    export DATABASE_URL="postgres://postgres:postgres@localhost:5433/uptime_monitor_test"
    cargo test --workspace
    docker compose down

Troubleshooting:
- DATABASE_URL errors: Postgres not up, or .env missing
- Auth errors: stale volume. Fix: docker compose down -v && docker compose up -d postgres
- Port 5433 in use: stop the other process or change docker-compose.yml
