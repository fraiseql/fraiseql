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
