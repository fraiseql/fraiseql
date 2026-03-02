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

echo "[TLS setup] Generating CA + server certificate chain in $PGDATA..."

# Step 1: Generate CA key and self-signed CA certificate (CA:TRUE so rustls accepts it)
openssl req -x509 -newkey rsa:2048 \
    -keyout "$PGDATA/ca.key" \
    -out    "$PGDATA/ca.crt" \
    -days   365 \
    -nodes \
    -subj   "/CN=fraiseql-test-ca" \
    -addext "basicConstraints=critical,CA:TRUE" \
    -addext "keyUsage=critical,keyCertSign,cRLSign" \
    2>/dev/null

# Step 2: Generate server key and CSR
openssl req -newkey rsa:2048 \
    -keyout "$PGDATA/server.key" \
    -out    "$PGDATA/server.csr" \
    -days   365 \
    -nodes \
    -subj   "/CN=localhost" \
    2>/dev/null

# Step 3: Sign the server cert with the CA — end-entity cert (no CA:TRUE)
openssl x509 -req \
    -in     "$PGDATA/server.csr" \
    -CA     "$PGDATA/ca.crt" \
    -CAkey  "$PGDATA/ca.key" \
    -CAcreateserial \
    -out    "$PGDATA/server.crt" \
    -days   365 \
    -extfile <(printf "subjectAltName=IP:127.0.0.1,DNS:localhost\nbasicConstraints=CA:FALSE") \
    2>/dev/null

# PostgreSQL requires the key to be readable only by the server process owner
chmod 600 "$PGDATA/server.key"

echo "[TLS setup] Certificates generated."
echo "[TLS setup]   CA cert:     $PGDATA/ca.crt"
echo "[TLS setup]   Server cert: $PGDATA/server.crt"
echo "[TLS setup]   Server key:  $PGDATA/server.key"

# Enable SSL in postgresql.conf here, AFTER certs exist.
# We do NOT pass -c ssl=on as a docker run arg because the postgres init-time
# temp server also starts with that flag — before this script has run — causing
# it to abort with "server.crt: No such file or directory".
{
    echo ""
    echo "# TLS — enabled by postgres-tls-setup.sh init script"
    echo "ssl = on"
    echo "ssl_cert_file = 'server.crt'"
    echo "ssl_key_file  = 'server.key'"
} >> "$PGDATA/postgresql.conf"
echo "[TLS setup] SSL enabled in postgresql.conf."

# Copy the CA cert to a well-known location so CI can docker-cp it out.
mkdir -p /var/run/postgresql
cp "$PGDATA/ca.crt" /var/run/postgresql/ca.crt
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
