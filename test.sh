#!/usr/bin/env bash
# Wrapper: yana loda .env.test sannan ya gudanar da cargo test akan uptime_monitor_test
# Amfani: ./test.sh                -> cargo test --workspace
#         ./test.sh --test foo -- --ignored   -> ana iya wuce wa cargo test kowane argument

set -euo pipefail

ENV_FILE=".env.test"

if [ ! -f "$ENV_FILE" ]; then
  echo "GAGGAWA: ba a sami $ENV_FILE ba a $(pwd). Ka tabbata kana cikin tushen repo." >&2
  exit 1
fi

set -a
source "$ENV_FILE"
set +a

echo "Ana amfani da DATABASE_URL daga $ENV_FILE (uptime_monitor_test)..."
cargo test --workspace "$@"
