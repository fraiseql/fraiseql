#!/usr/bin/env bash
#
# Setup PostgreSQL with TLS for testing
#
# Usage:
#   ./scripts/setup-tls-postgres.sh
#
# This script:
# 1. Generates self-signed certificates
# 2. Configures PostgreSQL to use TLS
# 3. Creates a test database
# 4. Outputs environment variables for running tests
#
# Requirements:
# - PostgreSQL installed and running
# - openssl
# - sudo access (for PostgreSQL config)

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

info() { echo -e "${GREEN}[INFO]${NC} $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1"; exit 1; }

# Configuration
CERT_DIR="${CERT_DIR:-/tmp/fraiseql-tls}"
PG_DATA_DIR="${PG_DATA_DIR:-/var/lib/postgres/data}"
DB_NAME="fraiseql_tls_test"
DB_USER="${USER}"
DB_PORT="${DB_PORT:-5432}"

info "Setting up PostgreSQL with TLS for FraiseQL testing..."

# Step 1: Generate certificates
info "Step 1: Generating self-signed certificates..."

mkdir -p "$CERT_DIR"
cd "$CERT_DIR"

# Generate CA key and certificate
if [ ! -f ca.key ]; then
    openssl genrsa -out ca.key 4096
    openssl req -new -x509 -days 365 -key ca.key -out ca.crt \
        -subj "/CN=FraiseQL Test CA/O=FraiseQL/C=US"
    info "Generated CA certificate"
else
    info "CA certificate already exists, skipping..."
fi

# Generate server key and CSR
if [ ! -f server.key ]; then
    openssl genrsa -out server.key 2048
    openssl req -new -key server.key -out server.csr \
        -subj "/CN=localhost/O=FraiseQL/C=US"

    # Create extensions file for SAN
    cat > server.ext << EOF
authorityKeyIdentifier=keyid,issuer
basicConstraints=CA:FALSE
keyUsage = digitalSignature, keyEncipherment
extendedKeyUsage = serverAuth
subjectAltName = @alt_names

[alt_names]
DNS.1 = localhost
IP.1 = 127.0.0.1
IP.2 = ::1
EOF

    # Sign server certificate with CA
    openssl x509 -req -in server.csr -CA ca.crt -CAkey ca.key -CAcreateserial \
        -out server.crt -days 365 -extfile server.ext

    rm server.csr server.ext
    info "Generated server certificate"
else
    info "Server certificate already exists, skipping..."
fi

# Set correct permissions
chmod 600 server.key
chmod 644 server.crt ca.crt

info "Certificates generated in $CERT_DIR"

# Step 2: Configure PostgreSQL
info "Step 2: Configuring PostgreSQL for TLS..."

# Check if running as root or with sudo
if [ "$EUID" -ne 0 ]; then
    warn "Not running as root. Will attempt to use sudo for PostgreSQL configuration."
    SUDO="sudo"
else
    SUDO=""
fi

# Copy certificates to PostgreSQL data directory
$SUDO cp "$CERT_DIR/server.crt" "$PG_DATA_DIR/server.crt"
$SUDO cp "$CERT_DIR/server.key" "$PG_DATA_DIR/server.key"
$SUDO cp "$CERT_DIR/ca.crt" "$PG_DATA_DIR/root.crt"

# Set ownership and permissions
$SUDO chown postgres:postgres "$PG_DATA_DIR/server.crt" "$PG_DATA_DIR/server.key" "$PG_DATA_DIR/root.crt"
$SUDO chmod 600 "$PG_DATA_DIR/server.key"
$SUDO chmod 644 "$PG_DATA_DIR/server.crt" "$PG_DATA_DIR/root.crt"

# Check if ssl is already enabled
if $SUDO grep -q "^ssl = on" "$PG_DATA_DIR/postgresql.conf" 2>/dev/null; then
    info "SSL already enabled in postgresql.conf"
else
    info "Enabling SSL in postgresql.conf..."

    # Backup original config
    $SUDO cp "$PG_DATA_DIR/postgresql.conf" "$PG_DATA_DIR/postgresql.conf.backup.$(date +%Y%m%d%H%M%S)"

    # Add SSL configuration
    $SUDO tee -a "$PG_DATA_DIR/postgresql.conf" > /dev/null << EOF

# FraiseQL TLS Testing Configuration
ssl = on
ssl_cert_file = 'server.crt'
ssl_key_file = 'server.key'
ssl_ca_file = 'root.crt'
EOF

    info "SSL configuration added to postgresql.conf"
fi

# Step 3: Create test database
info "Step 3: Creating test database..."

# Check if database exists
if psql -lqt | cut -d \| -f 1 | grep -qw "$DB_NAME"; then
    info "Database $DB_NAME already exists"
else
    createdb "$DB_NAME" || warn "Could not create database (may already exist)"
    info "Created database $DB_NAME"
fi

# Create a test view that fraiseql-wire can query
psql "$DB_NAME" << 'EOSQL'
-- Create a simple test table and view for TLS testing
CREATE TABLE IF NOT EXISTS test_entity (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    data JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Insert test data if empty
INSERT INTO test_entity (data)
SELECT jsonb_build_object(
    'name', 'Test Entity ' || i,
    'value', i * 10,
    'active', i % 2 = 0
)
FROM generate_series(1, 100) AS i
WHERE NOT EXISTS (SELECT 1 FROM test_entity LIMIT 1);

-- Create the view that fraiseql-wire expects
CREATE OR REPLACE VIEW v_test_entity AS
SELECT data FROM test_entity;

-- Create pg_tables-like view for compatibility
CREATE OR REPLACE VIEW pg_tables AS
SELECT jsonb_build_object(
    'schemaname', schemaname,
    'tablename', tablename,
    'tableowner', tableowner
) AS data
FROM pg_catalog.pg_tables
WHERE schemaname NOT IN ('pg_catalog', 'information_schema');

-- Create pg_version view
CREATE OR REPLACE VIEW pg_version AS
SELECT jsonb_build_object('version', version()) AS data;
EOSQL

info "Test database configured"

# Step 4: Restart PostgreSQL
info "Step 4: Restarting PostgreSQL to apply TLS configuration..."

$SUDO systemctl restart postgresql || {
    warn "Could not restart PostgreSQL via systemctl. Try manually:"
    echo "  sudo systemctl restart postgresql"
    echo "  # or"
    echo "  sudo pg_ctl -D $PG_DATA_DIR restart"
}

# Wait for PostgreSQL to be ready
sleep 2

# Verify TLS is working
info "Verifying TLS configuration..."
if psql "sslmode=require dbname=$DB_NAME" -c "SELECT 1" > /dev/null 2>&1; then
    info "TLS connection successful!"
else
    warn "TLS connection test failed. Check PostgreSQL logs."
fi

# Output environment variables
echo ""
echo "============================================"
echo "TLS PostgreSQL Setup Complete!"
echo "============================================"
echo ""
echo "To run TLS tests, set these environment variables:"
echo ""
echo "  export TLS_TEST_DB_URL=\"postgres://localhost:$DB_PORT/$DB_NAME?sslmode=require\""
echo "  export TLS_TEST_INSECURE=\"true\"  # For self-signed cert"
echo ""
echo "Then run the tests:"
echo ""
echo "  cargo test -p fraiseql-wire --test tls_integration -- --ignored --nocapture"
echo ""
echo "Certificate locations:"
echo "  CA cert:     $CERT_DIR/ca.crt"
echo "  Server cert: $CERT_DIR/server.crt"
echo "  Server key:  $CERT_DIR/server.key"
echo ""
