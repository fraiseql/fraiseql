#!/bin/sh
# FraiseQL Docker Entrypoint Script
# Handles database migrations and startup

set -e

echo "Starting FraiseQL..."

# Wait for database if DATABASE_URL is set
if [ -n "$DATABASE_URL" ]; then
    echo "Waiting for database..."

    # Extract host and port from DATABASE_URL
    # Format: postgresql://user:pass@host:port/dbname
    DB_HOST=$(echo $DATABASE_URL | sed -E 's/.*@([^:]+):([0-9]+).*/\1/')
    DB_PORT=$(echo $DATABASE_URL | sed -E 's/.*@([^:]+):([0-9]+).*/\2/')

    # Wait for database to be ready
    until python3 -c "import socket; s = socket.socket(); s.settimeout(1); s.connect(('$DB_HOST', int('$DB_PORT'))); s.close()" 2>/dev/null; do
        echo "Database is unavailable - sleeping"
        sleep 1
    done

    echo "Database is up - continuing"
fi

# Run database migrations if enabled
if [ "$RUN_MIGRATIONS" = "true" ]; then
    echo "Running database migrations..."
    # Add migration command when available
    # fraiseql migrate
fi

# Execute the main command
exec "$@"
