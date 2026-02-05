#!/bin/bash

##############################################################################
# Extended Mutations Integration Test Runner
#
# This script starts the Docker Compose environment, runs the extended
# mutations federation tests, and provides a summary of results.
#
# Usage:
#   ./run_extended_mutation_tests.sh [--no-cleanup]
#
# Options:
#   --no-cleanup    Don't stop Docker Compose after tests (for debugging)
##############################################################################

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
NO_CLEANUP=${1:-false}

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Functions
log_header() {
    echo -e "${BLUE}============================================================================${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}============================================================================${NC}"
}

log_info() {
    echo -e "${BLUE}➜${NC} $1"
}

log_success() {
    echo -e "${GREEN}✓${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}⚠${NC} $1"
}

log_error() {
    echo -e "${RED}✗${NC} $1"
}

cleanup_on_exit() {
    if [ "$NO_CLEANUP" != "--no-cleanup" ]; then
        log_info "Cleaning up Docker Compose environment..."
        cd "$SCRIPT_DIR"
        docker-compose down -v --remove-orphans 2>/dev/null || true
        log_success "Cleanup complete"
    else
        log_warning "Docker Compose environment still running (use 'docker-compose down -v' to clean up)"
    fi
}

# Trap exit to cleanup
trap cleanup_on_exit EXIT

# Main test execution
main() {
    log_header "Extended Mutations Integration Tests"
    echo

    # Step 1: Check Docker
    log_info "Checking Docker installation..."
    if ! command -v docker &> /dev/null; then
        log_error "Docker is not installed"
        exit 1
    fi
    log_success "Docker found"

    if ! command -v docker-compose &> /dev/null; then
        log_error "docker-compose is not installed"
        exit 1
    fi
    log_success "docker-compose found"
    echo

    # Step 2: Start Docker Compose
    log_info "Starting Docker Compose environment..."
    cd "$SCRIPT_DIR"

    if ! docker-compose up -d 2>&1 | grep -E "(Starting|Created|Creating)"; then
        log_error "Failed to start Docker Compose"
        exit 1
    fi
    log_success "Docker Compose environment started"
    echo

    # Step 3: Wait for services
    log_info "Waiting for services to be healthy (this may take 30-60 seconds)..."
    WAIT_TIME=0
    MAX_WAIT=120
    SERVICES_HEALTHY=false

    while [ $WAIT_TIME -lt $MAX_WAIT ]; do
        # Check if all services are healthy
        HEALTH_STATUS=$(docker-compose ps --services --filter "status=running" 2>/dev/null | wc -l)

        if [ "$HEALTH_STATUS" -ge 4 ]; then
            # Do a quick GraphQL health check
            if curl -s -X POST http://localhost:4002/graphql \
                -H "Content-Type: application/json" \
                -d '{"query": "{ __typename }"}' > /dev/null 2>&1; then
                SERVICES_HEALTHY=true
                break
            fi
        fi

        echo -ne "\r⏳ Waiting... ${WAIT_TIME}s"
        sleep 2
        WAIT_TIME=$((WAIT_TIME + 2))
    done

    if [ "$SERVICES_HEALTHY" = false ]; then
        log_error "Services did not become healthy within ${MAX_WAIT}s"
        log_info "Docker Compose status:"
        docker-compose ps
        exit 1
    fi

    echo
    log_success "All services are healthy"
    log_info "Service endpoints:"
    echo "  - Users Subgraph: http://localhost:4001/graphql"
    echo "  - Orders Subgraph: http://localhost:4002/graphql"
    echo "  - Apollo Router: http://localhost:4000/graphql"
    echo

    # Step 4: Run tests
    log_header "Running Extended Mutations Tests"
    echo

    cd "$PROJECT_ROOT"

    # Collect test results
    TEST_RESULTS_FILE="/tmp/extended_mutation_test_results_$$.txt"

    cargo test --test federation_docker_compose_integration \
        test_extended_mutation_ \
        --ignored \
        --nocapture \
        2>&1 | tee "$TEST_RESULTS_FILE"

    CARGO_EXIT_CODE=${PIPESTATUS[0]}
    echo

    # Step 5: Analyze results
    log_header "Test Results Summary"
    echo

    if [ $CARGO_EXIT_CODE -eq 0 ]; then
        log_success "All extended mutation tests passed!"

        # Count test results
        PASSED=$(grep -c "^test.*ok" "$TEST_RESULTS_FILE" || echo "0")
        echo "  Tests passed: $PASSED"
    else
        log_error "Some tests failed (exit code: $CARGO_EXIT_CODE)"

        # Show failures
        if grep -q "test result:" "$TEST_RESULTS_FILE"; then
            TEST_SUMMARY=$(grep "test result:" "$TEST_RESULTS_FILE")
            echo "  $TEST_SUMMARY"
        fi
    fi
    echo

    # Step 6: Show Docker Compose status
    log_info "Docker Compose service status:"
    docker-compose ps | tail -n +3
    echo

    # Step 7: Show recommendations
    log_header "Extended Mutations Testing Complete"

    if [ $CARGO_EXIT_CODE -eq 0 ]; then
        log_success "All extended mutation tests passed!"
        echo
        log_info "Next steps:"
        echo "  • Task #4: Composite key integration tests"
        echo "  • Task #5: 3+ subgraph federation tests"
        echo "  • Task #6: Apollo Router schema composition verification"
    else
        log_warning "Some extended mutation tests did not pass"
        echo
        log_info "Debugging hints:"
        echo "  • View logs: docker-compose logs -f orders-subgraph"
        echo "  • Check mutation status: curl -X POST http://localhost:4002/graphql ..."
        echo "  • Review test output above for specific errors"
    fi

    exit $CARGO_EXIT_CODE
}

# Run main
main "$@"
