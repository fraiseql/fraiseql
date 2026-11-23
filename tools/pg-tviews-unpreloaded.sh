#!/usr/bin/env bash
# A pg_tviews database whose library is no longer preloaded, for the `/metrics` case of
# tviews_cascade_e2e_pg that reads TVIEWS_UNPRELOADED_DATABASE_URL (#1551).
#
#   tools/pg-tviews-unpreloaded.sh up     # prints the URL to export
#   tools/pg-tviews-unpreloaded.sh down
#
# pg_tviews refuses `CREATE EXTENSION` without `shared_preload_libraries = 'pg_tviews'`,
# so the state is reached as an operator reaches it: install and create a TVIEW while the
# library is preloaded, then restart without it. The data directory is a volume carried
# from the first container to the second. The image is the one `postgres-tviews-test`
# builds, so the pg_tviews pin stays in docker/pg-tviews/Dockerfile.
#
# Everything this starts carries the label `fraiseql-pg-tviews-unpreloaded`, and `down`
# removes exactly that. PG_TVIEWS_UNPRELOADED_PORT (default 5442) is the host port.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
compose=(docker compose -f "$root/docker/docker-compose.test.yml")
label=fraiseql-pg-tviews-unpreloaded
name=fraiseql-pg-tviews-unpreloaded
port="${PG_TVIEWS_UNPRELOADED_PORT:-5442}"
env=(-e POSTGRES_DB=test_fraiseql -e POSTGRES_USER=fraiseql_test
     -e POSTGRES_PASSWORD=fraiseql_test_password)

down() {
    docker ps -aq --filter "label=$label" | xargs -r docker rm -f >/dev/null
    docker volume ls -q --filter "label=$label" | xargs -r docker volume rm >/dev/null
}

wait_ready() {
    for _ in $(seq 120); do
        if docker exec "$1" pg_isready -q -h 127.0.0.1 -U fraiseql_test -d test_fraiseql; then
            return 0
        fi
        sleep 1
    done
    echo "$1 did not become ready" >&2
    docker logs "$1" >&2
    return 1
}

up() {
    down
    "${compose[@]}" build -q postgres-tviews-test
    image="$("${compose[@]}" config --images postgres-tviews-test)"
    docker volume create --label "$label" "$name" >/dev/null

    # Preloaded, as the image's own command has it: install, and create the TVIEW.
    docker run -d --name "$name-install" --label "$label" "${env[@]}" \
        -v "$name:/var/lib/postgresql" "$image" >/dev/null
    wait_ready "$name-install"
    docker exec -i "$name-install" psql -q -v ON_ERROR_STOP=1 -U fraiseql_test \
        -d test_fraiseql <<'SQL'
SET client_min_messages = warning;
CREATE EXTENSION IF NOT EXISTS pg_tviews CASCADE;
DROP SCHEMA IF EXISTS p1551 CASCADE;
CREATE SCHEMA p1551;
CREATE TABLE p1551.tb_item (
    pk_item bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id uuid NOT NULL DEFAULT gen_random_uuid(),
    name text NOT NULL);
INSERT INTO p1551.tb_item (name) VALUES ('one');
CREATE TABLE p1551.tv_item AS
    SELECT pk_item, id, jsonb_build_object('id', id, 'name', name) AS data
    FROM p1551.tb_item;
CREATE VIEW p1551.v_item WITH (security_invoker = true) AS
    SELECT id, data FROM p1551.tv_item;
SQL
    docker stop -t 30 "$name-install" >/dev/null
    docker rm "$name-install" >/dev/null

    # The same data directory, without pg_tviews in shared_preload_libraries.
    docker run -d --name "$name" --label "$label" "${env[@]}" \
        -v "$name:/var/lib/postgresql" -p "127.0.0.1:$port:5432" "$image" \
        postgres -c shared_preload_libraries=pg_stat_statements >/dev/null
    wait_ready "$name"
    echo "postgresql://fraiseql_test:fraiseql_test_password@localhost:$port/test_fraiseql"
}

case "${1:-}" in
up) up ;;
down) down ;;
*) echo "usage: $0 up|down" >&2; exit 2 ;;
esac
