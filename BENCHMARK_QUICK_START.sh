#!/bin/bash
# FraiseQL Baseline Benchmarks - Quick Start Script
# Usage: bash BENCHMARK_QUICK_START.sh [target]
# Targets: setup, run-small, run-medium, run-large, run-all, clean

set -e

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DB_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql"
TARGET="${1:-setup}"

echo "=================================================="
echo "FraiseQL Benchmark Quick Start"
echo "=================================================="
echo ""
echo "Target: $TARGET"
echo "Project: $PROJECT_ROOT"
echo ""

# Color codes
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

info() {
  echo -e "${BLUE}→${NC} $1"
}

success() {
  echo -e "${GREEN}✓${NC} $1"
}

warn() {
  echo -e "${YELLOW}!${NC} $1"
}

case $TARGET in
  setup)
    echo "Setting up benchmark environment..."
    echo ""
    
    info "Step 1: Starting Docker Compose services"
    cd "$PROJECT_ROOT"
    docker compose -f docker-compose.yml up -d
    echo "Waiting 30 seconds for services to be ready..."
    sleep 30
    
    info "Step 2: Verifying PostgreSQL connection"
    if psql "$DB_URL" -c "SELECT version();" > /dev/null 2>&1; then
      success "PostgreSQL is accessible"
    else
      echo "ERROR: Could not connect to PostgreSQL"
      exit 1
    fi
    
    info "Step 3: Loading benchmark data (1M rows)"
    warn "This may take 1-2 minutes..."
    psql "$DB_URL" < "$PROJECT_ROOT/crates/fraiseql-core/benches/fixtures/setup_bench_data.sql"
    
    info "Step 4: Verifying data loaded"
    COUNT=$(psql "$DB_URL" -t -c "SELECT COUNT(*) FROM v_benchmark_data;" | tr -d ' ')
    success "Data loaded: $COUNT rows"
    
    echo ""
    echo "Setup complete! Run benchmarks with:"
    echo "  bash BENCHMARK_QUICK_START.sh run-small"
    echo "  bash BENCHMARK_QUICK_START.sh run-medium"
    echo "  bash BENCHMARK_QUICK_START.sh run-large"
    echo "  bash BENCHMARK_QUICK_START.sh run-all"
    ;;

  run-small)
    echo "Running small dataset benchmarks (10K rows)..."
    export DATABASE_URL="$DB_URL"
    cd "$PROJECT_ROOT"
    cargo bench --bench adapter_comparison --features "postgres,wire-backend" -- "10k_rows"
    success "Small benchmark complete"
    ;;

  run-medium)
    echo "Running medium dataset benchmarks (100K rows)..."
    export DATABASE_URL="$DB_URL"
    cd "$PROJECT_ROOT"
    cargo bench --bench adapter_comparison --features "postgres,wire-backend" -- "100k_rows"
    success "Medium benchmark complete"
    ;;

  run-large)
    echo "Running large dataset benchmarks (1M rows)..."
    warn "This will take several minutes and use significant memory"
    export DATABASE_URL="$DB_URL"
    cd "$PROJECT_ROOT"
    cargo bench --bench adapter_comparison --features "postgres,wire-backend" -- "1m_rows"
    success "Large benchmark complete"
    ;;

  run-all)
    echo "Running all benchmarks..."
    warn "This will take 15-30 minutes"
    export DATABASE_URL="$DB_URL"
    cd "$PROJECT_ROOT"
    cargo bench --features "postgres,wire-backend"
    success "All benchmarks complete"
    ;;

  wire-micro)
    echo "Running fraiseql-wire micro benchmarks..."
    export DATABASE_URL="$DB_URL"
    cd "$PROJECT_ROOT"
    cargo bench --bench micro_benchmarks -p fraiseql-wire
    success "Micro benchmarks complete"
    ;;

  clean)
    echo "Cleaning up benchmark environment..."
    cd "$PROJECT_ROOT"
    info "Stopping Docker containers"
    docker compose -f docker-compose.yml down
    success "Environment cleaned up"
    ;;

  report)
    echo "Opening benchmark results..."
    if [ -f "$PROJECT_ROOT/target/criterion/report/index.html" ]; then
      if command -v open &> /dev/null; then
        open "$PROJECT_ROOT/target/criterion/report/index.html"
      else
        warn "Please open in browser: $PROJECT_ROOT/target/criterion/report/index.html"
      fi
    else
      echo "ERROR: No benchmark results found. Run benchmarks first."
      exit 1
    fi
    ;;

  *)
    echo "Usage: bash BENCHMARK_QUICK_START.sh [target]"
    echo ""
    echo "Available targets:"
    echo "  setup          - Start Docker, load test data (run first!)"
    echo "  run-small      - Run 10K row benchmarks"
    echo "  run-medium     - Run 100K row benchmarks"
    echo "  run-large      - Run 1M row benchmarks"
    echo "  run-all        - Run all benchmarks"
    echo "  wire-micro     - Run fraiseql-wire micro benchmarks only"
    echo "  report         - Open HTML report in browser"
    echo "  clean          - Stop Docker containers"
    echo ""
    echo "Quick start:"
    echo "  1. bash BENCHMARK_QUICK_START.sh setup"
    echo "  2. bash BENCHMARK_QUICK_START.sh run-small"
    echo "  3. bash BENCHMARK_QUICK_START.sh report"
    exit 1
    ;;
esac

echo ""
echo "=================================================="
