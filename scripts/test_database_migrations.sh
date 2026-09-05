#!/usr/bin/env bash
set -euo pipefail

: "${DATABASE_URL:?DATABASE_URL must be set}"

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

command -v sqlx >/dev/null || { echo "sqlx CLI is required"; exit 1; }
command -v psql >/dev/null || { echo "psql is required"; exit 1; }

echo "[1/2] Fresh database: drop -> create -> 0001 -> latest"
sqlx database drop -y
sqlx database create
sqlx migrate run --source migrations

psql "$DATABASE_URL" -v ON_ERROR_STOP=1 <<'SQL'
DO $$
DECLARE
    missing text;
BEGIN
    SELECT string_agg(c, ', ' ORDER BY c)
    INTO missing
    FROM unnest(ARRAY[
        'id','user_id','tenant_id','status','created_at','completed_at',
        'failed_at','failure_reason','total_count','checked_count',
        'successful_count','failed_count'
    ]) AS c
    WHERE NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema='public'
          AND table_name='proxy_check_jobs'
          AND column_name=c
    );

    IF missing IS NOT NULL THEN
        RAISE EXCEPTION 'proxy_check_jobs missing columns: %', missing;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_indexes
        WHERE schemaname='public' AND indexname='proxy_check_jobs_tenant_idx'
    ) THEN RAISE EXCEPTION 'missing proxy_check_jobs_tenant_idx'; END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_indexes
        WHERE schemaname='public' AND indexname='proxy_check_jobs_status_idx'
    ) THEN RAISE EXCEPTION 'missing proxy_check_jobs_status_idx'; END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_indexes
        WHERE schemaname='public' AND indexname='proxy_check_jobs_user_created_idx'
    ) THEN RAISE EXCEPTION 'missing proxy_check_jobs_user_created_idx'; END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conrelid='proxy_check_jobs'::regclass
          AND conname='fk_proxy_check_jobs_tenant'
    ) THEN RAISE EXCEPTION 'missing tenant foreign key'; END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conrelid='proxy_check_jobs'::regclass
          AND conname='fk_proxy_check_jobs_tenant_user'
    ) THEN RAISE EXCEPTION 'missing tenant/user integrity foreign key'; END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conrelid='proxy_check_jobs'::regclass
          AND conname='chk_proxy_check_jobs_status'
    ) THEN RAISE EXCEPTION 'missing proxy status check'; END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_trigger
        WHERE tgrelid='proxy_check_jobs'::regclass
          AND tgname='trg_proxy_check_jobs_status_transition'
    ) THEN RAISE EXCEPTION 'missing proxy state-machine trigger'; END IF;
END $$;

DO $$
BEGIN
    IF (SELECT COUNT(*) FROM _sqlx_migrations WHERE success) <> 76 THEN
        RAISE EXCEPTION 'expected 76 successful migrations, got %',
            (SELECT COUNT(*) FROM _sqlx_migrations WHERE success);
    END IF;
END $$;

-- Prove the DB state machine rejects resurrection and invalid jumps.
DO $$
DECLARE
    u uuid := gen_random_uuid();
    t uuid := gen_random_uuid();
    completed_job uuid := gen_random_uuid();
    failed_job uuid := gen_random_uuid();
    pending_job uuid := gen_random_uuid();
BEGIN
    INSERT INTO users(id,email,password_hash,created_at,updated_at)
    VALUES (u, 'worker1-state-test@example.invalid', 'x', now(), now());
    INSERT INTO tenants(id,name,owner_user_id) VALUES (t, 'worker1-state-test', u);
    INSERT INTO tenant_memberships(tenant_id,user_id,role,joined_at)
    VALUES (t,u,'Owner',now());

    INSERT INTO proxy_check_jobs(id,user_id,tenant_id,status,total_count)
    VALUES (completed_job,u,t,'pending',1), (failed_job,u,t,'pending',1), (pending_job,u,t,'pending',1);

    UPDATE proxy_check_jobs SET status='running' WHERE id IN (completed_job,failed_job);
    UPDATE proxy_check_jobs SET status='completed', completed_at=now() WHERE id=completed_job;
    UPDATE proxy_check_jobs
    SET status='failed', failed_at=now(), failure_reason='fixture'
    WHERE id=failed_job;

    BEGIN
        UPDATE proxy_check_jobs SET status='running' WHERE id=completed_job;
        RAISE EXCEPTION 'completed -> running was allowed';
    EXCEPTION WHEN check_violation THEN NULL;
    END;

    BEGIN
        UPDATE proxy_check_jobs SET status='pending' WHERE id=completed_job;
        RAISE EXCEPTION 'completed -> pending was allowed';
    EXCEPTION WHEN check_violation THEN NULL;
    END;

    BEGIN
        UPDATE proxy_check_jobs SET status='running' WHERE id=failed_job;
        RAISE EXCEPTION 'failed -> running was allowed';
    EXCEPTION WHEN check_violation THEN NULL;
    END;

    BEGIN
        UPDATE proxy_check_jobs SET status='completed', completed_at=now() WHERE id=pending_job;
        RAISE EXCEPTION 'pending -> completed was allowed';
    EXCEPTION WHEN check_violation THEN NULL;
    END;
END $$;
SQL

echo "[2/2] Existing-schema upgrade: 0001 -> 0072, fixture, then latest"
UPGRADE_DATABASE_URL="${UPGRADE_DATABASE_URL:-${DATABASE_URL%/*}/uptime_monitor_upgrade}"
export UPGRADE_DATABASE_URL

