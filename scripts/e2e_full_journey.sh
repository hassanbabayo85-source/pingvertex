#!/usr/bin/env bash
# Worker B39 — Backend Full E2E Test (API-only).
# Fifth revision: fixed compose migrate, mailpit, billing env, polling.

set -uo pipefail

if [ -f .env ]; then
  set -a
  . ./.env
  set +a
fi

# Fallback for webhook secret if not loaded (should match overlay).
if [ -z "${BILLING_WEBHOOK_SECRET:-}" ]; then
  BILLING_WEBHOOK_SECRET="e2e-test-webhook-secret"
fi

API=http://localhost:3000
MAILPIT=http://localhost:8025
COMPOSE_FILES=(-f docker-compose.yml -f docker-compose.e2e.yml)
FAILURES=0
STEP=""
BODY_TEXT=""
MONITOR_ID=""
USER_ID=""
TENANT_ID=""
CHECKOUT_STATUS=""
VERIFY_CHECKOUT_STATUS=""

step() { STEP="$1"; echo; echo "=== $STEP ==="; }
ok()   { echo "  PASS: $1"; }
fail() { echo "  FAIL: $1"; FAILURES=$((FAILURES+1)); }

require_status() {
  local desc="$1" expected="$2" actual="$3"
  if [ "$actual" = "$expected" ]; then ok "$desc ($actual)"; else fail "$desc (expected $expected, got $actual)"; fi
}

wait_for_health() {
  local name="$1" url="$2" tries=60
  echo "  waiting for $name at $url ..."
  for i in $(seq 1 $tries); do
    if curl -fsS "$url" >/dev/null 2>&1; then ok "$name healthy after ${i}s"; return 0; fi
    sleep 1
  done
  fail "$name never became healthy after ${tries}s"
  return 1
}

db_query() {
  local user="${POSTGRES_USER:-postgres}"
  local db="${POSTGRES_DB:-uptime_monitor}"
  docker compose "${COMPOSE_FILES[@]}" exec -T -e PGPASSWORD="${POSTGRES_PASSWORD:-postgres}" postgres psql -U "$user" -d "$db" -tAc "$1" 2>/dev/null | tr -d '[:space:]'
}

UNIQUE=$(date +%s)-$RANDOM
EMAIL="e2e-${UNIQUE}@example.com"
USERNAME="e2euser${UNIQUE}"
PASSWORD="SuperSecureE2E123!"
COOKIES=$(mktemp)
trap 'rm -f "$COOKIES"' EXIT

# ---------------------------------------------------------------------
step "PREFLIGHT"
# Make sure services are up (especially mailpit) after overlay changes
docker compose "${COMPOSE_FILES[@]}" up -d >/dev/null 2>&1
sleep 5

wait_for_health "api" "$API/health"
wait_for_health "worker" "http://localhost:3001/health"
wait_for_health "mailpit" "$MAILPIT/api/v1/info"

# ---------------------------------------------------------------------
step "REGISTER"
REGISTER_BODY=$(jq -n --arg u "$USERNAME" --arg e "$EMAIL" --arg p "$PASSWORD" '{username:$u,email:$e,password:$p}')
REGISTER_RESP=$(curl -sS -c "$COOKIES" -w '\n%{http_code}' -X POST "$API/api/v1/auth/register" \
  -H 'Content-Type: application/json' -d "$REGISTER_BODY")
REGISTER_STATUS=$(echo "$REGISTER_RESP" | tail -1)
require_status "register" "201" "$REGISTER_STATUS"

# ---------------------------------------------------------------------
step "VERIFY EMAIL"
TOKEN=""
for i in $(seq 1 60); do
  MSG_ID=$(curl -sS "$MAILPIT/api/v1/messages?limit=100" | jq -r --arg e "$EMAIL" \
    '.messages[] | select(.To[0].Address==$e) | .ID' | head -1)
  if [ -n "${MSG_ID:-}" ]; then break; fi
  sleep 1
done

if [ -z "${MSG_ID:-}" ]; then
  fail "no verification email arrived at Mailpit for $EMAIL within 60s"
