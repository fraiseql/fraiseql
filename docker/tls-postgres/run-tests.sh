#!/usr/bin/env bash
#
# Start TLS PostgreSQL container and run TLS integration tests
#
# Usage: ./run-tests.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

cd "$SCRIPT_DIR"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

info() { echo -e "${GREEN}[INFO]${NC} $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1"; exit 1; }

# Step 1: Generate certificates if needed
if [ ! -f certs/server.crt ]; then
    info "Generating TLS certificates..."
    bash generate-certs.sh
else
    info "Certificates already exist, skipping generation"
fi

# Step 2: Start container
info "Starting TLS PostgreSQL container..."
docker compose down -v 2>/dev/null || true
docker compose up -d

# Step 3: Wait for PostgreSQL to be ready
info "Waiting for PostgreSQL to be ready..."
for i in {1..30}; do
    if docker compose exec -T postgres-tls pg_isready -U fraiseql -d fraiseql_tls_test >/dev/null 2>&1; then
        info "PostgreSQL is ready!"
        break
    fi
    if [ $i -eq 30 ]; then
        error "PostgreSQL failed to start within 30 seconds"
    fi
    sleep 1
done

# Step 4: Verify TLS is working
info "Verifying TLS connection..."
if docker compose exec -T postgres-tls psql -U fraiseql -d fraiseql_tls_test -c "SHOW ssl;" | grep -q "on"; then
    info "SSL is enabled in PostgreSQL container"
else
    warn "SSL may not be enabled correctly"
fi

# Step 5: Run tests
info "Running TLS integration tests..."
cd "$PROJECT_ROOT"

export TLS_TEST_DB_URL="postgres://fraiseql:fraiseql_test@localhost:5433/fraiseql_tls_test"
# Use our self-signed CA for proper certificate validation (not insecure mode)
export TLS_TEST_CA_CERT="$SCRIPT_DIR/certs/ca.crt"

cargo test -p fraiseql-wire --test tls_integration -- --ignored --nocapture

# Step 6: Cleanup (optional - comment out to keep container running)
# info "Stopping container..."
# cd "$SCRIPT_DIR"
# docker compose down -v

info "Tests complete!"
echo ""
echo "Container is still running. To stop it:"
echo "  cd $SCRIPT_DIR && docker compose down -v"
echo ""
echo "To connect manually:"
echo "  psql 'postgres://fraiseql:fraiseql_test@localhost:5433/fraiseql_tls_test?sslmode=require'"
