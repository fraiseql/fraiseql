#!/bin/bash
# Turn on TLS for the local TLS-test Postgres.
#
# Runs as the postgres user during initdb. The certificate/key arrive read-only via
# a bind mount, so they are copied into $PGDATA where the server can own them and
# the key can be chmod 600 (PostgreSQL refuses to start with a group/world-readable
# key). Mirrors `tlsPgService()` in .dagger/main.go.
set -e

cp /tls-certs/server.crt "$PGDATA/server.crt"
cp /tls-certs/server.key "$PGDATA/server.key"
chmod 600 "$PGDATA/server.key"

{
    echo "ssl = on"
    echo "ssl_cert_file = 'server.crt'"
    echo "ssl_key_file = 'server.key'"
} >>"$PGDATA/postgresql.conf"

# The wire TLS suite queries v_test_entity and expects at least 10 rows (#997).
# `tlsPgService()` seeds it, so the CI leg passed while every local run failed with
# `relation "v_test_entity" does not exist`. Keep this in step with the Dagger
# heredoc — "mirrors tlsPgService()" is the whole contract of this file.
psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<-'EOSQL'
    CREATE TABLE IF NOT EXISTS test_entities (
        id   SERIAL PRIMARY KEY,
        name TEXT  NOT NULL,
        data JSONB NOT NULL DEFAULT '{}'
    );
    INSERT INTO test_entities (name, data)
    SELECT 'entity_' || i, jsonb_build_object('index', i, 'tag', md5(i::text))
    FROM generate_series(1, 20) AS i;
    CREATE OR REPLACE VIEW v_test_entity AS SELECT id, name, data FROM test_entities;
EOSQL