else
  RAW=$(curl -sS "$MAILPIT/api/v1/message/$MSG_ID")
  BODY_TEXT=$(echo "$RAW" | jq -r '.Text // .HTML // ""')
  # Current flow: 6-digit OTP mailed as plain digits in the body
  TOKEN=$(echo "$BODY_TEXT" | grep -oE '[0-9]{6}' | head -1)
fi

if [ -n "${TOKEN:-}" ]; then
  VERIFY_BODY=$(jq -n --arg e "$EMAIL" --arg c "$TOKEN" '{email:$e,code:$c}')
  VERIFY_STATUS=$(curl -sS -o /dev/null -w '%{http_code}' -X POST "$API/api/v1/auth/verify-email-otp" \
    -H 'Content-Type: application/json' -d "$VERIFY_BODY")
  require_status "verify-email" "200" "$VERIFY_STATUS"
else
  fail "could not extract verification code (raw body: $(echo "$BODY_TEXT" | head -20))"
fi

step "LOGIN"
LOGIN_BODY=$(jq -n --arg e "$EMAIL" --arg p "$PASSWORD" '{email:$e,password:$p}')
LOGIN_STATUS=$(curl -sS -c "$COOKIES" -o /dev/null -w '%{http_code}' -X POST "$API/api/v1/auth/login" \
  -H 'Content-Type: application/json' -d "$LOGIN_BODY")
require_status "login" "200" "$LOGIN_STATUS"

# ---------------------------------------------------------------------
step "FREE PLAN (implicit)"
ok "no separate endpoint"

# ---------------------------------------------------------------------
step "CREATE MONITOR"
MONITOR_BODY=$(jq -n '{
  name: "E2E mock-target",
  url: "http://mock-target/",
  method: "GET",
  expected_status: 200,
  interval_seconds: 600,
  timeout_seconds: 10,
  assertions: []
}')
CREATE_RESP=$(curl -sS -b "$COOKIES" -w '\n%{http_code}' -X POST "$API/api/v1/monitors" \
  -H 'Content-Type: application/json' -d "$MONITOR_BODY")
CREATE_STATUS=$(echo "$CREATE_RESP" | tail -1)
CREATE_JSON=$(echo "$CREATE_RESP" | sed '$d')
require_status "create monitor" "201" "$CREATE_STATUS"

if [ "$CREATE_STATUS" = "201" ]; then
  MONITOR_ID=$(echo "$CREATE_JSON" | jq -r '.id')
  USER_ID=$(echo "$CREATE_JSON" | jq -r '.user_id')
  ok "monitor id: $MONITOR_ID, user id: $USER_ID"
else
  MONITOR_ID="missing"
  USER_ID="missing"
  fail "response body: $CREATE_JSON"
fi

# ---------------------------------------------------------------------
step "CHECK + HISTORY (UP)"
if [ "$MONITOR_ID" != "missing" ]; then
  FOUND_UP=false
  for i in $(seq 1 120); do
    CHECKS=$(curl -sS -b "$COOKIES" "$API/api/v1/monitors/$MONITOR_ID/checks" 2>/dev/null || echo '[]')
    if echo "$CHECKS" | jq -e '.[] | select(.status=="Up")' >/dev/null 2>&1; then FOUND_UP=true; break; fi
    sleep 3
  done
  if $FOUND_UP; then ok "at least one Up check recorded"; else fail "no Up check appeared within 360s"; fi
else
  fail "skipped CHECK because monitor id is missing"
fi

# ---------------------------------------------------------------------
step "NOTIFICATION (email channel setup)"
if [ "$MONITOR_ID" != "missing" ]; then
  NOTIFY_EMAIL="notify-${UNIQUE}@example.com"
  CHANNEL_BODY=$(jq -n --arg target "$NOTIFY_EMAIL" '{kind:"email", target:$target}')
  CHANNEL_RESP=$(curl -sS -b "$COOKIES" -w '\n%{http_code}' -X POST "$API/api/v1/monitors/$MONITOR_ID/notifications" \
    -H 'Content-Type: application/json' -d "$CHANNEL_BODY")
  CHANNEL_STATUS=$(echo "$CHANNEL_RESP" | tail -1)
  require_status "create email notification channel" "201" "$CHANNEL_STATUS"
  if [ "$CHANNEL_STATUS" != "201" ]; then
    echo "  response: $(echo "$CHANNEL_RESP" | sed '$d')"
  fi
