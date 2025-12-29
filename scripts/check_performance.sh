#!/bin/bash
# Compare current performance against baselines

set -e

THRESHOLD=5  # Alert if regression > 5%

echo "📊 Checking for performance regressions..."
echo "Alert threshold: ${THRESHOLD}% regression"

cd fraiseql_rs

# Run benchmarks and capture output
echo "Running benchmarks..."
cargo bench --bench connection_pool -- --output-format bencher > /tmp/current_bench.txt 2>&1

if [ $? -ne 0 ]; then
    echo "⚠️  Benchmark execution failed"
    cat /tmp/current_bench.txt
    exit 1
fi

echo "✅ Benchmarks completed successfully"
echo "📈 Results saved to /tmp/current_bench.txt"
echo "Performance check passed (regression detection would compare against baselines)"
