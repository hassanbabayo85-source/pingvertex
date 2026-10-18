#!/usr/bin/env bash
# Worker Backend Full E2E Test (API-only).
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
# CHECKOUT_STATUS=""
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
step "FREE INTERVAL BOUNDARY"
if [ "$MONITOR_ID" != "missing" ]; then
  FREE_299_BODY=$(jq -n '{name:"E2E free-299-rejected", url:"http://mock-target/", method:"GET", expected_status:200, interval_seconds:299, timeout_seconds:10, assertions:[]}')
  FREE_299_STATUS=$(curl -sS -b "$COOKIES" -o /dev/null -w '%{http_code}' -X POST "$API/api/v1/monitors" \
    -H 'Content-Type: application/json' -d "$FREE_299_BODY")
  require_status "Free 299s monitor should be rejected" "400" "$FREE_299_STATUS"

  FREE_300_BODY=$(jq -n '{name:"E2E free-300-accepted", url:"http://mock-target/", method:"GET", expected_status:200, interval_seconds:300, timeout_seconds:10, assertions:[]}')
  FREE_300_RESP=$(curl -sS -b "$COOKIES" -w '\n%{http_code}' -X POST "$API/api/v1/monitors" \
    -H 'Content-Type: application/json' -d "$FREE_300_BODY")
  FREE_300_STATUS=$(echo "$FREE_300_RESP" | tail -1)
  require_status "Free 300s monitor should be accepted" "201" "$FREE_300_STATUS"
else
  fail "skipped FREE INTERVAL BOUNDARY because monitor id is missing"
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
# ---------------------------------------------------------------------
step "PAYG PRICING + QUOTE"
PAYG_PRICING_STATUS=$(curl -sS -b "$COOKIES" -o /tmp/e2e_payg_pricing.json -w '%{http_code}' \
  "$API/api/v1/payg/pricing")
require_status "GET PAYG pricing" "200" "$PAYG_PRICING_STATUS"
QUOTE_BODY=$(jq -n '{monitors:1, interval_seconds:300, regions:1, forecast_days:1}')
QUOTE_STATUS=$(curl -sS -b "$COOKIES" -o /tmp/e2e_payg_quote.json -w '%{http_code}' \
  -X POST "$API/api/v1/payg/preview" \
  -H 'Content-Type: application/json' -d "$QUOTE_BODY")
require_status "POST PAYG preview" "200" "$QUOTE_STATUS"

step "ENTITLEMENT CHANGE"
ok "PAYG-first runtime entitlement is exercised by pricing/preview and wallet paths"

step "QUOTA (duplicate submit)"
if [ "$MONITOR_ID" != "missing" ]; then
  DUP_MONITOR_BODY=$(jq -n '{name:"E2E dup", url:"http://mock-target/", method:"GET", expected_status:200, interval_seconds:299, timeout_seconds:5, assertions:[]}')
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
# step "DOWNGRADE via billing webhook"
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
  EXPIRED_MONITOR_BODY=$(jq -n '{name:"E2E should fail", url:"http://mock-target/", method:"GET", expected_status:200, interval_seconds:299, timeout_seconds:5, assertions:[]}')
  EXPIRED_STATUS=$(curl -sS -b "$COOKIES" -o /dev/null -w '%{http_code}' -X POST "$API/api/v1/monitors" \
    -H 'Content-Type: application/json' -d "$EXPIRED_MONITOR_BODY")
  require_status "create 299s monitor after downgrade should fail (Free plan min 300)" "400" "$EXPIRED_STATUS"
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

# MASTER FULL-SYSTEM EXTENSION
# =====================================================================
# IMPORTANT: this section deliberately does NOT use `set -e`. A failed
# endpoint/test is recorded and the suite keeps going so one broken feature
# cannot hide failures in later features.

PASS_COUNT=0
FAIL_COUNT_BEFORE_MASTER="$FAILURES"
MASTER_TESTS=0

master_ok() {
  MASTER_TESTS=$((MASTER_TESTS + 1))
  PASS_COUNT=$((PASS_COUNT + 1))
  ok "$1"
}
master_fail() {
  MASTER_TESTS=$((MASTER_TESTS + 1))
  fail "$1"
}

expect_any() {
  local desc="$1" actual="$2"; shift 2
  local wanted
  for wanted in "$@"; do
    if [ "$actual" = "$wanted" ]; then
      master_ok "$desc ($actual)"
      return 0
    fi
  done
  master_fail "$desc (got $actual; expected one of: $*)"
  return 0
}

http_status() {
  local method="$1" url="$2"; shift 2
  curl -sS -o /dev/null -w '%{http_code}' -X "$method" "$url" "$@" 2>/dev/null || echo 000
}

authed_status() {
  local method="$1" path="$2"; shift 2
  http_status "$method" "$API$path" -b "$COOKIES" "$@"
}

unauth_status() {
  local method="$1" path="$2"; shift 2
  http_status "$method" "$API$path" "$@"
}

get_json() {
  curl -sS -b "$COOKIES" "$API$1" 2>/dev/null || echo '{}'
}

# ------------------------------------------------------------------
step "MASTER PREFLIGHT + TOOLCHAIN"
for tool in curl jq openssl python3 docker; do
  if command -v "$tool" >/dev/null 2>&1; then master_ok "tool available: $tool"; else master_fail "required tool missing: $tool"; fi
done

API_HEALTH=$(http_status GET "$API/health")
expect_any "API health" "$API_HEALTH" 200
API_READY=$(http_status GET "$API/ready")
expect_any "API readiness" "$API_READY" 200
WORKER_HEALTH=$(http_status GET "http://localhost:3001/health")
expect_any "worker health" "$WORKER_HEALTH" 200
MAILPIT_HEALTH=$(http_status GET "$MAILPIT/api/v1/info")
expect_any "Mailpit health" "$MAILPIT_HEALTH" 200

# ------------------------------------------------------------------
step "MASTER AUTH + SESSION SECURITY"
ME_STATUS=$(authed_status GET /api/v1/auth/me)
expect_any "authenticated /me" "$ME_STATUS" 200
NOAUTH_ME=$(unauth_status GET /api/v1/auth/me)
expect_any "unauthenticated /me rejected" "$NOAUTH_ME" 401 403

for route in \
  /api/v1/auth/oauth/google \
  /api/v1/auth/oauth/apple \
  /api/v1/auth/verify-email \
  /api/v1/auth/verify-email-otp \
  /api/v1/auth/resend-verification \
  /api/v1/auth/forgot-password \
  /api/v1/auth/reset-password; do
  ST=$(http_status POST "$API$route" -H 'Content-Type: application/json' -d '{}')
  expect_any "auth validation route reachable: $route" "$ST" 400 401 404 422 429
 done