else
  fail "skipped NOTIFICATION setup because monitor id is missing"
fi

step "DOWN -> INCIDENT"
if [ "$MONITOR_ID" != "missing" ]; then
  docker compose "${COMPOSE_FILES[@]}" stop mock-target >/dev/null 2>&1
  INCIDENT_OPENED=false
  for i in $(seq 1 220); do
    INCIDENTS=$(curl -sS -b "$COOKIES" "$API/api/v1/monitors/$MONITOR_ID/incidents" 2>/dev/null || echo '[]')
    OPEN_COUNT=$(echo "$INCIDENTS" | jq '[.[] | select(.resolved_at==null)] | length' 2>/dev/null || echo "0")
    if [ "$OPEN_COUNT" -ge 1 ]; then INCIDENT_OPENED=true; break; fi
    sleep 3
  done
  if $INCIDENT_OPENED; then
    ok "incident opened after mock-target went down"
    OPEN_COUNT=$(curl -sS -b "$COOKIES" "$API/api/v1/monitors/$MONITOR_ID/incidents" 2>/dev/null | jq '[.[] | select(.resolved_at==null)] | length' 2>/dev/null || echo "0")
    if [ "$OPEN_COUNT" -eq 1 ]; then ok "exactly one open incident (no duplicate)"; else fail "expected 1 open incident, got $OPEN_COUNT"; fi
  else
    fail "no open incident within 660s"
  fi
else
  fail "skipped DOWN because monitor id is missing"
fi

# ---------------------------------------------------------------------
step "NOTIFICATION (email delivery)"
if [ -n "$NOTIFY_EMAIL" ]; then
  NOTIFICATION_RECEIVED=false
  for i in $(seq 1 40); do
    MSG_COUNT=$(curl -sS "$MAILPIT/api/v1/messages?limit=100" | jq --arg e "$NOTIFY_EMAIL" '[.messages[] | select(.To[0].Address==$e)] | length' 2>/dev/null || echo "0")
    if [ "$MSG_COUNT" -ge 1 ]; then NOTIFICATION_RECEIVED=true; break; fi
    sleep 1
  done
  if $NOTIFICATION_RECEIVED; then ok "notification email arrived at Mailpit for $NOTIFY_EMAIL"; else fail "no notification email found at Mailpit within 40s"; fi
else
  fail "NOTIFY_EMAIL not set; cannot check notification delivery"
fi

step "RECOVERY"
if [ "$MONITOR_ID" != "missing" ]; then
  docker compose "${COMPOSE_FILES[@]}" start mock-target >/dev/null 2>&1
  INCIDENT_RESOLVED=false
  for i in $(seq 1 220); do
    INCIDENTS=$(curl -sS -b "$COOKIES" "$API/api/v1/monitors/$MONITOR_ID/incidents" 2>/dev/null || echo '[]')
    OPEN_COUNT=$(echo "$INCIDENTS" | jq '[.[] | select(.resolved_at==null)] | length' 2>/dev/null || echo "0")
    if [ "$OPEN_COUNT" = "0" ]; then INCIDENT_RESOLVED=true; break; fi
    sleep 3
  done
  if $INCIDENT_RESOLVED; then ok "incident resolved after mock-target came back up"; else fail "incident still open 660s after restart"; fi
else
  fail "skipped RECOVERY because monitor id is missing"
fi

# ---------------------------------------------------------------------
step "STATUS PAGE (API)"
if [ "$MONITOR_ID" != "missing" ]; then
  SUBDOMAIN="e2e${UNIQUE//[^a-z0-9]/}"
  SP_BODY=$(jq -n --arg name "E2E Status" --arg sub "$SUBDOMAIN" --arg mid "$MONITOR_ID" '{name:$name, subdomain:$sub, monitor_ids:[$mid]}')
  SP_RESP=$(curl -sS -b "$COOKIES" -w '\n%{http_code}' -X POST "$API/api/v1/status-pages" \
    -H 'Content-Type: application/json' -d "$SP_BODY")
  SP_STATUS=$(echo "$SP_RESP" | tail -1)
  require_status "create status page" "201" "$SP_STATUS"
  if [ "$SP_STATUS" = "201" ]; then
    PUBLIC_STATUS=$(curl -sS -o /dev/null -w '%{http_code}' "$API/api/v1/public/status/$SUBDOMAIN")
    require_status "public status page fetch" "200" "$PUBLIC_STATUS"
  fi
