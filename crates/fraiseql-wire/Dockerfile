FROM alpine:latest

# Install PostgreSQL and dependencies
RUN apk add --no-cache \
    postgresql \
    postgresql-client \
    postgresql-contrib \
    bash \
    ca-certificates \
    curl

# Create postgres user and directories
RUN mkdir -p /run/postgresql /var/lib/postgresql/data && \
    chown -R postgres:postgres /run/postgresql /var/lib/postgresql

# Initialize PostgreSQL database
RUN su - postgres -c "initdb -D /var/lib/postgresql/data"

# Create a custom PostgreSQL config to listen on all interfaces
RUN echo "listen_addresses = '*'" >> /var/lib/postgresql/data/postgresql.conf && \
    echo "host    all             all             0.0.0.0/0               md5" >> /var/lib/postgresql/data/pg_hba.conf

# Set up default user and database
RUN su - postgres -c "pg_ctl -D /var/lib/postgresql/data start && \
    psql -U postgres -c \"ALTER USER postgres WITH PASSWORD 'postgres';\" && \
    createdb -U postgres fraiseql_test && \
    pg_ctl -D /var/lib/postgresql/data stop"

# Create startup script
RUN cat > /start.sh << 'EOF'
#!/bin/bash
set -e

# Start PostgreSQL
su - postgres -c "pg_ctl -D /var/lib/postgresql/data start"

# Wait for PostgreSQL to be ready
echo "Waiting for PostgreSQL to be ready..."
for i in {1..30}; do
    if pg_isready -U postgres -h localhost 2>/dev/null; then
        echo "PostgreSQL is ready!"
        break
    fi
    echo "Attempt $i/30: waiting..."
    sleep 1
done

# Keep container running
tail -f /dev/null
EOF

RUN chmod +x /start.sh

EXPOSE 5432

CMD ["/start.sh"]
