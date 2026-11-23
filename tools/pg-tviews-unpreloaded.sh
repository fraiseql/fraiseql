#!/usr/bin/env bash
# A pg_tviews database whose library is no longer preloaded, for the `/metrics` case of
# tviews_cascade_e2e_pg that reads TVIEWS_UNPRELOADED_DATABASE_URL (#1551).
#
#   tools/pg-tviews-unpreloaded.sh up     # prints the URL to export
#   tools/pg-tviews-unpreloaded.sh down
#
# Runs docker/pg-tviews/unpreloaded-entrypoint.sh, which the Dagger leg runs too, in the
# image `postgres-tviews-test` builds, so the pg_tviews pin stays in
# docker/pg-tviews/Dockerfile. The container carries the label
# `fraiseql-pg-tviews-unpreloaded`, and `down` removes exactly that.
# PG_TVIEWS_UNPRELOADED_PORT (default 5442) is the host port.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
compose=(docker compose -f "$root/docker/docker-compose.test.yml")
label=fraiseql-pg-tviews-unpreloaded
name=fraiseql-pg-tviews-unpreloaded
port="${PG_TVIEWS_UNPRELOADED_PORT:-5442}"

down() {
    docker ps -aq --filter "label=$label" | xargs -r docker rm -f >/dev/null
}

up() {
    down
    "${compose[@]}" build -q postgres-tviews-test
    image="$("${compose[@]}" config --images postgres-tviews-test)"
    docker run -d --name "$name" --label "$label" --user postgres \
        -e POSTGRES_DB=test_fraiseql -e POSTGRES_USER=fraiseql_test \
        -e POSTGRES_PASSWORD=fraiseql_test_password \
        -v "$root/docker/pg-tviews/unpreloaded-entrypoint.sh:/unpreloaded-entrypoint.sh:ro" \
        -p "127.0.0.1:$port:5432" --entrypoint bash "$image" /unpreloaded-entrypoint.sh \
        >/dev/null
    for _ in $(seq 120); do
        if docker exec "$name" pg_isready -q -h 127.0.0.1 -U fraiseql_test -d test_fraiseql; then
            echo "postgresql://fraiseql_test:fraiseql_test_password@localhost:$port/test_fraiseql"
            return 0
        fi
        if [ "$(docker inspect -f '{{.State.Running}}' "$name")" != true ]; then
            break
        fi
        sleep 1
    done
    echo "$name did not become ready" >&2
    docker logs "$name" >&2
    return 1
}

case "${1:-}" in
up) up ;;
down) down ;;
*) echo "usage: $0 up|down" >&2; exit 2 ;;
esac