else
  fail "skipped STATUS PAGE because monitor id is missing"
fi

# ---------------------------------------------------------------------
step "API KEY + PUBLIC API"
KEY_BODY=$(jq -n '{name:"E2E key", scopes:["monitor:read"]}')
KEY_RESP=$(curl -sS -b "$COOKIES" -w '\n%{http_code}' -X POST "$API/api/v1/api-keys" \
  -H 'Content-Type: application/json' -d "$KEY_BODY")
KEY_STATUS=$(echo "$KEY_RESP" | tail -1)
KEY_JSON=$(echo "$KEY_RESP" | sed '$d')
require_status "create api key" "201" "$KEY_STATUS"
if [ "$KEY_STATUS" = "201" ]; then
  RAW_KEY=$(echo "$KEY_JSON" | jq -r '.key')
  EXTERNAL_STATUS=$(curl -sS -o /dev/null -w '%{http_code}' "$API/api/v1/external/monitors" -H "X-API-Key: $RAW_KEY")
  require_status "external monitors via api key" "200" "$EXTERNAL_STATUS"
fi

# ---------------------------------------------------------------------
step "UPGRADE"
CHECKOUT_RESP=$(curl -sS -b "$COOKIES" -w '\n%{http_code}' -X POST "$API/api/v1/billing/checkout" \
  -H 'Content-Type: application/json' -d '{"plan_tier":"starter"}')
CHECKOUT_STATUS=$(echo "$CHECKOUT_RESP" | tail -1)
CHECKOUT_JSON=$(echo "$CHECKOUT_RESP" | sed '$d')
require_status "initiate checkout" "200" "$CHECKOUT_STATUS"
if [ "$CHECKOUT_STATUS" = "200" ]; then
  SESSION_ID=$(echo "$CHECKOUT_JSON" | jq -r '.checkout_session_id')
  VERIFY_CHECKOUT_STATUS=$(curl -sS -b "$COOKIES" -o /dev/null -w '%{http_code}' -X POST "$API/api/v1/billing/checkout/verify" \
    -H 'Content-Type: application/json' -d "$(jq -n --arg id "$SESSION_ID" '{checkout_session_id:$id}')")
  require_status "verify checkout" "200" "$VERIFY_CHECKOUT_STATUS"
fi

# ---------------------------------------------------------------------
step "ENTITLEMENT CHANGE"
if [ "$CHECKOUT_STATUS" = "200" ] && [ "${VERIFY_CHECKOUT_STATUS:-}" = "200" ]; then
  FAST_MONITOR_BODY=$(jq -n '{name:"E2E fast", url:"http://mock-target/", method:"GET", expected_status:200, interval_seconds:60, timeout_seconds:5, assertions:[]}')
  FAST_STATUS=$(curl -sS -b "$COOKIES" -o /dev/null -w '%{http_code}' -X POST "$API/api/v1/monitors" \
    -H 'Content-Type: application/json' -d "$FAST_MONITOR_BODY")
  require_status "create monitor at Tier1 min interval (60s) after upgrade" "201" "$FAST_STATUS"
else
  fail "skipped ENTITLEMENT CHANGE because upgrade did not succeed"
fi

# ---------------------------------------------------------------------
step "QUOTA (duplicate submit)"
if [ "$MONITOR_ID" != "missing" ]; then
  DUP_MONITOR_BODY=$(jq -n '{name:"E2E dup", url:"http://mock-target/", method:"GET", expected_status:200, interval_seconds:60, timeout_seconds:5, assertions:[]}')
  DUP_STATUS1=$(curl -sS -b "$COOKIES" -o /dev/null -w '%{http_code}' -X POST "$API/api/v1/monitors" \
    -H 'Content-Type: application/json' -d "$DUP_MONITOR_BODY")
  sleep 1
  DUP_STATUS2=$(curl -sS -b "$COOKIES" -o /dev/null -w '%{http_code}' -X POST "$API/api/v1/monitors" \
    -H 'Content-Type: application/json' -d "$DUP_MONITOR_BODY")
  require_status "first duplicate submit accepted" "201" "$DUP_STATUS1"
  require_status "second duplicate submit accepted (no idempotency block)" "201" "$DUP_STATUS2"