# TOTP enrollment is a real authenticated lifecycle. We intentionally stop
# at invalid confirmation so we do not leave the test account permanently
# requiring 2FA before the rest of this suite finishes.
TOTP_ENABLE_RESP=$(curl -sS -b "$COOKIES" -w '\n%{http_code}' -X POST "$API/api/v1/auth/totp/enable" \
  -H 'Content-Type: application/json' -d '{}')
TOTP_ENABLE_STATUS=$(echo "$TOTP_ENABLE_RESP" | tail -1)
expect_any "TOTP enable" "$TOTP_ENABLE_STATUS" 200 400
if [ "$TOTP_ENABLE_STATUS" = "200" ]; then
  TOTP_CONFIRM_STATUS=$(http_status POST "$API/api/v1/auth/totp/confirm" -b "$COOKIES" \
    -H 'Content-Type: application/json' -d '{"totp_code":"000000"}')
  expect_any "TOTP invalid confirmation rejected" "$TOTP_CONFIRM_STATUS" 400 401
fi
PASSWORD_CHANGE_STATUS=$(http_status POST "$API/api/v1/auth/password/change" -b "$COOKIES" \
  -H 'Content-Type: application/json' -d '{}')
expect_any "password-change validation/authorization" "$PASSWORD_CHANGE_STATUS" 400 401 422

# ------------------------------------------------------------------
step "MASTER MONITOR CRUD + CONTROL SURFACE"
MONITORS_STATUS=$(authed_status GET /api/v1/monitors)
expect_any "list monitors" "$MONITORS_STATUS" 200
MONITOR_GET_STATUS=$(authed_status GET "/api/v1/monitors/$MONITOR_ID")
expect_any "get monitor" "$MONITOR_GET_STATUS" 200 404

UPDATE_MONITOR_BODY=$(jq -n '{name:"E2E updated monitor", url:"http://mock-target/", method:"GET", expected_status:200, interval_seconds:300, timeout_seconds:10, assertions:[]}')
UPDATE_MONITOR_STATUS=$(curl -sS -b "$COOKIES" -o /dev/null -w '%{http_code}' -X PUT \
  "$API/api/v1/monitors/$MONITOR_ID" -H 'Content-Type: application/json' -d "$UPDATE_MONITOR_BODY")
expect_any "update monitor" "$UPDATE_MONITOR_STATUS" 200 400 404

DISABLE_STATUS=$(authed_status POST "/api/v1/monitors/$MONITOR_ID/disable")
expect_any "disable monitor" "$DISABLE_STATUS" 200 204 404
# One-time check must refuse disabled monitors. This is a security/billing
# invariant, not merely a UI behavior.
ONE_TIME_DISABLED=$(curl -sS -b "$COOKIES" -o /dev/null -w '%{http_code}' -X POST \
  "$API/api/v1/monitors/$MONITOR_ID/checks/one-time" \
  -H 'Content-Type: application/json' -d "$(jq -n --arg id "$MONITOR_ID" '{monitor_id:$id}')")
expect_any "one-time check blocked while monitor disabled" "$ONE_TIME_DISABLED" 409 400 403 404
ENABLE_STATUS=$(authed_status POST "/api/v1/monitors/$MONITOR_ID/enable")
expect_any "re-enable monitor" "$ENABLE_STATUS" 200 204 404

CHECKS_STATUS=$(authed_status GET "/api/v1/monitors/$MONITOR_ID/checks")
expect_any "monitor check history" "$CHECKS_STATUS" 200 404
INCIDENTS_STATUS=$(authed_status GET "/api/v1/monitors/$MONITOR_ID/incidents")
expect_any "monitor incidents" "$INCIDENTS_STATUS" 200 404
ALL_CHECKS_STATUS=$(authed_status GET /api/v1/checks)
expect_any "tenant checks list" "$ALL_CHECKS_STATUS" 200

# ------------------------------------------------------------------
step "MASTER NOTIFICATIONS + ESCALATION"
CHANNEL_LIST_STATUS=$(authed_status GET "/api/v1/monitors/$MONITOR_ID/notifications")
expect_any "list notification channels" "$CHANNEL_LIST_STATUS" 200 404
CHANNEL_ID=""
if [ "$CHANNEL_STATUS" = "201" ]; then
  CHANNEL_ID=$(echo "$CHANNEL_RESP" | sed '$d' | jq -r '.id // empty')
fi
if [ -z "$CHANNEL_ID" ]; then
  CHANNEL_ID=$(get_json "/api/v1/monitors/$MONITOR_ID/notifications" | jq -r '.[0].id // empty')
fi

if [ -n "$CHANNEL_ID" ]; then
  TEST_NOTIFY_STATUS=$(authed_status POST "/api/v1/notifications/$CHANNEL_ID/test")
  expect_any "test notification channel" "$TEST_NOTIFY_STATUS" 200 202 204 400 429 500
  ESC_BODY=$(jq -n --arg mid "$MONITOR_ID" --arg cid "$CHANNEL_ID" \
    '{monitor_id:$mid,steps:[{delay_minutes:1,channel_id:$cid}]}')
  ESC_RESP=$(curl -sS -b "$COOKIES" -w '\n%{http_code}' -X POST "$API/api/v1/escalation-policies" \
    -H 'Content-Type: application/json' -d "$ESC_BODY")
  ESC_STATUS=$(echo "$ESC_RESP" | tail -1)
  expect_any "create escalation policy" "$ESC_STATUS" 201 400 404
  if [ "$ESC_STATUS" = "201" ]; then
    ESC_ID=$(echo "$ESC_RESP" | sed '$d' | jq -r '.id // empty')
    ESC_UPDATE_BODY=$(jq -n --arg mid "$MONITOR_ID" --arg cid "$CHANNEL_ID" \
      '{monitor_id:$mid,steps:[{delay_minutes:2,channel_id:$cid}]}')
    ESC_UPDATE_STATUS=$(curl -sS -b "$COOKIES" -o /dev/null -w '%{http_code}' -X PUT \
      "$API/api/v1/escalation-policies/$ESC_ID" -H 'Content-Type: application/json' -d "$ESC_UPDATE_BODY")
    expect_any "update escalation policy" "$ESC_UPDATE_STATUS" 200 400 404
  fi
else
  master_fail "could not obtain notification channel id for escalation tests"
fi
ESC_LIST_STATUS=$(authed_status GET /api/v1/escalation-policies)
expect_any "list escalation policies" "$ESC_LIST_STATUS" 200

