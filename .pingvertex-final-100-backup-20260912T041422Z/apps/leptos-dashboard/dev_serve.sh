#!/usr/bin/env bash
# Yana source din .env daga root sannan yana gudanar da trunk serve
# domin option_env!() a Rust ya ga GOOGLE_CLIENT_ID a build time.
set -a
source ../../.env
set +a
echo "GOOGLE_CLIENT_ID da za a yi amfani da shi: $GOOGLE_CLIENT_ID"
trunk serve
