#!/bin/bash
# Comprehensive benchmark: jsonb_build_object vs row_to_json vs to_jsonb
# Run with: ./run_benchmark.sh [database_name]

set -e

# Configuration
DB_NAME="${1:-postgres}"
DURATION=30  # seconds per test
CLIENTS=10
JOBS=4
REPORT_DIR="./results"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
REPORT_FILE="${REPORT_DIR}/benchmark_${TIMESTAMP}.md"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Create results directory
mkdir -p "${REPORT_DIR}"

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}JSONB Generation Method Benchmark${NC}"
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
# JSONB Generation Method Benchmark Results

**Date:** $(date)
**Database:** ${DB_NAME}
**Test Duration:** ${DURATION} seconds per test
**Concurrency:** ${CLIENTS} clients, ${JOBS} jobs

## Test Scenarios

1. **Single Row Lookup** - UUID-based single record retrieval (most common)
2. **Paginated Query** - 100 rows with OFFSET (GraphQL pagination pattern)
3. **Filtered Query** - WHERE clause with 100 row result
4. **Full Scan** - Complete table scan (worst case)
5. **Trinity Write** - INSERT with GENERATED column computation

---

## Results Summary

EOF

# Function to run a benchmark test
run_test() {
    local test_name=$1
    local test_file=$2
    local view_or_table=$3
    local test_label=$4

    echo -e "${YELLOW}Testing: ${test_label}${NC}"

    # Create test-specific SQL file
    local temp_file="${test_file%.sql}_temp.sql"
    sed "s/v_user_jsonb_build/${view_or_table}/g" "${test_file}" > "${temp_file}"

    # Run pgbench and capture output
    local output=$(pgbench -d "${DB_NAME}" \
        -f "${temp_file}" \
        -c "${CLIENTS}" \
        -j "${JOBS}" \
        -T "${DURATION}" \
        -P 5 \
        --progress-timestamp 2>&1)

    # Extract key metrics
    local tps=$(echo "${output}" | grep "^tps" | awk '{print $3}')
    local latency_avg=$(echo "${output}" | grep "^latency average" | awk '{print $4}')
    local latency_stddev=$(echo "${output}" | grep "^latency stddev" | awk '{print $4}')

    echo "  TPS: ${tps}"
    echo "  Latency (avg): ${latency_avg} ms"
    echo "  Latency (stddev): ${latency_stddev} ms"
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

    # Cleanup temp file
    rm -f "${temp_file}"
}

# Setup database
echo -e "${GREEN}Step 1: Setting up database schema...${NC}"
psql -d "${DB_NAME}" -f 00_setup.sql > /dev/null 2>&1
echo -e "${GREEN}✓ Database setup complete${NC}"
echo ""

# Test configurations
declare -a VIEWS=(
    "v_user_jsonb_build:jsonb_build_object (current)"
    "v_user_row_to_json_lateral:row_to_json with LATERAL"
    "v_user_row_to_json:row_to_json with subquery"
    "v_user_to_jsonb:to_jsonb (simplest)"
)

declare -a TABLES=(
    "tv_user_jsonb_build:Trinity with jsonb_build_object GENERATED"
    "tv_user_to_jsonb:Trinity with to_jsonb GENERATED"
)