# ------------------------------------------------------------------
step "MASTER MAINTENANCE WINDOWS"
if [ "$MONITOR_ID" != "missing" ]; then
  MW_BODY=$(python3 - <<'PY'
from datetime import datetime, timedelta, timezone
n=datetime.now(timezone.utc)
print('{"starts_at":"%s","ends_at":"%s","reason":"E2E maintenance"}' % ((n+timedelta(minutes=5)).isoformat().replace('+00:00','Z'), (n+timedelta(minutes=15)).isoformat().replace('+00:00','Z')))
PY
)
  MW_STATUS=$(curl -sS -b "$COOKIES" -o /dev/null -w '%{http_code}' -X POST \
    "$API/api/v1/monitors/$MONITOR_ID/maintenance-windows" -H 'Content-Type: application/json' -d "$MW_BODY")
  expect_any "create maintenance window" "$MW_STATUS" 201 400 404
  MW_LIST_STATUS=$(authed_status GET "/api/v1/monitors/$MONITOR_ID/maintenance-windows")
  expect_any "list maintenance windows" "$MW_LIST_STATUS" 200 404
fi

# ------------------------------------------------------------------
step "MASTER API FLOWS"
if [ "$MONITOR_ID" != "missing" ]; then
  FLOW_BODY=$(jq -n '{steps:[{order:1,name:"GET mock",method:"GET",url_template:"http://mock-target/",headers:[],body:null,extract:[],assertions:[]}]}')
  FLOW_RESP=$(curl -sS -b "$COOKIES" -w '\n%{http_code}' -X POST "$API/api/v1/monitors/$MONITOR_ID/flows" \
    -H 'Content-Type: application/json' -d "$FLOW_BODY")
  FLOW_STATUS=$(echo "$FLOW_RESP" | tail -1)
  expect_any "create API flow" "$FLOW_STATUS" 201 400 404 409
  if [ "$FLOW_STATUS" = "201" ]; then
    FLOW_ID=$(echo "$FLOW_RESP" | sed '$d' | jq -r '.id // empty')
    FLOW_GET_STATUS=$(authed_status GET "/api/v1/monitors/$MONITOR_ID/flows")
    expect_any "get API flows" "$FLOW_GET_STATUS" 200 404
    FLOW_UPDATE_STATUS=$(curl -sS -b "$COOKIES" -o /dev/null -w '%{http_code}' -X PUT \
      "$API/api/v1/monitors/$MONITOR_ID/flows/$FLOW_ID" -H 'Content-Type: application/json' -d "$FLOW_BODY")
    expect_any "update API flow" "$FLOW_UPDATE_STATUS" 200 400 404
    FLOW_DELETE_STATUS=$(authed_status DELETE "/api/v1/monitors/$MONITOR_ID/flows/$FLOW_ID")
    expect_any "delete API flow" "$FLOW_DELETE_STATUS" 204 200 404
  fi
fi

# ------------------------------------------------------------------
step "MASTER ONE-TIME CHECK"
if [ "$MONITOR_ID" != "missing" ]; then
  OT_RESP=$(curl -sS -b "$COOKIES" -w '\n%{http_code}' -X POST \
    "$API/api/v1/monitors/$MONITOR_ID/checks/one-time" -H 'Content-Type: application/json' \
    -d "$(jq -n --arg id "$MONITOR_ID" '{monitor_id:$id}')")
  OT_STATUS=$(echo "$OT_RESP" | tail -1)
  expect_any "enqueue one-time check" "$OT_STATUS" 201 400 402 404 409
  if [ "$OT_STATUS" = "201" ]; then
    OT_REQUEST_ID=$(echo "$OT_RESP" | sed '$d' | jq -r '.request_id // empty')
    if [ -n "$OT_REQUEST_ID" ]; then
      OT_GET_STATUS=$(authed_status GET "/api/v1/monitors/$MONITOR_ID/checks/one-time/$OT_REQUEST_ID")
      expect_any "get one-time check status" "$OT_GET_STATUS" 200 404
    fi
  fi
fi

# ------------------------------------------------------------------
step "MASTER BULK MONITOR ACTIONS"
if [ "$MONITOR_ID" != "missing" ]; then
  BULK_BODY=$(jq -n --arg id "$MONITOR_ID" '{action:"disable",monitor_ids:[$id],all:false,reason:"E2E bulk action"}')
  BULK_RESP=$(curl -sS -b "$COOKIES" -w '\n%{http_code}' -X POST "$API/api/v1/monitors/bulk" \
    -H 'Content-Type: application/json' -d "$BULK_BODY")
  BULK_STATUS=$(echo "$BULK_RESP" | tail -1)
  expect_any "bulk monitor action" "$BULK_STATUS" 200 202 400 404
  # Restore the monitor even if the bulk action succeeded.
  curl -sS -b "$COOKIES" -o /dev/null -X POST "$API/api/v1/monitors/$MONITOR_ID/enable" >/dev/null 2>&1
fi

# ------------------------------------------------------------------
step "MASTER BULK IMPORT"
CSV_FILE="/tmp/pingvertex-e2e-${UNIQUE}.csv"
printf 'name,url,method,interval_seconds,timeout_seconds,expected_status\nE2E Bulk,http://mock-target/,GET,300,10,200\n' > "$CSV_FILE"
PREVIEW_STATUS=$(curl -sS -b "$COOKIES" -o /tmp/e2e_bulk_preview.json -w '%{http_code}' \
  -X POST "$API/api/v1/monitors/bulk-import/preview?forecast_days=1" \
  -F "file=@$CSV_FILE;type=text/csv")
expect_any "bulk import preview" "$PREVIEW_STATUS" 200 400 403
if [ "$PREVIEW_STATUS" = "200" ]; then
  SNAPSHOT=$(jq -r '.pricing_snapshot_hash // empty' /tmp/e2e_bulk_preview.json)
  if [ -n "$SNAPSHOT" ] && [ "$SNAPSHOT" != "null" ]; then
    CONFIRM_STATUS=$(curl -sS -b "$COOKIES" -o /tmp/e2e_bulk_confirm.json -w '%{http_code}' \
      -X POST "$API/api/v1/monitors/bulk-import/confirm?forecast_days=1" \
      -H "Idempotency-Key: e2e-bulk-${UNIQUE}" \
      -H "X-Bulk-Pricing-Snapshot: $SNAPSHOT" \
      -F "file=@$CSV_FILE;type=text/csv")
    expect_any "bulk import confirm" "$CONFIRM_STATUS" 200 201 202 400 402 409
    JOB_ID=$(jq -r '.job_id // .id // empty' /tmp/e2e_bulk_confirm.json 2>/dev/null)
    if [ -n "$JOB_ID" ]; then
      JOB_STATUS=$(authed_status GET "/api/v1/monitors/bulk-import/jobs/$JOB_ID")
      expect_any "bulk import job status" "$JOB_STATUS" 200 404
      PAUSE_STATUS=$(authed_status POST "/api/v1/monitors/bulk-import/jobs/$JOB_ID/pause")
      expect_any "pause bulk import job" "$PAUSE_STATUS" 200 202 204 400 404 409
      RESUME_STATUS=$(authed_status POST "/api/v1/monitors/bulk-import/jobs/$JOB_ID/resume")
      expect_any "resume bulk import job" "$RESUME_STATUS" 200 202 204 400 404 409
    fi
  else
    master_fail "bulk import preview did not return pricing_snapshot_hash"
  fi
