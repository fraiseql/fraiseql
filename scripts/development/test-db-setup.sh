#!/bin/bash
# ABOUTME: Script to set up a PostgreSQL database container for testing with podman
# ABOUTME: Creates a containerized test database for FraiseQL test suite

set -e

# Container and database configuration
CONTAINER_NAME="fraiseql-test-db"
DB_NAME="fraiseql_test"
DB_USER="fraiseql_test"
DB_PASSWORD="fraiseql_test"
DB_PORT="5435"  # Different from default to avoid conflicts

echo "Setting up PostgreSQL test database with podman..."

# Stop and remove existing container if it exists
if podman ps -a | grep -q "$CONTAINER_NAME"; then
    echo "Removing existing container..."
    podman stop "$CONTAINER_NAME" 2>/dev/null || true
    podman rm "$CONTAINER_NAME" 2>/dev/null || true
fi

# Create and start new PostgreSQL container
echo "Creating new PostgreSQL container..."
podman run -d \
    --name "$CONTAINER_NAME" \
    -e POSTGRES_DB="$DB_NAME" \
    -e POSTGRES_USER="$DB_USER" \
    -e POSTGRES_PASSWORD="$DB_PASSWORD" \
    -p "$DB_PORT:5432" \
    postgres:16-alpine

# Wait for database to be ready
echo "Waiting for database to be ready..."
for i in {1..30}; do
    if podman exec "$CONTAINER_NAME" pg_isready -U "$DB_USER" > /dev/null 2>&1; then
        echo "Database is ready!"
        break
    fi
    echo -n "."
    sleep 1
done

if ! podman exec "$CONTAINER_NAME" pg_isready -U "$DB_USER" > /dev/null 2>&1; then
    echo "Database failed to start within 30 seconds"
    exit 1
fi

# Export connection string for tests
export TEST_DATABASE_URL="postgresql://$DB_USER:$DB_PASSWORD@localhost:$DB_PORT/$DB_NAME"
echo ""
echo "Test database is ready!"
echo "Connection string: $TEST_DATABASE_URL"
echo ""
echo "To stop the database: podman stop $CONTAINER_NAME"
echo "To remove the database: podman rm $CONTAINER_NAME"
