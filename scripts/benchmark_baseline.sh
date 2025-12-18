#!/bin/bash
# Run full benchmark suite and capture baseline

set -e

BENCH_DIR="fraiseql_rs/target/criterion"
BASELINE_DIR="performance/baselines"
DATE=$(date +%Y-%m-%d_%H-%M-%S)

echo "🚀 Running performance baselines..."

# Create baseline directory
mkdir -p "$BASELINE_DIR"

# Run all benchmarks
cd fraiseql_rs
cargo bench --bench connection_pool -- --output-format bencher | tee "../$BASELINE_DIR/connection_pool_$DATE.txt"
cargo bench --bench query_execution -- --output-format bencher | tee "../$BASELINE_DIR/query_execution_$DATE.txt"
cargo bench --bench streaming -- --output-format bencher | tee "../$BASELINE_DIR/streaming_$DATE.txt"

echo ""
echo "✅ Baselines captured:"
ls -lh "$BASELINE_DIR/"

echo ""
echo "📊 HTML reports available in:"
echo "  $BENCH_DIR"
echo ""
echo "View with: open $BENCH_DIR/report/index.html"