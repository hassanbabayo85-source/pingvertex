#!/usr/bin/env bash
# Sources .env from the repository root and then runs trunk serve, so that
# option_env!() in Rust can see GOOGLE_CLIENT_ID at build time.
set -a
source ../../.env
set +a
echo "Using GOOGLE_CLIENT_ID: $GOOGLE_CLIENT_ID"
trunk serve
