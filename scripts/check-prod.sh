#!/usr/bin/env bash
set -euo pipefail

# Post-deploy sanity check: the site must be reachable over public HTTPS.
# Run after deploying. Run from anywhere.

echo "Waiting for https://middlegame.danbock.net/ to come up..."
for _ in $(seq 1 30); do
  if curl -sf -o /dev/null https://middlegame.danbock.net/; then
    echo "Site is up."
    exit 0
  fi
  sleep 1
done

echo "FAILED: https://middlegame.danbock.net/ did not come up within 30s." >&2
echo "Check 'docker compose logs backend' on the server." >&2
exit 1