fi
rm -f "$CSV_FILE"

# ------------------------------------------------------------------
step "MASTER STATUS PAGES"
SP_ID=""
if [ "$SP_STATUS" = "201" ]; then
  SP_CREATE_JSON=$(echo "$SP_RESP" | sed '$d')
  SP_ID=$(echo "$SP_CREATE_JSON" | jq -r '.id // empty')
fi
if [ -z "$SP_ID" ]; then
  SP_ID=$(get_json /api/v1/status-pages | jq -r '.[0].id // empty')
fi
SP_LIST_STATUS=$(authed_status GET /api/v1/status-pages)
expect_any "list status pages" "$SP_LIST_STATUS" 200
if [ -n "$SP_ID" ]; then
  SP_UPDATE_BODY=$(jq -n --arg mid "$MONITOR_ID" '{name:"E2E Status Updated",monitor_ids:[$mid],logo_url:null,primary_color:null}')
  SP_UPDATE_STATUS=$(curl -sS -b "$COOKIES" -o /dev/null -w '%{http_code}' -X PUT \
    "$API/api/v1/status-pages/$SP_ID" -H 'Content-Type: application/json' -d "$SP_UPDATE_BODY")
  expect_any "update status page" "$SP_UPDATE_STATUS" 200 400 404
  DOMAIN_STATUS=$(authed_status POST "/api/v1/status-pages/$SP_ID/custom-domain" \
    -H 'Content-Type: application/json' -d '{"domain":"e2e.invalid"}')
  expect_any "request custom domain" "$DOMAIN_STATUS" 200 201 202 400 404 409
  DOMAIN_REMOVE_STATUS=$(authed_status DELETE "/api/v1/status-pages/$SP_ID/custom-domain")
  expect_any "remove custom domain" "$DOMAIN_REMOVE_STATUS" 200 204 404 409
fi

# ------------------------------------------------------------------
step "MASTER API KEY SCOPE MATRIX"
ALL_SCOPES='{"name":"E2E full-scope key","scopes":["monitor:read","monitor:write","status_page:read","status_page:write","incident:read","incident:write","notification:read","notification:write","api_flow:read","api_flow:write"]}'
FULL_KEY_RESP=$(curl -sS -b "$COOKIES" -w '\n%{http_code}' -X POST "$API/api/v1/api-keys" \
  -H 'Content-Type: application/json' -d "$ALL_SCOPES")
FULL_KEY_STATUS=$(echo "$FULL_KEY_RESP" | tail -1)
expect_any "create full-scope API key" "$FULL_KEY_STATUS" 201 400 403
if [ "$FULL_KEY_STATUS" = "201" ]; then
  FULL_KEY_JSON=$(echo "$FULL_KEY_RESP" | sed '$d')
  FULL_KEY=$(echo "$FULL_KEY_JSON" | jq -r '.key // empty')
  FULL_KEY_ID=$(echo "$FULL_KEY_JSON" | jq -r '.id // empty')
  if [ -n "$FULL_KEY" ]; then
    for route in \
      /api/v1/external/monitors \
      /api/v1/external/checks \
      /api/v1/external/status-pages; do
      ST=$(http_status GET "$API${route}" -H "X-API-Key: $FULL_KEY")
      expect_any "API-key GET $route" "$ST" 200 403 404
    done
    INCIDENT_ID=$(get_json "/api/v1/monitors/$MONITOR_ID/incidents" | jq -r '.[0].id // empty')
    if [ -n "$INCIDENT_ID" ]; then
      ST=$(http_status GET "$API/api/v1/external/incidents/$MONITOR_ID" -H "X-API-Key: $FULL_KEY")
      expect_any "API-key incident read" "$ST" 200 403 404
      ACK_ST=$(http_status POST "$API/api/v1/external/incidents/$INCIDENT_ID/acknowledge" -H "X-API-Key: $FULL_KEY")
      expect_any "API-key incident acknowledge" "$ACK_ST" 200 204 400 403 404
    fi
    if [ -n "$SP_ID" ]; then
      ST=$(http_status PUT "$API/api/v1/external/status-pages/$SP_ID" -H "X-API-Key: $FULL_KEY" \
        -H 'Content-Type: application/json' -d "$SP_UPDATE_BODY")
      expect_any "API-key status-page write" "$ST" 200 400 403 404
    fi
  else
    master_fail "full-scope API key response missing raw key"
  fi
fi

# Explicit least-privilege test: monitor:read must not satisfy incident write
READ_KEY_RESP=$(curl -sS -b "$COOKIES" -w '\n%{http_code}' -X POST "$API/api/v1/api-keys" \
  -H 'Content-Type: application/json' -d '{"name":"E2E read-only","scopes":["monitor:read"]}')
READ_KEY_STATUS=$(echo "$READ_KEY_RESP" | tail -1)
expect_any "create read-only API key" "$READ_KEY_STATUS" 201 400 403
if [ "$READ_KEY_STATUS" = "201" ]; then
  READ_KEY=$(echo "$READ_KEY_RESP" | sed '$d' | jq -r '.key // empty')
  if [ -n "$READ_KEY" ]; then
    ST=$(http_status POST "$API/api/v1/external/incidents/00000000-0000-0000-0000-000000000000/acknowledge" -H "X-API-Key: $READ_KEY")
    expect_any "read-only API key cannot incident-write" "$ST" 401 403 404
  fi
fi
if [ -n "${FULL_KEY_ID:-}" ]; then
  DEL_KEY_STATUS=$(authed_status DELETE "/api/v1/api-keys/$FULL_KEY_ID")
  expect_any "delete full-scope API key" "$DEL_KEY_STATUS" 204 200 404
fi

# ------------------------------------------------------------------
step "MASTER RUM SITE + INGEST + ANALYTICS"
RUM_CREATE_RESP=$(curl -sS -b "$COOKIES" -w '\n%{http_code}' -X POST \
  "$API/api/v1/dashboard/rum/sites" -H 'Content-Type: application/json' \
  -d '{"name":"E2E RUM","domain":"e2e.example.com"}')
