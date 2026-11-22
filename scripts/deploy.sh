#!/usr/bin/env bash
set -euo pipefail

# Deploy the latest code on the server. Run from anywhere.
cd ~/projects/middlegame.danbock.net

git pull

# Build the frontend's static assets into ./frontend/dist on the host
# (the backend serves them from there; see docker-compose.yml).
docker compose build frontend
docker compose run --rm frontend

# Rebuild the backend image and restart it. Schema changes are applied
# automatically at startup (db.rs runs CREATE TABLE IF NOT EXISTS on boot).
docker compose up -d --build backend

"$(dirname "$0")/check-prod.sh"