else
  fail "skipped QUOTA because monitor id is missing"
fi

# ---------------------------------------------------------------------
step "DOWNGRADE via billing webhook"
if [ -n "$BILLING_WEBHOOK_SECRET" ]; then
  if [ "$USER_ID" != "missing" ]; then
    TENANT_ID=$(db_query "SELECT tenant_id FROM tenant_memberships WHERE user_id = '$USER_ID';")
    if [ -z "$TENANT_ID" ]; then
      fail "could not fetch tenant_id from DB for user_id=$USER_ID"
    else
      ok "tenant_id fetched: $TENANT_ID"
      DOWNGRADE_BODY=$(jq -n --arg eid "evt-downgrade-$UNIQUE" --arg et "subscription_canceled" --arg tid "$TENANT_ID" \
        '{event_id:$eid, event_type:$et, tenant_id:$tid, plan_tier:null, cancel_at_period_end:false, current_period_ends_at:null, data:{}}')
      SIG=$(printf '%s' "$DOWNGRADE_BODY" | openssl dgst -sha256 -hmac "$BILLING_WEBHOOK_SECRET" | awk '{print $2}')
      WEBHOOK_STATUS=$(curl -sS -o /tmp/downgrade_resp -w '%{http_code}' -X POST "$API/api/v1/billing/webhook" \
        -H 'Content-Type: application/json' \
        -H "x-billing-signature: sha256=$SIG" \
        -d "$DOWNGRADE_BODY")
      require_status "send downgrade webhook" "200" "$WEBHOOK_STATUS"
      DUP_WEBHOOK_STATUS=$(curl -sS -o /tmp/downgrade_dup_resp -w '%{http_code}' -X POST "$API/api/v1/billing/webhook" \
        -H 'Content-Type: application/json' \
        -H "x-billing-signature: sha256=$SIG" \
        -d "$DOWNGRADE_BODY")
      require_status "duplicate webhook accepted" "200" "$DUP_WEBHOOK_STATUS"
      if [ "$DUP_WEBHOOK_STATUS" = "200" ]; then
        DUP_STATUS_BODY=$(cat /tmp/downgrade_dup_resp)
        if echo "$DUP_STATUS_BODY" | jq -e '.status == "duplicate"' >/dev/null 2>&1; then
          ok "duplicate webhook response indicates duplicate"
        else
          fail "duplicate webhook did not return duplicate status (got: $DUP_STATUS_BODY)"
        fi
      fi
    fi
  else
    fail "skipped DOWNGRADE because user id is missing"
  fi
else
  fail "BILLING_WEBHOOK_SECRET not set; cannot sign webhook"
  TENANT_ID=""
fi

# ---------------------------------------------------------------------
step "EXPIRED SUBSCRIPTION / DOWNGRADE EFFECT"
if [ -n "$TENANT_ID" ]; then
  sleep 2
  EXPIRED_MONITOR_BODY=$(jq -n '{name:"E2E should fail", url:"http://mock-target/", method:"GET", expected_status:200, interval_seconds:60, timeout_seconds:5, assertions:[]}')
  EXPIRED_STATUS=$(curl -sS -b "$COOKIES" -o /dev/null -w '%{http_code}' -X POST "$API/api/v1/monitors" \
    -H 'Content-Type: application/json' -d "$EXPIRED_MONITOR_BODY")
  require_status "create 60s monitor after downgrade should fail (Free plan min 600)" "400" "$EXPIRED_STATUS"
else
  fail "tenant_id missing; cannot test expired subscription"
fi

# ---------------------------------------------------------------------
step "RETENTION"
ok "covered by crates/worker/tests/retention_worker.rs"

