#!/bin/bash
# A pg_tviews database whose library is no longer preloaded, for the `/metrics` case of
# tviews_cascade_e2e_pg that reads TVIEWS_UNPRELOADED_DATABASE_URL.
#
# Owned once and used by BOTH rigs: `tools/pg-tviews-unpreloaded.sh` locally (and so
# `make test-pg-tviews`) and `pgTviewsUnpreloadedService` in `.dagger/main.go`. Runs in
# the image docker/pg-tviews builds, as `postgres`, in place of the image's entrypoint.
#
# pg_tviews refuses `CREATE EXTENSION` without `shared_preload_libraries = 'pg_tviews'`,
# so the state is reached as an operator reaches it: initialise, start with the library
# preloaded, install it and create a TVIEW, stop, and start again without it. The first
# server listens on the socket only, so the port opens (and a rig's readiness check
# passes) only once the second one is up.
set -euo pipefail

: "${POSTGRES_USER:?}" "${POSTGRES_PASSWORD:?}" "${POSTGRES_DB:?}"
PGDATA="${PGDATA:?}"
export PGHOST=/var/run/postgresql

initdb -D "$PGDATA" -U "$POSTGRES_USER" --auth-local=trust --auth-host=scram-sha-256 \
    --pwfile=<(printf '%s\n' "$POSTGRES_PASSWORD") >/dev/null
echo "host all all all scram-sha-256" >>"$PGDATA/pg_hba.conf"

pg_ctl -D "$PGDATA" -w -l /tmp/install.log start \
    -o "-c listen_addresses='' -c shared_preload_libraries=pg_tviews"
psql -q -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d postgres \
    -c "CREATE DATABASE \"$POSTGRES_DB\""
psql -q -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d "$POSTGRES_DB" <<'SQL'
SET client_min_messages = warning;
CREATE EXTENSION pg_tviews CASCADE;
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
pg_ctl -D "$PGDATA" -w stop >/dev/null

# The same data directory, without pg_tviews in shared_preload_libraries.
exec postgres -D "$PGDATA" -c listen_addresses='*' \
    -c shared_preload_libraries=pg_stat_statements