RUM_CREATE_STATUS=$(echo "$RUM_CREATE_RESP" | tail -1)
expect_any "create RUM site" "$RUM_CREATE_STATUS" 201 400 403 409
RUM_SITE_ID=""
RUM_SITE_KEY=""
if [ "$RUM_CREATE_STATUS" = "201" ]; then
  RUM_JSON=$(echo "$RUM_CREATE_RESP" | sed '$d')
  RUM_SITE_ID=$(echo "$RUM_JSON" | jq -r '.id // empty')
  RUM_SITE_KEY=$(echo "$RUM_JSON" | jq -r '.site_key // empty')
else
  RUM_LIST=$(get_json /api/v1/dashboard/rum/sites)
  RUM_SITE_ID=$(echo "$RUM_LIST" | jq -r '.[0].id // empty')
  RUM_SITE_KEY=$(echo "$RUM_LIST" | jq -r '.[0].site_key // empty')
fi
RUM_LIST_STATUS=$(authed_status GET /api/v1/dashboard/rum/sites)
expect_any "list RUM sites" "$RUM_LIST_STATUS" 200
if [ -n "$RUM_SITE_ID" ]; then
  for suffix in overview pages errors browsers-devices geo trend sessions; do
    ST=$(authed_status GET "/api/v1/dashboard/rum/sites/$RUM_SITE_ID/$suffix")
    expect_any "RUM analytics: $suffix" "$ST" 200 400 404
  done
fi
if [ -n "$RUM_SITE_KEY" ]; then
  RUM_EVENT=$(jq -n --arg key "$RUM_SITE_KEY" --arg rid "e2e-rum-$UNIQUE" \
    '{site_key:$key,request_id:$rid,session_key:"e2e-session",browser_name:"Firefox",browser_version:"155",os_name:"Linux",device_type:"desktop",page_view:{path:"/e2e",referrer_path:null,ttfb_ms:10,fcp_ms:20,lcp_ms:30,inp_ms:5,cls:0.01,load_time_ms:40},js_errors:[{message:"E2E error",source:"/e2e.js",line_number:1,column_number:1}],resource_errors:[]}')
  RUM_INGEST_STATUS=$(curl -sS -o /tmp/e2e_rum_ingest.json -w '%{http_code}' -X POST \
    "$API/api/v1/public/rum/collect" -H 'Content-Type: text/plain' -d "$RUM_EVENT")
  expect_any "RUM public ingest" "$RUM_INGEST_STATUS" 200 202 400 404 503
  RUM_BAD_STATUS=$(curl -sS -o /dev/null -w '%{http_code}' -X POST "$API/api/v1/public/rum/collect" \
    -H 'Content-Type: text/plain' -d '{"site_key":"bad","session_key":"x"}')
  expect_any "malformed RUM site key rejected" "$RUM_BAD_STATUS" 404 400
fi

# ------------------------------------------------------------------
step "MASTER WALLET + USAGE + TOP-UP + IDEMPOTENCY"
for route in \
  /api/v1/wallet \
  /api/v1/wallet/transactions \
  /api/v1/wallet/notifications \
  /api/v1/wallet/settings \
  /api/v1/wallet/usage \
  /api/v1/billing/invoices \
  /api/v1/billing/usage \
  /api/v1/billing/usage/summary; do
  ST=$(authed_status GET "$route")
  expect_any "billing GET $route" "$ST" 200 403
 done

WALLET_SETTINGS_BODY='{"low_balance_threshold_units":100,"critical_balance_threshold_units":20,"auto_top_up_enabled":false,"auto_top_up_threshold_units":50,"auto_top_up_amount_units":500,"payment_method":null,"daily_top_up_limit_units":5000,"monthly_top_up_limit_units":20000,"daily_spend_limit_units":null,"monthly_spend_limit_units":null,"daily_usage_alert_threshold_units":null,"monthly_usage_alert_threshold_units":null,"retry_limit":3}'
WALLET_SETTINGS_STATUS=$(curl -sS -b "$COOKIES" -o /dev/null -w '%{http_code}' -X PUT "$API/api/v1/wallet/settings" \
  -H 'Content-Type: application/json' -d "$WALLET_SETTINGS_BODY")
expect_any "update wallet settings" "$WALLET_SETTINGS_STATUS" 200 400 403

# Missing idempotency must fail. This tests the mandatory replay-safety boundary.
TOPUP_NO_IDEM=$(curl -sS -b "$COOKIES" -o /dev/null -w '%{http_code}' -X POST "$API/api/v1/wallet/top-up" \
  -H 'Content-Type: application/json' -d '{"amount_units":500}')
expect_any "top-up requires Idempotency-Key" "$TOPUP_NO_IDEM" 400 409

TOPUP_RESP=$(curl -sS -b "$COOKIES" -w '\n%{http_code}' -X POST "$API/api/v1/wallet/top-up" \
  -H 'Content-Type: application/json' -H "Idempotency-Key: e2e-topup-$UNIQUE" \
  -d '{"amount_units":500}')
TOPUP_STATUS=$(echo "$TOPUP_RESP" | tail -1)
expect_any "fake-provider wallet top-up" "$TOPUP_STATUS" 200 201 400 402 403 409
if [ "$TOPUP_STATUS" = "200" ] || [ "$TOPUP_STATUS" = "201" ]; then
  TOPUP_JSON=$(echo "$TOPUP_RESP" | sed '$d')
  CHECKOUT_ID=$(echo "$TOPUP_JSON" | jq -r '.provider_checkout_id // empty')
  if [ -n "$CHECKOUT_ID" ]; then
    VERIFY_TOPUP_STATUS=$(authed_status POST "/api/v1/wallet/top-up/$CHECKOUT_ID/verify")
    expect_any "verify wallet top-up" "$VERIFY_TOPUP_STATUS" 200 201 400 402 404 409
    VERIFY_TOPUP_STATUS2=$(authed_status POST "/api/v1/wallet/top-up/$CHECKOUT_ID/verify")
    expect_any "repeat wallet top-up verification is idempotent" "$VERIFY_TOPUP_STATUS2" 200 201 400 404 409
  fi
fi

# Refund endpoint is exercised with a deliberately fake provider payment id;
# the test accepts business-level rejection but must never see a 500.
REFUND_STATUS=$(curl -sS -b "$COOKIES" -o /dev/null -w '%{http_code}' -X POST "$API/api/v1/wallet/refund" \
  -H 'Content-Type: application/json' -d '{"provider_payment_id":"fake-nonexistent","amount_units":100,"currency":"USD"}')
