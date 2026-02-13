#!/bin/bash
set -e

# Wait for database to be ready
echo "Waiting for database..."
until pg_isready -h ${DB_HOST:-localhost} -p ${DB_PORT:-5432} -U postgres; do
  sleep 1
done
echo "Database is ready"

# Compile schema
echo "Compiling schema..."
fraiseql compile schema.py --output schema.compiled.json

# Start GraphQL server with federation configuration
echo "Starting GraphQL server..."
fraiseql server \
    --schema schema.compiled.json \
    --database-url "${DATABASE_URL}" \
    --port "${PORT:-4003}" \
    --graphql-path "${GRAPHQL_PATH:-/graphql}" \
    --federation \
    --federation-config federation.toml
