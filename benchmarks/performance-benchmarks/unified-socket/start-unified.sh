#!/bin/bash
set -e

echo "Starting unified container with PostgreSQL and application..."

# Debug information
echo "Environment:"
echo "  PGDATA: $PGDATA"
echo "  USER: $(whoami)"
echo "  PostgreSQL version: $(postgres --version 2>/dev/null || echo 'not found')"

# Initialize PostgreSQL if needed
if [ ! -s "$PGDATA/PG_VERSION" ]; then
    echo "Initializing PostgreSQL database..."
    # Use the correct PostgreSQL version path
    PGVERSION=$(ls /usr/lib/postgresql/ | head -n1)
    echo "  Using PostgreSQL version: $PGVERSION"
    su - postgres -c "/usr/lib/postgresql/$PGVERSION/bin/initdb -D $PGDATA"

    # Configure PostgreSQL for socket connections
    echo "Configuring PostgreSQL..."
    su - postgres -c "echo \"local all all trust\" > $PGDATA/pg_hba.conf"
    su - postgres -c "echo \"host all all 127.0.0.1/32 trust\" >> $PGDATA/pg_hba.conf"

    # Start PostgreSQL temporarily for setup
    su - postgres -c "/usr/lib/postgresql/$PGVERSION/bin/pg_ctl -D $PGDATA -o '-c listen_addresses=localhost' start"

    # Wait for PostgreSQL to be ready
    until su - postgres -c "psql -U postgres -c 'SELECT 1'" &> /dev/null; do
        echo "Waiting for PostgreSQL to start..."
        sleep 1
    done

    # Create database and user
    su - postgres -c "psql -U postgres -c \"CREATE USER benchmark WITH PASSWORD 'benchmark';\""
    su - postgres -c "psql -U postgres -c \"CREATE DATABASE benchmark_db OWNER benchmark;\""

    # Run initialization scripts
    echo "Running database initialization scripts..."
    su - postgres -c "PGPASSWORD=benchmark psql -U benchmark -d benchmark_db -f /docker-entrypoint-initdb.d/01-schema.sql"

    # Run FraiseQL views if they exist
    if [ -f "/docker-entrypoint-initdb.d/02-views.sql" ]; then
        su - postgres -c "PGPASSWORD=benchmark psql -U benchmark -d benchmark_db -f /docker-entrypoint-initdb.d/02-views.sql"
    fi

    # Generate and run adaptive seed data
    if [ -f "/docker-entrypoint-initdb.d/create_adaptive_seed.sh" ]; then
        echo "Generating adaptive seed data..."
        cd /docker-entrypoint-initdb.d
        ./create_adaptive_seed.sh
        su - postgres -c "PGPASSWORD=benchmark psql -U benchmark -d benchmark_db -f /tmp/seed-data-generated.sql"
    fi

    # Stop PostgreSQL
    su - postgres -c "/usr/lib/postgresql/$PGVERSION/bin/pg_ctl -D $PGDATA stop"
fi

# Configure PostgreSQL for optimal performance with Unix sockets
cat > $PGDATA/postgresql.conf << EOF
# Basic settings
listen_addresses = ''  # Only Unix socket, no TCP/IP
unix_socket_directories = '/var/run/postgresql'
max_connections = 200

# Memory settings (adjust based on container limits)
shared_buffers = 256MB
effective_cache_size = 1GB
maintenance_work_mem = 64MB
work_mem = 4MB

# Write performance
checkpoint_completion_target = 0.9
wal_buffers = 16MB
default_statistics_target = 100
random_page_cost = 1.1  # SSD optimized

# Query performance
effective_io_concurrency = 200
max_parallel_workers_per_gather = 2
max_parallel_workers = 4

# Logging (minimal for benchmarks)
log_destination = 'stderr'
logging_collector = off
log_min_messages = error
log_min_error_statement = error
EOF

echo "Starting services with supervisor..."
exec /usr/bin/supervisord -n -c /etc/supervisor/conf.d/supervisord.conf