sqlx database drop -y --database-url "$UPGRADE_DATABASE_URL" 2>/dev/null || true
sqlx database create --database-url "$UPGRADE_DATABASE_URL"

TMP_MIGRATIONS="$(mktemp -d)"
trap 'rm -rf "$TMP_MIGRATIONS"' EXIT
find migrations -maxdepth 1 -type f -name '*.sql' | sort | head -n 72 |
    while IFS= read -r migration; do cp "$migration" "$TMP_MIGRATIONS/"; done

sqlx migrate run --source "$TMP_MIGRATIONS" --database-url "$UPGRADE_DATABASE_URL"

psql "$UPGRADE_DATABASE_URL" -v ON_ERROR_STOP=1 <<'SQL'
INSERT INTO users(id,email,password_hash,created_at,updated_at)
VALUES ('10000000-0000-0000-0000-000000000001',
        'worker1-upgrade@example.invalid', 'fixture-hash', now(), now());

INSERT INTO tenants(id,name,owner_user_id)
VALUES ('20000000-0000-0000-0000-000000000001',
        'Worker 1 Upgrade Tenant',
        '10000000-0000-0000-0000-000000000001');

INSERT INTO tenant_memberships(tenant_id,user_id,role,joined_at)
VALUES ('20000000-0000-0000-0000-000000000001',
        '10000000-0000-0000-0000-000000000001',
        'Owner', now());

INSERT INTO subscriptions(id,tenant_id,plan_tier)
VALUES ('30000000-0000-0000-0000-000000000001',
        '20000000-0000-0000-0000-000000000001',
        'tier_1');

INSERT INTO checkout_sessions(
    id,tenant_id,plan_tier,status,provider,provider_checkout_id
) VALUES (
    '40000000-0000-0000-0000-000000000001',
    '20000000-0000-0000-0000-000000000001',
    'tier_1','pending','fixture','worker1-upgrade-checkout'
);

INSERT INTO proxy_check_jobs(
    id,user_id,tenant_id,status,total_count,checked_count
) VALUES (
    '50000000-0000-0000-0000-000000000001',
    '10000000-0000-0000-0000-000000000001',
    '20000000-0000-0000-0000-000000000001',
    'pending', 1, 0
);

INSERT INTO proxy_entries(
    id,job_id,host,port,scheme,encrypted_credentials,result_json
) VALUES (
    '60000000-0000-0000-0000-000000000001',
    '50000000-0000-0000-0000-000000000001',
    'example.com', 8080, 'http', NULL,
    '{"reachable":true,"latency_ms":12,"anonymity":null,"supports_https":false,"country_code":null,"error":null}'::jsonb
);

INSERT INTO billing_audit_log(
    id,provider_event_id,event_type,tenant_id,subscription_id,payload
) VALUES (
    '70000000-0000-0000-0000-000000000001',
    'worker1-upgrade-event',
    'payment_succeeded',
    '20000000-0000-0000-0000-000000000001',
    '30000000-0000-0000-0000-000000000001',
    '{"fixture":true}'::jsonb
);
SQL

sqlx migrate run --source migrations --database-url "$UPGRADE_DATABASE_URL"

psql "$UPGRADE_DATABASE_URL" -v ON_ERROR_STOP=1 <<'SQL'
DO $$
BEGIN
    IF (SELECT plan_tier FROM subscriptions
        WHERE id='30000000-0000-0000-0000-000000000001') <> 'starter'
    THEN RAISE EXCEPTION 'subscription data was not migrated tier_1 -> starter'; END IF;

    IF (SELECT plan_tier FROM checkout_sessions
        WHERE id='40000000-0000-0000-0000-000000000001') <> 'starter'
    THEN RAISE EXCEPTION 'checkout data was not migrated tier_1 -> starter'; END IF;

    IF NOT EXISTS (
        SELECT 1 FROM tenants
        WHERE id='20000000-0000-0000-0000-000000000001'
          AND owner_user_id='10000000-0000-0000-0000-000000000001'
    ) THEN RAISE EXCEPTION 'tenant data was lost'; END IF;

    IF NOT EXISTS (
        SELECT 1 FROM users
        WHERE id='10000000-0000-0000-0000-000000000001'
    ) THEN RAISE EXCEPTION 'user data was lost'; END IF;

    IF NOT EXISTS (
        SELECT 1 FROM proxy_check_jobs
        WHERE id='50000000-0000-0000-0000-000000000001'
          AND tenant_id='20000000-0000-0000-0000-000000000001'
          AND status='pending'
          AND total_count=1
    ) THEN RAISE EXCEPTION 'proxy job data was lost'; END IF;

    IF NOT EXISTS (
        SELECT 1 FROM proxy_entries
        WHERE id='60000000-0000-0000-0000-000000000001'
          AND job_id='50000000-0000-0000-0000-000000000001'
    ) THEN RAISE EXCEPTION 'proxy entry data was lost'; END IF;

    IF NOT EXISTS (
        SELECT 1 FROM billing_audit_log
        WHERE id='70000000-0000-0000-0000-000000000001'
    ) THEN RAISE EXCEPTION 'billing audit record was lost'; END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conrelid='subscriptions'::regclass
          AND conname='chk_subscriptions_plan_tier'
    ) THEN RAISE EXCEPTION 'subscriptions plan constraint missing'; END IF;
END $$;
SQL

echo "Worker 1 database migration tests: PASS"
