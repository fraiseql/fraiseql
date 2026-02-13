#!/bin/bash
# FraiseQL Design Quality Pre-Commit Hook
#
# This hook runs the design quality audit before allowing commits to the schema.
# Install with: cp examples/pre-commit-hooks.sh .git/hooks/pre-commit && chmod +x .git/hooks/pre-commit
#
# Override with: git commit --no-verify

set -e

# Configuration
SCHEMA_FILE="${SCHEMA_FILE:-schema.compiled.json}"
API_ENDPOINT="${FRAISEQL_API_ENDPOINT:-http://localhost:8080}"
THRESHOLD="${DESIGN_QUALITY_THRESHOLD:-70}"
PYTHON_CMD="${PYTHON_CMD:-python3}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Helper functions
log_info() {
    echo -e "${BLUE}ℹ️  $1${NC}"
}

log_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

log_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

log_error() {
    echo -e "${RED}❌ $1${NC}"
}

# Check if schema file changed
if ! git diff --cached --name-only | grep -q "$SCHEMA_FILE"; then
    log_info "Schema file not modified, skipping design quality check"
    exit 0
fi

log_info "FraiseQL Design Quality Pre-Commit Hook"
echo "─────────────────────────────────────────"

# Check if server is running
log_info "Checking fraiseql-server at $API_ENDPOINT..."
if ! curl -f -s "$API_ENDPOINT/health" > /dev/null 2>&1; then
    log_error "fraiseql-server not running at $API_ENDPOINT"
    echo ""
    echo "To start the server, run:"
    echo "  fraiseql-server"
    echo ""
    echo "Or using Docker:"
    echo "  docker run -p 8080:8080 fraiseql/fraiseql-server"
    echo ""
    echo "To skip this check, run:"
    echo "  git commit --no-verify"
    exit 1
fi
log_success "Server is running"

# Check if Python is available
if ! command -v "$PYTHON_CMD" &> /dev/null; then
    log_error "$PYTHON_CMD not found"
    exit 1
fi
log_success "Python environment ready"

# Run design audit
log_info "Running design quality audit..."
echo ""

if ! $PYTHON_CMD examples/agents/python/schema_auditor.py \
    "$SCHEMA_FILE" \
    --api-endpoint "$API_ENDPOINT" \
    --fail-if-below "$THRESHOLD"; then

    echo ""
    log_error "Design quality check failed (score below $THRESHOLD)"
    echo ""
    echo "Options:"
    echo "  1. Fix the issues shown above"
    echo "  2. Review the audit with: python examples/agents/python/schema_auditor.py $SCHEMA_FILE --api-endpoint $API_ENDPOINT"
    echo "  3. Skip this check with: git commit --no-verify"
    exit 1
fi

log_success "Design quality check passed"
echo "─────────────────────────────────────────"
exit 0
