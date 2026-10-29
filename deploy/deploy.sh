#!/usr/bin/env sh
# Compatible with Compose versions lacking `up --wait`.
set -eu
if [ "$#" -ne 1 ] || ! printf '%s\n' "$1" | LC_ALL=C grep -Eq '^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'; then
    echo 'Usage: ./deploy/deploy.sh vX.Y.Z (for example v1.0.1)' >&2
    exit 2
fi
# Environment wins over .env interpolation; never deploy an implicit latest tag.
TODOER_IMAGE="ghcr.io/mattmezza/todoer.pro:$1"
export TODOER_IMAGE
cd "$(dirname "$0")"
[ -f .env ] && [ -f app.env ] || {
    echo 'Configure deploy/.env and deploy/app.env first; see deploy/README.md.' >&2
    exit 2
}
# Compose reads COMPOSE_FILE from .env, including the optional replica overlay.
docker compose --project-name todoer config --quiet
docker compose --project-name todoer pull
docker compose --project-name todoer up -d --remove-orphans
attempt=0
while [ "$attempt" -lt 60 ]; do
    id=$(docker compose --project-name todoer ps -q app)
    if [ -n "$id" ]; then
        health=$(docker inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}' "$id")
        case "$health" in
            healthy) echo 'Todoer is healthy. Verify the public HTTPS endpoint next.'; exit 0 ;;
            unhealthy|exited|dead) echo 'Todoer failed health checks. Inspect docker compose logs app; no automatic data rollback performed.' >&2; exit 1 ;;
        esac
    fi
    attempt=$((attempt + 1))
    sleep 2
done
echo 'Timed out waiting for Todoer health. Inspect docker compose logs app.' >&2
exit 1