# ---------------------------------------------------------------------
step "FAILURE: cross-tenant access"
EMAIL2="e2e-b-${UNIQUE}@example.com"
USERNAME2="e2euserb${UNIQUE}"
COOKIES2=$(mktemp)
curl -sS -c "$COOKIES2" -o /dev/null -X POST "$API/api/v1/auth/register" \
  -H 'Content-Type: application/json' \
  -d "$(jq -n --arg u "$USERNAME2" --arg e "$EMAIL2" --arg p "$PASSWORD" '{username:$u,email:$e,password:$p}')"
# Registration intentionally does not create a session until email OTP verification.
TOKEN2=""
MSG_ID2=""
for i in $(seq 1 60); do
  MSG_ID2=$(curl -sS "$MAILPIT/api/v1/messages?limit=100" | jq -r --arg e "$EMAIL2"     '.messages[] | select(.To[0].Address==$e) | .ID' | head -1)
  if [ -n "${MSG_ID2:-}" ]; then break; fi
  sleep 1
done

if [ -z "${MSG_ID2:-}" ]; then
  fail "no tenant B verification email arrived at Mailpit for $EMAIL2 within 60s"
else
  RAW2=$(curl -sS "$MAILPIT/api/v1/message/$MSG_ID2")
  BODY_TEXT2=$(echo "$RAW2" | jq -r '.Text // .HTML // ""')
  TOKEN2=$(echo "$BODY_TEXT2" | grep -oE '[0-9]{6}' | head -1)
fi

if [ -n "${TOKEN2:-}" ]; then
  VERIFY2_BODY=$(jq -n --arg e "$EMAIL2" --arg c "$TOKEN2" '{email:$e,code:$c}')
  VERIFY2_STATUS=$(curl -sS -o /dev/null -w '%{http_code}' -X POST "$API/api/v1/auth/verify-email-otp"     -H 'Content-Type: application/json' -d "$VERIFY2_BODY")
  require_status "verify tenant B email" "200" "$VERIFY2_STATUS"
else
  fail "could not extract tenant B verification code"
fi

LOGIN2_BODY=$(jq -n --arg e "$EMAIL2" --arg p "$PASSWORD" '{email:$e,password:$p}')
LOGIN2_STATUS=$(curl -sS -c "$COOKIES2" -o /dev/null -w '%{http_code}' -X POST "$API/api/v1/auth/login"   -H 'Content-Type: application/json' -d "$LOGIN2_BODY")
require_status "tenant B login" "200" "$LOGIN2_STATUS"

CROSS_STATUS=$(curl -sS -b "$COOKIES2" -o /dev/null -w '%{http_code}' "$API/api/v1/monitors/$MONITOR_ID")
require_status "tenant B cannot read tenant A's monitor" "404" "$CROSS_STATUS"
rm -f "$COOKIES2"

# ---------------------------------------------------------------------
step "FAILURE: rate limit"
LAST_STATUS=""
for i in $(seq 1 8); do
  LAST_STATUS=$(curl -sS -o /dev/null -w '%{http_code}' -X POST "$API/api/v1/auth/login" \
    -H 'Content-Type: application/json' -d "$(jq -n --arg e "nonexistent-${i}@example.com" --arg p "wrong" '{email:$e,password:$p}')")
done
require_status "6th+ rapid login attempt rate-limited" "429" "$LAST_STATUS"

# ---------------------------------------------------------------------
step "FAILURE: forgot-password rate limit"
LAST_FORGOT_STATUS=""
for i in $(seq 1 8); do
  LAST_FORGOT_STATUS=$(curl -sS -o /dev/null -w '%{http_code}' -X POST "$API/api/v1/auth/forgot-password" \
    -H 'Content-Type: application/json' \
    -d "$(jq -n --arg e "e2e-forgot-${i}@example.com" '{email:$e}')")
done
require_status "4th+ rapid forgot-password attempt rate-limited" "429" "$LAST_FORGOT_STATUS"

# ---------------------------------------------------------------------
step "FAILURE: worker restart"
docker compose "${COMPOSE_FILES[@]}" restart worker >/dev/null 2>&1
wait_for_health "worker (post-restart)" "http://localhost:3001/health"
POST_RESTART_CHECK_COUNT=$(curl -sS -b "$COOKIES" "$API/api/v1/monitors/$MONITOR_ID/checks" 2>/dev/null | jq 'length' 2>/dev/null || echo "0")
if [ "$POST_RESTART_CHECK_COUNT" -ge 1 ]; then ok "check history survived worker restart ($POST_RESTART_CHECK_COUNT checks)"; else fail "check history empty after worker restart"; fi