# Test 1: Single Row Lookup
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Test 1: Single Row Lookup by UUID${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

cat >> "${REPORT_FILE}" << EOF

## Test 1: Single Row Lookup (UUID-based)

Most common query pattern in GraphQL - fetch single record by ID.

EOF

for view_config in "${VIEWS[@]}"; do
    IFS=':' read -r view label <<< "${view_config}"
    run_test "single_row" "01_test_single_row.sql" "${view}" "${label}"
done

# Test 2: Paginated Query
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Test 2: Paginated Query (100 rows)${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

cat >> "${REPORT_FILE}" << EOF

## Test 2: Paginated Query (100 rows)

Typical GraphQL pagination pattern - LIMIT 100 with random OFFSET.

EOF

for view_config in "${VIEWS[@]}"; do
    IFS=':' read -r view label <<< "${view_config}"
    run_test "paginated" "03_test_paginated.sql" "${view}" "${label}"
done

# Test 3: Filtered Query
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Test 3: Filtered Query${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

cat >> "${REPORT_FILE}" << EOF

## Test 3: Filtered Query

Query with WHERE clause returning ~100 active users.

EOF

for view_config in "${VIEWS[@]}"; do
    IFS=':' read -r view label <<< "${view_config}"
    run_test "filtered" "04_test_filtered.sql" "${view}" "${label}"
done

# Test 4: Full Scan (lower concurrency to avoid overwhelming DB)
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Test 4: Full Table Scan${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

cat >> "${REPORT_FILE}" << EOF

## Test 4: Full Table Scan (10,000 rows)

Worst case scenario - SELECT all rows with JSONB generation.

EOF

CLIENTS_BACKUP="${CLIENTS}"
JOBS_BACKUP="${JOBS}"
CLIENTS=4
JOBS=2

for view_config in "${VIEWS[@]}"; do
    IFS=':' read -r view label <<< "${view_config}"
    run_test "full_scan" "02_test_full_scan.sql" "${view}" "${label}"
done

CLIENTS="${CLIENTS_BACKUP}"
JOBS="${JOBS_BACKUP}"

# Test 5: Trinity Write Performance
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Test 5: Trinity Table Write Performance${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

cat >> "${REPORT_FILE}" << EOF

## Test 5: Trinity Table Write Performance

INSERT performance with GENERATED JSONB column computation.

EOF

for table_config in "${TABLES[@]}"; do
    IFS=':' read -r table label <<< "${table_config}"

    # Create table-specific write test
    temp_file="05_test_trinity_write_temp.sql"
    sed "s/tv_user_jsonb_build/${table}/g" "05_test_trinity_write.sql" > "${temp_file}"

    run_test "trinity_write" "${temp_file}" "${table}" "${label}"

    rm -f "${temp_file}"
done

# Generate comparison chart
cat >> "${REPORT_FILE}" << EOF

---

## Performance Comparison Summary

### Single Row Lookup (TPS)
| Method | TPS | vs Baseline |
|--------|-----|-------------|
EOF

# Extract TPS values for comparison chart
baseline_tps=$(grep -A 2 "jsonb_build_object (current)" "${REPORT_FILE}" | grep "TPS:" | awk '{print $3}' | head -1)

for view_config in "${VIEWS[@]}"; do
    IFS=':' read -r view label <<< "${view_config}"
    tps=$(grep -A 2 "${label}" "${REPORT_FILE}" | grep "TPS:" | awk '{print $3}' | head -1)

    if [ -n "${tps}" ] && [ -n "${baseline_tps}" ]; then
        improvement=$(awk "BEGIN {printf \"%.1f%%\", (($tps / $baseline_tps) - 1) * 100}")
        echo "| ${label} | ${tps} | ${improvement} |" >> "${REPORT_FILE}"
    fi
done

# Storage comparison
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Storage Analysis${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

cat >> "${REPORT_FILE}" << EOF

---

## Storage Analysis

### Table Sizes

EOF

psql -d "${DB_NAME}" -t -A -F '|' << 'EOSQL' >> "${REPORT_FILE}"
SELECT
    '| ' || table_name || ' | ' ||
    pg_size_pretty(pg_total_relation_size(table_name::regclass)) || ' | ' ||
    pg_size_pretty(pg_relation_size(table_name::regclass)) || ' | ' ||
    pg_size_pretty(pg_total_relation_size(table_name::regclass) - pg_relation_size(table_name::regclass)) || ' |'
FROM (VALUES
    ('tb_user_bench'),
    ('tv_user_jsonb_build'),
    ('tv_user_to_jsonb')
) AS t(table_name);
EOSQL

# Add table header
sed -i '/## Storage Analysis/a\\n| Table | Total Size | Data Size | Index Size |\n|-------|-----------|-----------|------------|' "${REPORT_FILE}"

# Finalize report
cat >> "${REPORT_FILE}" << EOF

---

## Conclusions

### Key Findings

1. **Single Row Lookup Performance:**
   - Compare TPS values above to determine fastest method
   - Most critical metric for GraphQL APIs

2. **Bulk Query Performance:**
   - Paginated and filtered queries show scaling characteristics
   - Full scan tests maximum throughput

3. **Write Performance:**
   - Trinity tables with GENERATED columns show INSERT overhead
   - Compare jsonb_build_object vs to_jsonb generation cost

4. **Storage Efficiency:**
   - Generated columns add storage overhead
   - Indexes contribute significantly to total size

### Recommendations

Based on the benchmark results:
- **For Views:** Choose the method with best TPS/latency for your workload
- **For Trinity Tables:** Consider generation overhead vs query performance tradeoff
- **For Production:** Test with your actual data patterns and query complexity

---

**Benchmark completed:** $(date)

EOF

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}Benchmark Complete!${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo -e "Report saved to: ${GREEN}${REPORT_FILE}${NC}"
echo ""
echo "View results with:"
echo "  cat ${REPORT_FILE}"
echo ""
echo "Or open in your browser/markdown viewer"
