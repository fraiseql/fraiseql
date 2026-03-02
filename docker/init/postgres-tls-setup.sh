#!/usr/bin/env bash
# postgres-tls-setup.sh — runs inside postgres:16 container during initdb
#
# Called by docker-entrypoint.sh as part of the init scripts in
# /docker-entrypoint-initdb.d/. Runs as the postgres user.
#
# Generates a self-signed TLS certificate + key in $PGDATA so PostgreSQL
# can start with `ssl=on`. The server.crt is copied to /var/run/postgresql
# so callers can extract it via `docker cp` and use it as the CA cert.
#
# Usage in docker-compose.test.yml:
#   image: postgres:16
#   volumes:
#     - ./init/postgres-tls-setup.sh:/docker-entrypoint-initdb.d/00-tls.sh:ro
#   command: ["-c", "ssl=on"]
#
# After the container is running, extract the CA cert:
#   docker cp postgres-tls:/var/run/postgresql/ca.crt /tmp/tls-ca.crt

set -euo pipefail

echo "[TLS setup] Generating self-signed certificate in $PGDATA..."

openssl req -x509 -newkey rsa:2048 \
    -keyout "$PGDATA/server.key" \
    -out   "$PGDATA/server.crt" \
    -days  365 \
    -nodes \
    -subj  "/CN=localhost" \
    2>/dev/null

# PostgreSQL requires the key to be readable only by the server process owner
chmod 600 "$PGDATA/server.key"

echo "[TLS setup] Certificate generated."
echo "[TLS setup]   cert: $PGDATA/server.crt"
echo "[TLS setup]   key:  $PGDATA/server.key"

# Copy the CA cert (self-signed, so server cert = CA cert) to a well-known
# location so the CI step can docker-cp it out without knowing $PGDATA.
mkdir -p /var/run/postgresql
cp "$PGDATA/server.crt" /var/run/postgresql/ca.crt
echo "[TLS setup] CA cert available at /var/run/postgresql/ca.crt"

# Create the wire test view — fraiseql-wire TLS integration tests query v_test_entity
# and expect at least 10 rows.
echo "[TLS setup] Creating v_test_entity view for wire TLS tests..."

psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<-EOSQL
    CREATE TABLE IF NOT EXISTS test_entities (
        id   SERIAL PRIMARY KEY,
        name TEXT        NOT NULL,
        data JSONB       NOT NULL DEFAULT '{}'
    );

    INSERT INTO test_entities (name, data)
    SELECT
        'entity_' || i,
        jsonb_build_object('index', i, 'tag', md5(i::text))
    FROM generate_series(1, 20) AS i;

    CREATE OR REPLACE VIEW v_test_entity AS
        SELECT id, name, data FROM test_entities;
EOSQL

echo "[TLS setup] v_test_entity created with 20 rows."