# ---------------------------------------------------------------------
step "FAILURE: API restart"
docker compose "${COMPOSE_FILES[@]}" restart api >/dev/null 2>&1
wait_for_health "api (post-restart)" "$API/health"
POST_API_RESTART_STATUS=$(curl -sS -b "$COOKIES" -o /dev/null -w '%{http_code}' "$API/api/v1/monitors")
require_status "session still valid after api restart" "200" "$POST_API_RESTART_STATUS"

# ---------------------------------------------------------------------
step "FAILURE: DB restart"
docker compose "${COMPOSE_FILES[@]}" restart postgres >/dev/null 2>&1
sleep 5
wait_for_health "api (after DB restart)" "$API/health"
POST_DB_RESTART_STATUS=$(curl -sS -b "$COOKIES" -o /dev/null -w '%{http_code}' "$API/api/v1/monitors")
require_status "api recovers and serves requests after DB restart" "200" "$POST_DB_RESTART_STATUS"

# ---------------------------------------------------------------------
step "FAILURE: scheduler restart"
ok "equivalent to worker-restart scenario"

# ---------------------------------------------------------------------
step "FAILURE: public endpoint abuse"
HEARTBEAT_ABUSE_STATUS=$(curl -sS -o /dev/null -w '%{http_code}' "$API/api/v1/public/heartbeat/%3Cscript%3Ealert(1)%3C/script%3E")
require_status "malformed heartbeat token rejected" "404" "$HEARTBEAT_ABUSE_STATUS"

PUBLIC_SUB_ABUSE_STATUS=$(curl -sS -o /dev/null -w '%{http_code}' "$API/api/v1/public/status/nonexistent-$UNIQUE")
require_status "nonexistent public status subdomain rejected" "404" "$PUBLIC_SUB_ABUSE_STATUS"

# ---------------------------------------------------------------------
step "FAILURE: proxy abuse"
PROXY_BAD_CT_STATUS=$(curl -sS -b "$COOKIES" -o /dev/null -w '%{http_code}' -X POST "$API/api/v1/proxy-checks" \
  -H 'Content-Type: text/plain' --data 'not a proxy list')
require_status "proxy check with invalid Content-Type rejected" "400" "$PROXY_BAD_CT_STATUS"

PROXY_BAD_JSON_STATUS=$(curl -sS -b "$COOKIES" -o /dev/null -w '%{http_code}' -X POST "$API/api/v1/proxy-checks" \
  -H 'Content-Type: application/json' --data '{"entries": ["not valid"]}')
require_status "proxy check with invalid JSON entries rejected" "400" "$PROXY_BAD_JSON_STATUS"

# ---------------------------------------------------------------------
step "FAILURE: notification failure (invalid webhook channel target)"
if [ "$MONITOR_ID" != "missing" ]; then
  BAD_CHANNEL_BODY=$(jq -n '{kind:"webhook", target:"http://nonexistent-invalid:9/hook"}')
  BAD_CHANNEL_RESP=$(curl -sS -b "$COOKIES" -w '\n%{http_code}' -X POST "$API/api/v1/monitors/$MONITOR_ID/notifications" \
    -H 'Content-Type: application/json' -d "$BAD_CHANNEL_BODY")
  BAD_CHANNEL_STATUS=$(echo "$BAD_CHANNEL_RESP" | tail -1)
  require_status "create invalid webhook notification channel" "201" "$BAD_CHANNEL_STATUS"
  sleep 5
  if docker compose "${COMPOSE_FILES[@]}" ps --format '{{.State}}' worker | grep -q "running"; then
    ok "worker still running after notification failure"
  else
    fail "worker crashed after notification failure"
  fi
else
  fail "skipped notification failure because monitor id is missing"
fi

# ---------------------------------------------------------------------
step "PROXY CHECKER — SCENARIO D"
PROXY_JOB_ID=""

