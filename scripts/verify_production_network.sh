#!/usr/bin/env bash
# Production network hardening verification.
#
# Run this on the production host AFTER `docker compose ... up -d`.
#
# It is NON-DESTRUCTIVE: it only inspects running containers and does
# not modify any state. It exits 0 when every check passes, non-zero
# otherwise, so it can be wired into a deploy pipeline.
#
# Checks:
#   1. No service publishes port 3443 to the host.
#   2. The API container publishes NO ports in production.
#   3. The mTLS network contains only `api` and `agent`.
#   4. The agent's mTLS private key is a runtime mount, not baked into
#      the image.
#   5. Every application container runs as a non-root user.
#   6. Capabilities are dropped on hardened services.
#   7. `no-new-privileges` is set on every application container.
#
# NOTE: deliberately NO `set -e` — we want to collect ALL failures, not
# stop at the first one. The script exits non-zero at the end if any
# check failed.

set -u

FAIL=0
pass() { printf '  \033[32mPASS\033[0m %s\n' "$1"; }
fail() { printf '  \033[31mFAIL\033[0m %s\n' "$1"; FAIL=1; }
info() { printf '       %s\n' "$1"; }

# Prefer Compose v2; fail early if unavailable.
if ! docker compose version >/dev/null 2>&1; then
    echo "ERROR: 'docker compose' (v2) is required for this verification." >&2
    exit 2
fi

echo "===== PART 13 — production network hardening verification ====="
echo

# ---------------------------------------------------------------
# Check 1: port 3443 must not appear in any published port mapping.
# ---------------------------------------------------------------
echo "[1] No service publishes port 3443 to the host"
published=$(docker ps --format '{{.Names}}\t{{.Ports}}' 2>/dev/null \
    | grep -E ':3443->' || true)
if [ -z "$published" ]; then
    pass "no container publishes :3443"
else
    fail "some container publishes :3443:"
    printf '%s\n' "$published" | sed 's/^/         /'
fi
echo

# ---------------------------------------------------------------
# Check 2: the api container must not publish ANY port in production.
# ---------------------------------------------------------------
echo "[2] API container publishes no ports (Caddy is the only ingress)"
api_id=$(docker compose ps -q api 2>/dev/null | head -1 || true)
if [ -z "$api_id" ]; then
    fail "api container is not running (or not in this compose project)"
else
    api_ports=$(docker inspect "$api_id" --format '{{json .NetworkSettings.Ports}}' 2>/dev/null || echo '{}')
    if [ "$api_ports" = "{}" ] || [ -z "$api_ports" ]; then
        pass "api container publishes nothing"
    else
        fail "api container publishes: $api_ports"
    fi
fi
echo

# ---------------------------------------------------------------
# Check 3: the mTLS network contains only `api` and `agent`.
# ---------------------------------------------------------------
echo "[3] The mTLS network contains only 'api' and 'agent'"
if [ -n "$api_id" ]; then
    net_name=$(docker inspect "$api_id" \
        --format '{{range $k, $_ := .NetworkSettings.Networks}}{{$k}}{{"\n"}}{{end}}' 2>/dev/null \
        | grep 'pv_agent_mtls' | head -1 || true)
    if [ -z "$net_name" ]; then
        fail "api container is not attached to a *pv_agent_mtls* network"
    else
        members=$(docker network inspect "$net_name" \
            --format '{{range .Containers}}{{.Name}}{{"\n"}}{{end}}' 2>/dev/null || true)
        unexpected=$(printf '%s\n' "$members" \
            | grep -v -E '^uptime-monitor-(api|agent)$' \
            | grep -v '^$' || true)
        if [ -z "$unexpected" ]; then
            pass "pv_agent_mtls contains only api and agent"
        else
            fail "pv_agent_mtls has unexpected members:"
            printf '%s\n' "$unexpected" | sed 's/^/         /'
        fi
    fi
else
    fail "cannot inspect networks (api container not running)"
fi
echo

# ---------------------------------------------------------------
# Check 4: agent private key is a runtime mount, not baked in.
# ---------------------------------------------------------------
echo "[4] Agent mTLS private key is a runtime mount, not baked into the image"
agent_id=$(docker compose ps -q agent 2>/dev/null | head -1 || true)
if [ -z "$agent_id" ]; then
    fail "agent container is not running (or not in this compose project)"
else
    key_dest="/app/certs/agents/agent-us-east-1/client-key.pem"
    mount_type=$(docker inspect "$agent_id" \
        --format "{{range .Mounts}}{{if eq .Destination \"$key_dest\"}}{{.Type}}{{end}}{{end}}" \
        2>/dev/null || true)
    if [ "$mount_type" = "bind" ] || [ "$mount_type" = "volume" ]; then
        pass "private key is a runtime mount (type=$mount_type), not baked into the image"
    else
        fail "private key is NOT a runtime mount (type='$mount_type'); it may be baked into the image"
    fi
fi
echo

# ---------------------------------------------------------------
# Check 5: every application container runs as non-root.
# ---------------------------------------------------------------
echo "[5] Every application container runs as non-root (uid != 0)"
for name in uptime-monitor-api uptime-monitor-worker uptime-monitor-agent uptime-monitor-migrate uptime-monitor-caddy; do
    id=$(docker ps --filter "name=^${name}$" -q 2>/dev/null || true)
    if [ -z "$id" ]; then
        info "skip $name (not running)"
        continue
    fi
    uid=$(docker exec "$id" id -u 2>/dev/null || echo "unknown")
    if [ "$uid" = "0" ]; then
        fail "$name runs as root (uid=0)"
    elif [ "$uid" = "unknown" ]; then
        info "skip $name (cannot exec, e.g. distroless)"
    else
        pass "$name runs as uid=$uid"
    fi
done
echo

# ---------------------------------------------------------------
# Check 6: capabilities dropped on hardened services.
# ---------------------------------------------------------------
echo "[6] Capabilities are dropped on hardened services"
for name in uptime-monitor-api uptime-monitor-worker uptime-monitor-agent; do
    id=$(docker ps --filter "name=^${name}$" -q 2>/dev/null || true)
    if [ -z "$id" ]; then
        info "skip $name (not running)"
        continue
    fi
    capdrop=$(docker inspect "$id" --format '{{.HostConfig.CapDrop}}' 2>/dev/null || true)
    if printf '%s' "$capdrop" | grep -qi 'ALL'; then
        pass "$name drops ALL capabilities"
    else
        fail "$name does not drop ALL capabilities (got: $capdrop)"
    fi
done
echo

# ---------------------------------------------------------------
# Check 7: no-new-privileges on every application container.
# ---------------------------------------------------------------
echo "[7] no-new-privileges is set on every application container"
for name in uptime-monitor-api uptime-monitor-worker uptime-monitor-agent uptime-monitor-caddy; do
    id=$(docker ps --filter "name=^${name}$" -q 2>/dev/null || true)
    if [ -z "$id" ]; then
        info "skip $name (not running)"
        continue
    fi
    opts=$(docker inspect "$id" --format '{{.HostConfig.SecurityOpt}}' 2>/dev/null || true)
    if printf '%s' "$opts" | grep -q 'no-new-privileges'; then
        pass "$name has no-new-privileges:true"
    else
        fail "$name is missing no-new-privileges (got: $opts)"
    fi
done
echo

echo "===== done ====="
if [ "$FAIL" -eq 0 ]; then
    printf '\033[32mAll PART 13 checks passed.\033[0m\n'
    exit 0
else
    printf '\033[31mSome PART 13 checks FAILED — see above.\033[0m\n'
    exit 1
fi