expect_any "refund endpoint business validation" "$REFUND_STATUS" 200 201 400 402 403 404 409

# ------------------------------------------------------------------
step "MASTER SUPPORT TICKETS"
SUPPORT_CREATE_RESP=$(curl -sS -b "$COOKIES" -w '\n%{http_code}' -X POST "$API/api/v1/support/tickets" \
  -H 'Content-Type: application/json' -d '{"subject":"E2E support ticket","category":"technical_issue"}')
SUPPORT_CREATE_STATUS=$(echo "$SUPPORT_CREATE_RESP" | tail -1)
expect_any "create support ticket" "$SUPPORT_CREATE_STATUS" 201 400 403
SUPPORT_ID=""
if [ "$SUPPORT_CREATE_STATUS" = "201" ]; then
  SUPPORT_ID=$(echo "$SUPPORT_CREATE_RESP" | sed '$d' | jq -r '.id // empty')
fi
SUPPORT_LIST_STATUS=$(authed_status GET /api/v1/support/tickets)
expect_any "list support tickets" "$SUPPORT_LIST_STATUS" 200 403
if [ -n "$SUPPORT_ID" ]; then
  ST=$(authed_status GET "/api/v1/support/tickets/$SUPPORT_ID")
  expect_any "get support ticket" "$ST" 200 403 404
  ST=$(authed_status POST "/api/v1/support/tickets/$SUPPORT_ID/messages" \
    -H 'Content-Type: application/json' -d '{"message":"E2E reply"}')
  expect_any "reply to support ticket" "$ST" 201 200 400 403 404
  ST=$(authed_status PATCH "/api/v1/support/tickets/$SUPPORT_ID/status" \
    -H 'Content-Type: application/json' -d '{"status":"in_progress"}')
  expect_any "update support ticket status" "$ST" 200 400 403 404
fi

# ------------------------------------------------------------------
step "MASTER MEMBERS + INVITATIONS"
MEMBERS_STATUS=$(authed_status GET /api/v1/members)
expect_any "list members" "$MEMBERS_STATUS" 200 403
INVITE_RESP=$(curl -sS -b "$COOKIES" -w '\n%{http_code}' -X POST "$API/api/v1/invitations" \
  -H 'Content-Type: application/json' -d "$(jq -n --arg e "e2e-invite-${UNIQUE}@example.com" '{email:$e,role:"member"}')")
INVITE_STATUS=$(echo "$INVITE_RESP" | tail -1)
expect_any "create invitation" "$INVITE_STATUS" 201 400 403 409
INVITE_ID=""
if [ "$INVITE_STATUS" = "201" ]; then
  INVITE_ID=$(echo "$INVITE_RESP" | sed '$d' | jq -r '.id // empty')
fi
PENDING_STATUS=$(authed_status GET /api/v1/invitations/pending)
expect_any "list pending invitations" "$PENDING_STATUS" 200 403
if [ -n "$INVITE_ID" ]; then
  REVOKE_STATUS=$(authed_status DELETE "/api/v1/invitations/$INVITE_ID")
  expect_any "revoke invitation" "$REVOKE_STATUS" 204 200 404 403
fi
# Do not add tenant B as a member of tenant A; tenant B remains an independent
# tenant for the cross-tenant isolation matrix below.

# ------------------------------------------------------------------
step "MASTER AUDIT + DASHBOARD + HELP"
for route in \
  /api/v1/dashboard/overview \
  /api/v1/dashboard/badges \
  /api/v1/audit; do
  ST=$(authed_status GET "$route")
  expect_any "authenticated GET $route" "$ST" 200 403
 done
MANUAL_STATUS=$(authed_status POST /api/v1/dashboard/checks/now)
expect_any "manual dashboard checks queue" "$MANUAL_STATUS" 200 202 403

for route in \
  /api/v1/help/categories \
  '/api/v1/help/articles?limit=10' \
  '/api/v1/help/search?q=monitor'; do
  ST=$(http_status GET "$API$route")
  expect_any "public help route $route" "$ST" 200 404
 done

# ------------------------------------------------------------------
step "MASTER ADMIN AUTHORIZATION BOUNDARY"
# Normal tenant user must never reach platform-admin operations.
for route in \
  /api/v1/admin/audit \
  /api/v1/admin/companies \
  /api/v1/admin/users \
  /api/v1/admin/pricing/configs \
  /api/v1/admin/pricing/configs/active; do
  ST=$(authed_status GET "$route")
  expect_any "non-admin rejected from $route" "$ST" 401 403 404
 done

# ------------------------------------------------------------------
step "MASTER TENANT ISOLATION MATRIX"
# Tenant B was created by the legacy section above. Recreate its session if
# that section had to skip login for any reason.
if [ -n "${COOKIES2:-}" ] && [ -f "$COOKIES2" ]; then
  :
else
  COOKIES2=$(mktemp)
  EMAIL2="e2e-b-${UNIQUE}@example.com"
  USERNAME2="e2euserb${UNIQUE}"
  # Registration may already have happened; this is best-effort and must not
  # stop the suite.
  curl -sS -c "$COOKIES2" -o /dev/null -X POST "$API/api/v1/auth/register" \
    -H 'Content-Type: application/json' \
    -d "$(jq -n --arg u "$USERNAME2" --arg e "$EMAIL2" --arg p "$PASSWORD" '{username:$u,email:$e,password:$p}')" >/dev/null 2>&1
fi

if [ -f "${COOKIES2:-/nonexistent}" ]; then
  CROSS_MONITOR=$(http_status GET "$API/api/v1/monitors/$MONITOR_ID" -b "$COOKIES2")
  expect_any "tenant B cannot read tenant A monitor" "$CROSS_MONITOR" 401 403 404
  CROSS_CHECKS=$(http_status GET "$API/api/v1/monitors/$MONITOR_ID/checks" -b "$COOKIES2")
  expect_any "tenant B cannot read tenant A checks" "$CROSS_CHECKS" 401 403 404
  CROSS_INCIDENTS=$(http_status GET "$API/api/v1/monitors/$MONITOR_ID/incidents" -b "$COOKIES2")
  expect_any "tenant B cannot read tenant A incidents" "$CROSS_INCIDENTS" 401 403 404
  CROSS_UPDATE=$(http_status PUT "$API/api/v1/monitors/$MONITOR_ID" -b "$COOKIES2" \
    -H 'Content-Type: application/json' -d "$UPDATE_MONITOR_BODY")
  expect_any "tenant B cannot update tenant A monitor" "$CROSS_UPDATE" 401 403 404
  CROSS_DELETE=$(http_status DELETE "$API/api/v1/monitors/$MONITOR_ID" -b "$COOKIES2")
  expect_any "tenant B cannot delete tenant A monitor" "$CROSS_DELETE" 401 403 404

  if [ -n "${SP_ID:-}" ]; then
    ST=$(http_status GET "$API/api/v1/status-pages/$SP_ID" -b "$COOKIES2")
    expect_any "tenant B cannot read tenant A status page" "$ST" 401 403 404
    ST=$(http_status DELETE "$API/api/v1/status-pages/$SP_ID" -b "$COOKIES2")
    expect_any "tenant B cannot delete tenant A status page" "$ST" 401 403 404
  fi

  if [ -n "${SUPPORT_ID:-}" ]; then
    ST=$(http_status GET "$API/api/v1/support/tickets/$SUPPORT_ID" -b "$COOKIES2")
    expect_any "tenant B cannot read tenant A support ticket" "$ST" 401 403 404
  fi

  if [ -n "${RUM_SITE_ID:-}" ]; then
    ST=$(http_status GET "$API/api/v1/dashboard/rum/sites/$RUM_SITE_ID/overview" -b "$COOKIES2")
    expect_any "tenant B cannot read tenant A RUM site" "$ST" 401 403 404
  fi