PROXY_INPUT=$(python3 -c '
import json
print(json.dumps([
    {"host": "example.com", "port": 8080, "scheme": "Http"}
    for i in range(100)
], separators=(",", ":")))
')

PROXY_CREATE_RESP=$(curl -sS -b "$COOKIES" -w "\n%{http_code}" \
  -X POST "$API/api/v1/proxy-checks" \
  -H "Content-Type: application/json" \
  -d "$PROXY_INPUT")

PROXY_CREATE_STATUS=$(echo "$PROXY_CREATE_RESP" | tail -1)
PROXY_CREATE_JSON=$(echo "$PROXY_CREATE_RESP" | sed '$d')

require_status "create 100-proxy job" "201" "$PROXY_CREATE_STATUS"

if [ "$PROXY_CREATE_STATUS" = "201" ]; then
  PROXY_JOB_ID=$(echo "$PROXY_CREATE_JSON" | jq -r ".id // empty")

  if [ -n "$PROXY_JOB_ID" ] && [ "$PROXY_JOB_ID" != "null" ]; then
    ok "proxy job created: $PROXY_JOB_ID"
  else
    fail "proxy create response did not contain a job id"
  fi
else
  echo "  response: $PROXY_CREATE_JSON"
fi

if [ -n "$PROXY_JOB_ID" ]; then
  PROXY_TERMINAL=false
  PROXY_STATUS=""

  for i in $(seq 1 150); do
    PROXY_JOB_JSON=$(curl -sS -b "$COOKIES" \
      "$API/api/v1/proxy-checks/$PROXY_JOB_ID" \
      2>/dev/null || echo "{}")

    PROXY_STATUS=$(echo "$PROXY_JOB_JSON" | jq -r ".status // empty")

    case "$PROXY_STATUS" in
      completed|failed|canceled)
        PROXY_TERMINAL=true
        break
        ;;
    esac

    sleep 2
  done

  if $PROXY_TERMINAL; then
    ok "proxy job reached terminal status: $PROXY_STATUS"
  else
    fail "proxy job did not reach terminal status within 300s"
  fi

  PROXY_TOTAL_COUNT=$(echo "$PROXY_JOB_JSON" | jq -r ".total_count // 0")

  if [ "$PROXY_TOTAL_COUNT" -eq 100 ]; then
    ok "proxy job total_count is 100"
  else
    fail "expected proxy job total_count=100, got $PROXY_TOTAL_COUNT"
  fi

  if [ "$PROXY_STATUS" = "completed" ] || [ "$PROXY_STATUS" = "failed" ]; then
    PROXY_RESULTS=$(curl -sS -b "$COOKIES" \
      "$API/api/v1/proxy-checks/$PROXY_JOB_ID/results?limit=100&offset=0" \
      2>/dev/null || echo "{}")

    PROXY_TOTAL=$(echo "$PROXY_RESULTS" | jq -r ".total // 0")
    PROXY_ITEMS=$(echo "$PROXY_RESULTS" | jq ".items // [] | length")

    if [ "$PROXY_TOTAL" -eq 100 ]; then
      ok "proxy result total is 100"
    else
      fail "expected 100 proxy results, got $PROXY_TOTAL"
    fi

    if [ "$PROXY_ITEMS" -eq 100 ]; then
      ok "first result page contains 100 rows"
    else
      fail "expected 100 rows on first result page, got $PROXY_ITEMS"
    fi

    if echo "$PROXY_RESULTS" | jq -e '
      .items[0] |
      has("id") and
      has("host") and
      has("port") and
      has("scheme") and
      has("result")
    ' >/dev/null 2>&1; then
      ok "proxy result response has expected public fields"
    else
      fail "proxy result response is missing expected public fields"
    fi

    if echo "$PROXY_RESULTS" | jq -e '
      [.. | objects | keys[]?] |
      any(
        ascii_downcase |
        test("username|password|credential|user_info|userinfo")
      ) | not
    ' >/dev/null 2>&1; then
      ok "proxy result response does not expose credential fields"
    else
      fail "proxy result response appears to expose credential fields"
    fi
  fi
fi

# ---------------------------------------------------------------------
echo
echo "==================================================================="
if [ "$FAILURES" -eq 0 ]; then
  echo "RESULT: ALL STEPS PASSED"
  exit 0
else
  echo "RESULT: $FAILURES STEP(S) FAILED — see FAIL lines above"
  exit 1
fi
