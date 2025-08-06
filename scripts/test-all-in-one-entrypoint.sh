#!/bin/bash
# ABOUTME: Entrypoint script for all-in-one test container
# ABOUTME: Starts PostgreSQL and runs tests via Unix socket

set -e

# Initialize PostgreSQL data directory if not exists
if [ ! -d "/var/lib/postgresql/16/main" ]; then
    echo "Initializing PostgreSQL..."
    sudo -u postgres /usr/lib/postgresql/16/bin/initdb -D /var/lib/postgresql/16/main
fi

# Configure PostgreSQL for local connections
echo "Configuring PostgreSQL..."
cat > /var/lib/postgresql/16/main/pg_hba.conf << EOF
local   all             all                                     trust
host    all             all             127.0.0.1/32            trust
host    all             all             ::1/128                 trust
EOF

# Create log directory
mkdir -p /var/log/postgresql
chown postgres:postgres /var/log/postgresql

# Start PostgreSQL
echo "Starting PostgreSQL..."
sudo -u postgres /usr/lib/postgresql/16/bin/pg_ctl -D /var/lib/postgresql/16/main -l /var/log/postgresql/postgresql.log start

# Wait for PostgreSQL to be ready
for i in {1..30}; do
    if sudo -u postgres /usr/lib/postgresql/16/bin/pg_isready > /dev/null 2>&1; then
        echo "PostgreSQL is ready!"
        break
    fi
    echo -n "."
    sleep 1
done

# Create test database
echo "Creating test database..."
sudo -u postgres createdb fraiseql_test 2>/dev/null || true

# Switch to test user and run tests
echo "Running tests..."
cd /home/testuser/app
sudo -u testuser bash -c '
    source .venv/bin/activate
    export TEST_DATABASE_URL="postgresql:///fraiseql_test?host=/var/run/postgresql"
    # Use pytest from virtual environment
    .venv/bin/pytest $@
' -- "$@"