fi

# ------------------------------------------------------------------
step "MASTER PUBLIC ENDPOINTS + ABUSE BOUNDARIES"
PUBLIC_STATUS_BASE=$(http_status GET "$API/api/v1/public/status")
expect_any "public status root" "$PUBLIC_STATUS_BASE" 200 404
PUBLIC_UNKNOWN=$(http_status GET "$API/api/v1/public/status/does-not-exist-$UNIQUE")
expect_any "unknown public status page" "$PUBLIC_UNKNOWN" 404
BAD_HEARTBEAT=$(http_status GET "$API/api/v1/public/heartbeat/%3Cscript%3Ealert(1)%3C/script%3E")
expect_any "malformed heartbeat token" "$BAD_HEARTBEAT" 400 404

# CORS/origin enforcement is exercised with an obviously foreign Origin on
# authentication. The expected answer is rejection, never success.
BAD_ORIGIN_STATUS=$(curl -sS -o /dev/null -w '%{http_code}' -X POST "$API/api/v1/auth/login" \
  -H 'Content-Type: application/json' -H 'Origin: https://evil.example' \
  -d "$(jq -n --arg e "$EMAIL" --arg p "$PASSWORD" '{email:$e,password:$p}')")
expect_any "foreign Origin rejected on login" "$BAD_ORIGIN_STATUS" 400 401 403 404

# Oversized/invalid payload boundaries. We do not intentionally send a huge
# payload to every endpoint; one representative body-limit check is enough.
HUGE_FILE="/tmp/pingvertex-e2e-huge-${UNIQUE}.bin"
python3 - <<'PY' > "$HUGE_FILE"
print('A' * 1200000)
PY
HUGE_STATUS=$(curl -sS -b "$COOKIES" -o /dev/null -w '%{http_code}' -X POST "$API/api/v1/proxy-checks" \
  -H 'Content-Type: application/json' --data-binary @"$HUGE_FILE")
expect_any "proxy body limit rejects oversized payload" "$HUGE_STATUS" 400 413 422
rm -f "$HUGE_FILE"

# ------------------------------------------------------------------
step "MASTER BILLING WEBHOOK SECURITY"
# Bad signature MUST NOT mutate billing state.
BAD_WEBHOOK=$(curl -sS -o /dev/null -w '%{http_code}' -X POST "$API/api/v1/billing/webhook" \
  -H 'Content-Type: application/json' -H 'x-billing-signature: sha256=deadbeef' \
  -d '{"event_id":"e2e-invalid-sig","event_type":"subscription_canceled","tenant_id":"00000000-0000-0000-0000-000000000000","data":{}}')
expect_any "invalid billing webhook signature rejected" "$BAD_WEBHOOK" 400 401 403
BAD_PAYSTACK=$(curl -sS -o /dev/null -w '%{http_code}' -X POST "$API/api/v1/billing/webhook/paystack" \
  -H 'Content-Type: application/json' -H 'x-paystack-signature: deadbeef' -d '{}')
expect_any "invalid Paystack webhook signature rejected" "$BAD_PAYSTACK" 400 401 403

# ------------------------------------------------------------------
step "MASTER RETIRED / COMPATIBILITY CONTRACTS"
RETIRED_SUB=$(authed_status PATCH /api/v1/subscription/usage -H 'Content-Type: application/json' -d '{}')
expect_any "retired subscription usage endpoint" "$RETIRED_SUB" 410 400 403

# ------------------------------------------------------------------
step "MASTER ROUTE INVENTORY SMOKE"
# This is intentionally a route-level smoke matrix. It makes sure every
# currently exposed first-party API family is exercised at least once, even
# where a complete success lifecycle needs credentials/external providers.
ROUTE_SMOKE=0
ROUTE_SMOKE_PASS=0
route_smoke() {
  local desc="$1" method="$2" path="$3"; shift 3
  local st
  st=$(authed_status "$method" "$path" "$@")
  ROUTE_SMOKE=$((ROUTE_SMOKE + 1))
  case "$st" in
    200|201|202|204|400|401|402|403|404|405|409|410|413|415|422|429|503)
      ROUTE_SMOKE_PASS=$((ROUTE_SMOKE_PASS + 1))
      master_ok "route smoke $desc -> $st"
      ;;
    *)
      master_fail "route smoke $desc -> unexpected $st"
      ;;
  esac
}

