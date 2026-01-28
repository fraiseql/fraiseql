#!/bin/bash

# ============================================================================
# FraiseQL 3-Subgraph Federation Test Runner
# ============================================================================
#
# This script:
# 1. Starts Docker Compose with 3 subgraphs
# 2. Waits for all services to be healthy
# 3. Runs all 3-subgraph federation tests
# 4. Reports results and cleans up
#
# Usage:
#   ./run_3subgraph_tests.sh              # Run all 3-subgraph tests
#   ./run_3subgraph_tests.sh --no-cleanup # Keep containers running
#   ./run_3subgraph_tests.sh --logs       # Show logs during tests
#

set -e

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
INTEGRATION_DIR="$SCRIPT_DIR"
COMPOSE_FILE="$INTEGRATION_DIR/docker-compose.yml"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Options
CLEANUP=true
SHOW_LOGS=false
FILTER_PATTERN="test_three_subgraph_"

# Parse arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --no-cleanup)
      CLEANUP=false
      shift
      ;;
    --logs)
      SHOW_LOGS=true
      shift
      ;;
    --filter)
      FILTER_PATTERN="$2"
      shift 2
      ;;
    *)
      echo "Unknown option: $1"
      echo "Usage: $0 [--no-cleanup] [--logs] [--filter PATTERN]"
      exit 1
      ;;
  esac
done

# ============================================================================
# Utility Functions
# ============================================================================

print_header() {
  echo -e "\n${BLUE}============================================================================${NC}"
  echo -e "${BLUE}$1${NC}"
  echo -e "${BLUE}============================================================================${NC}\n"
}

print_success() {
  echo -e "${GREEN}✓ $1${NC}"
}

print_error() {
  echo -e "${RED}✗ $1${NC}"
}

print_info() {
  echo -e "${YELLOW}ℹ $1${NC}"
}

wait_for_service() {
  local url=$1
  local max_retries=${2:-30}
  local retry=0

  echo -n "  Waiting for $url..."

  while [ $retry -lt $max_retries ]; do
    if curl -s -f "$url" \
      -H "Content-Type: application/json" \
      -d '{"query":"{ __typename }"}' \
      > /dev/null 2>&1; then
      echo -e " ${GREEN}ready${NC}"
      return 0
    fi

    echo -n "."
    sleep 1
    retry=$((retry + 1))
  done

  echo -e " ${RED}failed${NC}"
  return 1
}

# ============================================================================
# Main Execution
# ============================================================================

print_header "FraiseQL 3-Subgraph Federation Test Runner"

# Check prerequisites
print_info "Checking prerequisites..."

if ! command -v docker-compose &> /dev/null; then
  print_error "docker-compose not found. Please install Docker Compose."
  exit 1
fi
print_success "docker-compose found"

if ! command -v cargo &> /dev/null; then
  print_error "cargo not found. Please install Rust."
  exit 1
fi
print_success "cargo found"

if [ ! -f "$COMPOSE_FILE" ]; then
  print_error "docker-compose.yml not found at $COMPOSE_FILE"
  exit 1
fi
print_success "docker-compose.yml found"

# ============================================================================
# Start Services
# ============================================================================

print_header "Starting Docker Compose Services"

cd "$INTEGRATION_DIR"

# Check if services already running
if docker-compose ps 2>/dev/null | grep -q "postgres-users"; then
  print_info "Services already running. Skipping docker-compose up."
else
  print_info "Starting services..."
  docker-compose up -d
  sleep 5
fi

# ============================================================================
# Wait for Services
# ============================================================================

print_header "Waiting for All Services"

services=(
  "http://localhost:4001/graphql:Users subgraph"
  "http://localhost:4002/graphql:Orders subgraph"
  "http://localhost:4003/graphql:Products subgraph"
  "http://localhost:4000/graphql:Apollo Router gateway"
)

all_ready=true
for service_info in "${services[@]}"; do
  url="${service_info%:*}"
  name="${service_info#*:}"

  if wait_for_service "$url" 30; then
    print_success "$name is ready"
  else
    print_error "$name failed to start"
    all_ready=false
  fi
done

if [ "$all_ready" = false ]; then
  print_error "Some services failed to start. Check logs:"
  echo "  docker-compose logs"
  exit 1
fi

print_success "All services are ready!"

# ============================================================================
# Run Tests
# ============================================================================

print_header "Running 3-Subgraph Federation Tests"

cd "$REPO_ROOT"

# Build test if needed
print_info "Compiling tests..."
cargo test --test federation_docker_compose_integration --no-run --quiet 2>&1 || {
  print_error "Test compilation failed"
  exit 1
}
print_success "Tests compiled successfully"

# Run tests
print_info "Executing tests (filter: $FILTER_PATTERN)..."
echo ""

if [ "$SHOW_LOGS" = true ]; then
  # Show logs in background while tests run
  docker-compose -f "$COMPOSE_FILE" logs -f &
  LOGS_PID=$!
fi

# Run the tests
RUST_BACKTRACE=1 cargo test \
  --test federation_docker_compose_integration \
  "$FILTER_PATTERN" \
  --ignored \
  --nocapture \
  --test-threads=1

TEST_RESULT=$?

if [ "$SHOW_LOGS" = true ] && [ ! -z "$LOGS_PID" ]; then
  kill $LOGS_PID 2>/dev/null || true
fi

# ============================================================================
# Summary and Cleanup
# ============================================================================

echo ""
if [ $TEST_RESULT -eq 0 ]; then
  print_header "Test Results: PASSED ✓"
  print_success "All 3-subgraph federation tests completed successfully!"
else
  print_header "Test Results: FAILED ✗"
  print_error "Some tests failed. Review output above for details."
  echo ""
  print_info "To debug, you can:"
  echo "  1. View service logs: docker-compose logs [service-name]"
  echo "  2. Run a single test: cargo test test_three_subgraph_setup_validation --ignored --nocapture"
  echo "  3. Keep containers running: ./run_3subgraph_tests.sh --no-cleanup"
fi

# Cleanup
if [ "$CLEANUP" = true ]; then
  print_header "Cleaning Up"

  print_info "Stopping Docker Compose services..."
  docker-compose -f "$COMPOSE_FILE" down -v --remove-orphans > /dev/null 2>&1
  print_success "Services stopped and cleaned up"
else
  print_info "Services left running (use 'docker-compose down -v' to stop)"
fi

exit $TEST_RESULT
