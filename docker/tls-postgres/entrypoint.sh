#!/bin/sh
# Custom entrypoint to fix certificate permissions for PostgreSQL

set -e

# Copy certificates to a location PostgreSQL can access with correct ownership
mkdir -p /var/lib/postgresql/certs
cp /certs/server.crt /var/lib/postgresql/certs/server.crt
cp /certs/server.key /var/lib/postgresql/certs/server.key
cp /certs/ca.crt /var/lib/postgresql/certs/root.crt

# Set correct ownership (postgres user is UID 70 in alpine)
chown postgres:postgres /var/lib/postgresql/certs/*
chmod 600 /var/lib/postgresql/certs/server.key
chmod 644 /var/lib/postgresql/certs/server.crt /var/lib/postgresql/certs/root.crt

# Run the original entrypoint with SSL configuration
exec docker-entrypoint.sh postgres \
    -c ssl=on \
    -c ssl_cert_file=/var/lib/postgresql/certs/server.crt \
    -c ssl_key_file=/var/lib/postgresql/certs/server.key \
    -c ssl_ca_file=/var/lib/postgresql/certs/root.crt