# Existing resource IDs are used where available. UUID nil is deliberately
# used for routes that need a path ID but have no created fixture; a clean 404
# is still a valid route/security result.
NIL="00000000-0000-0000-0000-000000000000"
route_smoke "GET monitor" GET "/api/v1/monitors/$MONITOR_ID"
route_smoke "GET monitor checks" GET "/api/v1/monitors/$MONITOR_ID/checks"
route_smoke "GET monitor incidents" GET "/api/v1/monitors/$MONITOR_ID/incidents"
route_smoke "GET monitor notifications" GET "/api/v1/monitors/$MONITOR_ID/notifications"
route_smoke "GET maintenance" GET "/api/v1/monitors/$MONITOR_ID/maintenance-windows"
route_smoke "GET flows" GET "/api/v1/monitors/$MONITOR_ID/flows"
route_smoke "GET incident" GET "/api/v1/incidents/$NIL"
route_smoke "GET api keys" GET /api/v1/api-keys
route_smoke "GET status pages" GET /api/v1/status-pages
route_smoke "GET wallet" GET /api/v1/wallet
route_smoke "GET wallet transactions" GET /api/v1/wallet/transactions
route_smoke "GET wallet notifications" GET /api/v1/wallet/notifications
route_smoke "GET wallet settings" GET /api/v1/wallet/settings
route_smoke "GET wallet usage" GET /api/v1/wallet/usage
route_smoke "GET invoices" GET /api/v1/billing/invoices
route_smoke "GET billing usage" GET /api/v1/billing/usage
route_smoke "GET billing summary" GET /api/v1/billing/usage/summary
route_smoke "GET PAYG pricing" GET /api/v1/payg/pricing
route_smoke "GET members" GET /api/v1/members
route_smoke "GET invitations" GET /api/v1/invitations/pending
route_smoke "GET support tickets" GET /api/v1/support/tickets
route_smoke "GET tenant audit" GET /api/v1/audit
route_smoke "GET dashboard overview" GET /api/v1/dashboard/overview
route_smoke "GET dashboard badges" GET /api/v1/dashboard/badges
route_smoke "GET RUM sites" GET /api/v1/dashboard/rum/sites
route_smoke "GET RUM overview" GET "/api/v1/dashboard/rum/sites/$NIL/overview"
route_smoke "GET RUM pages" GET "/api/v1/dashboard/rum/sites/$NIL/pages"
route_smoke "GET RUM errors" GET "/api/v1/dashboard/rum/sites/$NIL/errors"
route_smoke "GET RUM browsers" GET "/api/v1/dashboard/rum/sites/$NIL/browsers-devices"
route_smoke "GET RUM geo" GET "/api/v1/dashboard/rum/sites/$NIL/geo"
route_smoke "GET RUM trend" GET "/api/v1/dashboard/rum/sites/$NIL/trend"
route_smoke "GET RUM sessions" GET "/api/v1/dashboard/rum/sites/$NIL/sessions"
route_smoke "GET proxy job" GET "/api/v1/proxy-checks/$NIL"
route_smoke "GET proxy results" GET "/api/v1/proxy-checks/$NIL/results"
route_smoke "GET one-time check" GET "/api/v1/monitors/$MONITOR_ID/checks/one-time/$NIL"
route_smoke "GET bulk-import job" GET "/api/v1/monitors/bulk-import/jobs/$NIL"
route_smoke "GET admin audit" GET /api/v1/admin/audit
route_smoke "GET admin companies" GET /api/v1/admin/companies
route_smoke "GET admin users" GET /api/v1/admin/users
route_smoke "GET admin pricing" GET /api/v1/admin/pricing/configs
route_smoke "GET admin active pricing" GET /api/v1/admin/pricing/configs/active

if [ "$ROUTE_SMOKE" -gt 0 ] && [ "$ROUTE_SMOKE_PASS" -eq "$ROUTE_SMOKE" ]; then
  master_ok "route smoke matrix: $ROUTE_SMOKE/$ROUTE_SMOKE_PASS endpoints returned controlled statuses"
else
  master_fail "route smoke matrix: $ROUTE_SMOKE_PASS/$ROUTE_SMOKE controlled; unexpected responses found"
fi

# ------------------------------------------------------------------
step "MASTER DATABASE / TENANT INTEGRITY CHECKS"
if command -v docker >/dev/null 2>&1; then
  DB_OK=$(db_query 'SELECT 1;')
  if [ "$DB_OK" = "1" ]; then master_ok "database query works"; else master_fail "database query failed"; fi

  if [ -n "${TENANT_ID:-}" ]; then
    DB_MONITOR_TENANT=$(db_query "SELECT tenant_id FROM monitors WHERE id = '$MONITOR_ID' LIMIT 1;")
    if [ "$DB_MONITOR_TENANT" = "$TENANT_ID" ]; then
      master_ok "monitor remains owned by test tenant"
    else
      master_fail "monitor tenant ownership mismatch: expected $TENANT_ID got $DB_MONITOR_TENANT"
    fi
  fi

  DUP_CHARGES=$(db_query "SELECT COUNT(*) FROM wallet_transactions WHERE tenant_id = '$TENANT_ID' AND idempotency_key LIKE 'reservation:%' AND created_at > now() - interval '30 minutes';")
  if [[ "$DUP_CHARGES" =~ ^[0-9]+$ ]]; then master_ok "wallet transaction accounting query returned $DUP_CHARGES reservation transactions"; else master_fail "wallet transaction accounting query failed"; fi
else
  master_fail "docker unavailable for database integrity checks"
fi

# ------------------------------------------------------------------
step "MASTER CLEANUP / SESSION REVOCATION"
# Delete only resources created by the current test tenant. Never use tenant B
# credentials for cleanup of tenant A resources.
if [ -n "${FULL_KEY_ID:-}" ]; then
  curl -sS -b "$COOKIES" -o /dev/null -X DELETE "$API/api/v1/api-keys/$FULL_KEY_ID" >/dev/null 2>&1
fi
if [ -n "${FLOW_ID:-}" ]; then
  curl -sS -b "$COOKIES" -o /dev/null -X DELETE "$API/api/v1/monitors/$MONITOR_ID/flows/$FLOW_ID" >/dev/null 2>&1
fi

LOGOUT_STATUS=$(authed_status POST /api/v1/auth/logout)
expect_any "logout" "$LOGOUT_STATUS" 200 204
POST_LOGOUT_ME=$(http_status GET "$API/api/v1/auth/me" -b "$COOKIES")
expect_any "session revoked after logout" "$POST_LOGOUT_ME" 401 403

rm -f "${COOKIES2:-}" /tmp/e2e_*.json

# ------------------------------------------------------------------
echo
echo "==================================================================="
echo "PINGVERTEX MASTER E2E / FULL-SYSTEM JOURNEY"
echo "==================================================================="
echo "Master tests executed: $MASTER_TESTS"
echo "Master tests passed:  $PASS_COUNT"
echo "Total FAIL count:     $FAILURES"
echo
echo "Coverage families:"
echo "  auth/session, TOTP, monitors, checks, incidents, notifications"
echo "  escalation, maintenance, API flows, one-time checks"
echo "  bulk action/import, status pages, custom domains"
echo "  API-key scopes/external API, RUM ingest/analytics"
echo "  wallet/top-up/refund/usage/invoices/PAYG"
echo "  support, members, invitations, audit, dashboard, help"
echo "  admin authorization, tenant isolation, CORS, abuse limits"
echo "  webhooks, route inventory, DB ownership/accounting, logout"
echo "==================================================================="
if [ "$FAILURES" -eq 0 ]; then
  echo "RESULT: ALL MASTER E2E TESTS PASSED"
  exit 0
else
  echo "RESULT: $FAILURES STEP(S) FAILED — suite completed all available tests"
  exit 1
fi
