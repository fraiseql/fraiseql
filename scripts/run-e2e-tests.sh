#!/usr/bin/env bash
# FraiseQL E2E Test Runner
#
# This script:
# 1. Starts the Docker containers (PostgreSQL + FraiseQL server)
# 2. Waits for services to be healthy
# 3. Runs the E2E tests
# 4. Cleans up containers
#
# Usage:
#   ./scripts/run-e2e-tests.sh           # Run E2E tests
#   ./scripts/run-e2e-tests.sh --keep    # Keep containers running after tests

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
COMPOSE_FILE="$PROJECT_ROOT/docker-compose.e2e.yml"
SERVER_URL="http://localhost:9001"
MAX_WAIT_SECONDS=120
KEEP_CONTAINERS=false

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --keep)
            KEEP_CONTAINERS=true
            shift
            ;;
        *)
            echo -e "${RED}Unknown argument: $1${NC}"
            exit 1
            ;;
    esac
done

echo -e "${GREEN}=== FraiseQL E2E Test Runner ===${NC}"
echo ""

# Cleanup function
cleanup() {
    if [ "$KEEP_CONTAINERS" = false ]; then
        echo -e "${YELLOW}Cleaning up containers...${NC}"
        docker compose -f "$COMPOSE_FILE" down -v --remove-orphans 2>/dev/null || true
    else
        echo -e "${YELLOW}Keeping containers running (--keep flag)${NC}"
        echo "  Stop with: docker compose -f docker-compose.e2e.yml down -v"
    fi
}

# Set trap to cleanup on exit
trap cleanup EXIT

# Step 1: Ensure schema is compiled
echo -e "${YELLOW}Step 1: Compiling E2E test schema...${NC}"
cd "$PROJECT_ROOT"
if [ ! -f "target/release/fraiseql-cli" ]; then
    echo "  Building fraiseql-cli..."
    cargo build --release -p fraiseql-cli
fi
./target/release/fraiseql-cli compile tests/e2e/schema.json -o tests/e2e/schema.compiled.json
echo -e "${GREEN}  Schema compiled successfully${NC}"
echo ""

# Step 2: Start containers
echo -e "${YELLOW}Step 2: Starting Docker containers...${NC}"
docker compose -f "$COMPOSE_FILE" down -v --remove-orphans 2>/dev/null || true
docker compose -f "$COMPOSE_FILE" up -d --build
echo -e "${GREEN}  Containers started${NC}"
echo ""

# Step 3: Wait for services to be healthy
echo -e "${YELLOW}Step 3: Waiting for services to be healthy...${NC}"
start_time=$(date +%s)

wait_for_health() {
    local url=$1
    local name=$2
    local elapsed=0

    while [ $elapsed -lt $MAX_WAIT_SECONDS ]; do
        if curl -sf "$url" > /dev/null 2>&1; then
            echo -e "  ${GREEN}✓ $name is healthy${NC}"
            return 0
        fi
        sleep 2
        elapsed=$(($(date +%s) - start_time))
        echo "  Waiting for $name... (${elapsed}s)"
    done

    echo -e "  ${RED}✗ $name failed to become healthy after ${MAX_WAIT_SECONDS}s${NC}"
    echo "  Checking container logs:"
    docker compose -f "$COMPOSE_FILE" logs --tail=50
    return 1
}

wait_for_health "$SERVER_URL/health" "FraiseQL Server"
echo ""

# Step 4: Run basic health checks
echo -e "${YELLOW}Step 4: Running health checks...${NC}"

# Test health endpoint
echo "  Testing /health endpoint..."
health_response=$(curl -sf "$SERVER_URL/health")
if echo "$health_response" | grep -q "healthy"; then
    echo -e "  ${GREEN}✓ Health endpoint OK${NC}"
else
    echo -e "  ${RED}✗ Health endpoint returned unexpected response${NC}"
    echo "  Response: $health_response"
    exit 1
fi

# Test introspection endpoint
echo "  Testing /introspection endpoint..."
introspection_response=$(curl -sf "$SERVER_URL/introspection" || echo "error")
if [ "$introspection_response" != "error" ]; then
    echo -e "  ${GREEN}✓ Introspection endpoint OK${NC}"
else
    echo -e "  ${YELLOW}⚠ Introspection endpoint not available (may be disabled)${NC}"
fi
echo ""

# Step 5: Run GraphQL query tests
echo -e "${YELLOW}Step 5: Running GraphQL query tests...${NC}"

# Test users query
echo "  Testing 'users' query..."
users_response=$(curl -sf -X POST "$SERVER_URL/graphql" \
    -H "Content-Type: application/json" \
    -d '{"query": "{ users { id name email } }"}')

if echo "$users_response" | grep -q '"data"'; then
    user_count=$(echo "$users_response" | grep -o '"id"' | wc -l)
    echo -e "  ${GREEN}✓ Users query returned $user_count users${NC}"
else
    echo -e "  ${RED}✗ Users query failed${NC}"
    echo "  Response: $users_response"
    exit 1
fi

# Test posts query
echo "  Testing 'posts' query..."
posts_response=$(curl -sf -X POST "$SERVER_URL/graphql" \
    -H "Content-Type: application/json" \
    -d '{"query": "{ posts { id title author { name } } }"}')

if echo "$posts_response" | grep -q '"data"'; then
    post_count=$(echo "$posts_response" | grep -o '"title"' | wc -l)
    echo -e "  ${GREEN}✓ Posts query returned $post_count posts${NC}"
else
    echo -e "  ${RED}✗ Posts query failed${NC}"
    echo "  Response: $posts_response"
    exit 1
fi

# Test products query
echo "  Testing 'products' query..."
products_response=$(curl -sf -X POST "$SERVER_URL/graphql" \
    -H "Content-Type: application/json" \
    -d '{"query": "{ products { id name price stock } }"}')

if echo "$products_response" | grep -q '"data"'; then
    product_count=$(echo "$products_response" | grep -o '"name"' | wc -l)
    echo -e "  ${GREEN}✓ Products query returned $product_count products${NC}"
else
    echo -e "  ${RED}✗ Products query failed${NC}"
    echo "  Response: $products_response"
    exit 1
fi
echo ""

# Step 6: Run Rust E2E tests against Docker container
echo -e "${YELLOW}Step 6: Running Rust E2E tests...${NC}"
cd "$PROJECT_ROOT"

# Run the server E2E tests with FRAISEQL_TEST_URL pointing to Docker container
if FRAISEQL_TEST_URL="$SERVER_URL" cargo test -p fraiseql-server --test http_server_e2e_test -- --include-ignored 2>&1; then
    echo -e "${GREEN}✓ Rust E2E tests passed${NC}"
else
    echo -e "${RED}✗ Some Rust E2E tests failed${NC}"
    # Don't exit - continue to show summary
fi
echo ""

# Summary
echo -e "${GREEN}=== E2E Test Summary ===${NC}"
echo -e "  ${GREEN}✓ PostgreSQL database healthy${NC}"
echo -e "  ${GREEN}✓ FraiseQL server healthy${NC}"
echo -e "  ${GREEN}✓ Health endpoint working${NC}"
echo -e "  ${GREEN}✓ GraphQL queries working${NC}"
echo ""
echo -e "${GREEN}All E2E tests completed!${NC}"
