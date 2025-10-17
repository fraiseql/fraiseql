#!/bin/bash
# Pure function benchmark: Isolate function overhead from query pattern overhead
# Run with: ./run_pure_function_benchmark.sh [database_name]

set -e

# Configuration
DB_NAME="${1:-postgres}"
DURATION=30
CLIENTS=10
JOBS=4
REPORT_DIR="./results"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
REPORT_FILE="${REPORT_DIR}/pure_function_benchmark_${TIMESTAMP}.md"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

mkdir -p "${REPORT_DIR}"

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Pure Function Benchmark${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""
echo "Database: ${DB_NAME}"
echo "Duration per test: ${DURATION}s"
echo "Clients: ${CLIENTS}"
echo "Jobs: ${JOBS}"
echo "Report: ${REPORT_FILE}"
echo ""

# Initialize report
cat > "${REPORT_FILE}" << EOF
# Pure Function Benchmark Results

**Purpose:** Measure ONLY function overhead, no joins or subquery overhead

**Date:** $(date)
**Database:** ${DB_NAME}
**Test Duration:** ${DURATION} seconds per test
**Concurrency:** ${CLIENTS} clients, ${JOBS} jobs

## Test Setup

These tests use **direct function calls** with no LATERAL joins or nested subqueries:

- **jsonb_build_object**: \`SELECT jsonb_build_object(...) FROM table\`
- **row_to_json**: \`SELECT row_to_json(ROW(...)) FROM table\` (no subquery!)
- **to_jsonb**: \`SELECT to_jsonb(table) FROM table\`

This isolates pure function performance from query pattern overhead.

---

## Results

EOF

# Function to run benchmark
run_test() {
    local test_name=$1
    local test_file=$2
    local view_name=$3
    local test_label=$4

    echo -e "${YELLOW}Testing: ${test_label}${NC}"

    # Create test-specific SQL file
    local temp_file="${test_file%.sql}_temp.sql"
    sed "s/v_pure_jsonb_build/${view_name}/g" "${test_file}" > "${temp_file}"

    # Run pgbench
    local output=$(pgbench -d "${DB_NAME}" \
        -f "${temp_file}" \
        -c "${CLIENTS}" \
        -j "${JOBS}" \
        -T "${DURATION}" \
        -P 5 \
        --progress-timestamp 2>&1)

    # Extract metrics
    local tps=$(echo "${output}" | grep "^tps" | awk '{print $3}')
    local latency_avg=$(echo "${output}" | grep "^latency average" | awk '{print $4}')
    local latency_stddev=$(echo "${output}" | grep "^latency stddev" | awk '{print $4}')

    echo "  TPS: ${tps}"
    echo "  Latency (avg): ${latency_avg} ms"
    echo ""

    # Append to report
    cat >> "${REPORT_FILE}" << EOF
### ${test_label}

- **TPS:** ${tps}
- **Latency (avg):** ${latency_avg} ms
- **Latency (stddev):** ${latency_stddev} ms

<details>
<summary>Full Output</summary>

\`\`\`
${output}
\`\`\`

</details>

EOF

    rm -f "${temp_file}"
}

# Setup
echo -e "${GREEN}Setting up pure function test views...${NC}"
psql -d "${DB_NAME}" -f 00_setup_pure_functions.sql > /dev/null 2>&1
echo -e "${GREEN}✓ Setup complete${NC}"
echo ""

# Create test SQL files for each scenario
mkdir -p /tmp/pure_tests

# Full scan test
cat > /tmp/pure_tests/full_scan.sql << 'EOF'
SELECT id, identifier, data FROM v_pure_jsonb_build;
EOF

# Paginated test
cat > /tmp/pure_tests/paginated.sql << 'EOF'
\set offset random(0, 9900)
SELECT id, identifier, data FROM v_pure_jsonb_build
ORDER BY id LIMIT 100 OFFSET :offset;
EOF

# Filtered test
cat > /tmp/pure_tests/filtered.sql << 'EOF'
SELECT id, identifier, data FROM v_pure_jsonb_build
WHERE id IN (SELECT id FROM tb_user_bench WHERE is_active = true LIMIT 100);
EOF

# Test 1: Full Scan
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Test 1: Full Table Scan (10,000 rows)${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

cat >> "${REPORT_FILE}" << EOF

## Test 1: Full Table Scan (10,000 rows)

Pure function overhead for full table scans.

EOF

CLIENTS_BACKUP="${CLIENTS}"
JOBS_BACKUP="${JOBS}"
CLIENTS=4
JOBS=2

run_test "full_scan" "/tmp/pure_tests/full_scan.sql" "v_pure_jsonb_build" "jsonb_build_object (pure)"
run_test "full_scan" "/tmp/pure_tests/full_scan.sql" "v_pure_row_to_json" "row_to_json(ROW(...)) (pure)"
run_test "full_scan" "/tmp/pure_tests/full_scan.sql" "v_pure_to_jsonb" "to_jsonb (pure)"

CLIENTS="${CLIENTS_BACKUP}"
JOBS="${JOBS_BACKUP}"

# Test 2: Paginated
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Test 2: Paginated Query (100 rows)${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

cat >> "${REPORT_FILE}" << EOF

## Test 2: Paginated Query (100 rows)

Pure function overhead for paginated queries.

EOF

run_test "paginated" "/tmp/pure_tests/paginated.sql" "v_pure_jsonb_build" "jsonb_build_object (pure)"
run_test "paginated" "/tmp/pure_tests/paginated.sql" "v_pure_row_to_json" "row_to_json(ROW(...)) (pure)"
run_test "paginated" "/tmp/pure_tests/paginated.sql" "v_pure_to_jsonb" "to_jsonb (pure)"

# Test 3: Filtered
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Test 3: Filtered Query${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

cat >> "${REPORT_FILE}" << EOF

## Test 3: Filtered Query

Pure function overhead for filtered queries.

EOF

run_test "filtered" "/tmp/pure_tests/filtered.sql" "v_pure_jsonb_build" "jsonb_build_object (pure)"
run_test "filtered" "/tmp/pure_tests/filtered.sql" "v_pure_row_to_json" "row_to_json(ROW(...)) (pure)"
run_test "filtered" "/tmp/pure_tests/filtered.sql" "v_pure_to_jsonb" "to_jsonb (pure)"

# Generate comparison
cat >> "${REPORT_FILE}" << EOF

---

## Summary

### Full Table Scan Performance
EOF

# Extract TPS values for comparison
baseline_full=$(grep -A 2 "jsonb_build_object (pure)" "${REPORT_FILE}" | grep "TPS:" | head -1 | awk '{print $3}')
row_full=$(grep -A 2 "row_to_json(ROW(...)) (pure)" "${REPORT_FILE}" | grep "TPS:" | head -1 | awk '{print $3}')
to_full=$(grep -A 2 "to_jsonb (pure)" "${REPORT_FILE}" | grep "TPS:" | tail -1 | awk '{print $3}')

cat >> "${REPORT_FILE}" << EOF

| Method | TPS | vs jsonb_build_object |
|--------|-----|-----------------------|
| jsonb_build_object | ${baseline_full} | Baseline |
| row_to_json(ROW(...)) | ${row_full} | $(awk "BEGIN {printf \"%.1f%%\", (($row_full / $baseline_full) - 1) * 100}") |
| to_jsonb | ${to_full} | $(awk "BEGIN {printf \"%.1f%%\", (($to_full / $baseline_full) - 1) * 100}") |

---

## Key Findings

1. **Pure Function Performance**: These results show the actual function overhead WITHOUT query pattern complications
2. **Comparison to Full Benchmark**: Differences between this and the full benchmark reveal query pattern overhead (LATERAL joins, subqueries)
3. **PostgreSQL Version**: Functions may have different performance characteristics across PostgreSQL versions

---

**Benchmark completed:** $(date)

EOF

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}Pure Function Benchmark Complete!${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo -e "Report saved to: ${GREEN}${REPORT_FILE}${NC}"
echo ""
echo "Compare this with results/benchmark_*.md to see query pattern overhead!"
