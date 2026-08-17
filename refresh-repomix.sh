#!/bin/bash
set -e  # tsaya nan take idan wani umarni ya kasa

rm -f overview.xml sashi-*.xml

repomix --style xml --compress --output overview.xml
repomix --include "crates/**" --style xml --output sashi-1-crates.xml
repomix --include "apps/**" --style xml --output sashi-2-apps.xml
repomix --include "dashboard/**" --style xml --compress --output sashi-3-dashboard.xml
repomix --include "configs/**,migrations/**" --style xml --output sashi-4-configs.xml
repomix --include "Dockerfile.*,docker-compose.yml,scripts/**,monitoring/**" --style xml --output sashi-5-deploy.xml

echo "✅ An sabunta duk files. Sakamako:"
ls -lh overview.xml sashi-*.xml
