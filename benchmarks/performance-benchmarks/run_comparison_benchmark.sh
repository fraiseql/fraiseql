#!/bin/bash

set -e

echo "🚀 FraiseQL vs Hasura vs PostGraphile Benchmark"
echo "=============================================="

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Create results directory
mkdir -p results/comparison_$(date +%Y%m%d_%H%M%S)
RESULTS_DIR="results/comparison_$(date +%Y%m%d_%H%M%S)"

echo -e "${BLUE}📊 Results will be saved to: ${RESULTS_DIR}${NC}"

# Test queries
SIMPLE_QUERY='{
  "query": "{ users(limit: 10) { id email name created_at } }"
}'

NESTED_QUERY='{
  "query": "{ users(limit: 5) { id name posts(limit: 3) { id title comments(limit: 2) { id content author { name } } } } }"
}'

COMPLEX_QUERY='{
  "query": "{ organizations(limit: 3) { id name departments { id name projects { id name tasks { id title assignee { name } } } } }"
}'

# Function to run load test with k6
run_load_test() {
    local framework=$1
    local url=$2
    local query=$3
    local test_name=$4

    echo -e "${YELLOW}🔧 Testing ${framework} - ${test_name}${NC}"

    # Create k6 test script
    cat > /tmp/k6_test.js << EOF
import http from 'k6/http';
import { check } from 'k6';

export let options = {
  stages: [
    { duration: '30s', target: 10 },
    { duration: '60s', target: 50 },
    { duration: '30s', target: 0 },
  ],
};

export default function() {
  let payload = ${query};
  let params = {
    headers: {
      'Content-Type': 'application/json',
    },
  };

  let response = http.post('${url}', JSON.stringify(payload), params);

  check(response, {
    'status is 200': (r) => r.status === 200,
    'response time < 1000ms': (r) => r.timings.duration < 1000,
  });
}
EOF

    # Run k6 test
    k6 run --out json=${RESULTS_DIR}/${framework}_${test_name}_results.json /tmp/k6_test.js > ${RESULTS_DIR}/${framework}_${test_name}_output.log 2>&1 || true
}

# Function to start service and wait for health
wait_for_service() {
    local url=$1
    local service_name=$2
    local max_attempts=60
    local attempt=1

    echo -e "${YELLOW}⏳ Waiting for ${service_name} to be ready...${NC}"

    while [ $attempt -le $max_attempts ]; do
        if curl -s -f "$url" > /dev/null 2>&1; then
            echo -e "${GREEN}✅ ${service_name} is ready!${NC}"
            return 0
        fi
        echo "Attempt $attempt/$max_attempts..."
        sleep 2
        ((attempt++))
    done

    echo -e "${RED}❌ ${service_name} failed to start${NC}"
    return 1
}

# Test FraiseQL (using existing unified container)
echo -e "${BLUE}🔵 Testing FraiseQL${NC}"
if podman ps --format "{{.Names}}" | grep -q "fraiseql-final-test"; then
    echo -e "${GREEN}✅ FraiseQL container already running${NC}"
else
    echo -e "${YELLOW}🔄 Starting FraiseQL container...${NC}"
    podman run -d --name fraiseql-benchmark -p 8000:8000 localhost/fraiseql-blog-unified || true
    sleep 15
fi

if wait_for_service "http://localhost:8000/playground" "FraiseQL"; then
    run_load_test "fraiseql" "http://localhost:8000/graphql" "$SIMPLE_QUERY" "simple"
    run_load_test "fraiseql" "http://localhost:8000/graphql" "$NESTED_QUERY" "nested"
    # run_load_test "fraiseql" "http://localhost:8000/graphql" "$COMPLEX_QUERY" "complex"
fi

# Clean up FraiseQL
podman stop fraiseql-benchmark 2>/dev/null || true
podman rm fraiseql-benchmark 2>/dev/null || true

# Test Hasura
echo -e "${BLUE}🟡 Testing Hasura${NC}"
cd hasura
docker-compose up -d
if wait_for_service "http://localhost:8080/healthz" "Hasura"; then
    run_load_test "hasura" "http://localhost:8080/v1/graphql" "$SIMPLE_QUERY" "simple"
    run_load_test "hasura" "http://localhost:8080/v1/graphql" "$NESTED_QUERY" "nested"
    # run_load_test "hasura" "http://localhost:8080/v1/graphql" "$COMPLEX_QUERY" "complex"
fi
docker-compose down
cd ..

# Test PostGraphile
echo -e "${BLUE}🟣 Testing PostGraphile${NC}"
cd postgraphile
docker-compose up -d
if wait_for_service "http://localhost:5000/graphql" "PostGraphile"; then
    run_load_test "postgraphile" "http://localhost:5000/graphql" "$SIMPLE_QUERY" "simple"
    run_load_test "postgraphile" "http://localhost:5000/graphql" "$NESTED_QUERY" "nested"
    # run_load_test "postgraphile" "http://localhost:5000/graphql" "$COMPLEX_QUERY" "complex"
fi
docker-compose down
cd ..

echo -e "${GREEN}🎉 Benchmark completed! Results saved to ${RESULTS_DIR}${NC}"

# Generate summary report
echo -e "${BLUE}📈 Generating summary report...${NC}"
python3 << EOF
import json
import os
import glob

results_dir = "${RESULTS_DIR}"
frameworks = ["fraiseql", "hasura", "postgraphile"]
test_types = ["simple", "nested"]

print("\\n" + "="*60)
print("     PERFORMANCE COMPARISON SUMMARY")
print("="*60)

for test_type in test_types:
    print(f"\\n🔍 {test_type.upper()} QUERIES:")
    print("-" * 40)

    for framework in frameworks:
        result_file = f"{results_dir}/{framework}_{test_type}_results.json"

        if os.path.exists(result_file):
            try:
                with open(result_file, 'r') as f:
                    lines = f.readlines()
                    if lines:
                        # Parse last line which contains summary
                        summary_line = [line for line in lines if '"type":"Point"' in line and '"metric":"http_req_duration"' in line]
                        if summary_line:
                            data = json.loads(summary_line[-1])
                            avg_duration = data['data']['value']
                            print(f"  {framework:12}: {avg_duration:6.1f}ms average")
                        else:
                            print(f"  {framework:12}: No duration data")
            except Exception as e:
                print(f"  {framework:12}: Error reading results")
        else:
            print(f"  {framework:12}: No results file")

print("\\n" + "="*60)
print("✅ Benchmark analysis complete!")
print("\\n📁 Detailed results available in:")
print(f"   {results_dir}/")
EOF

echo -e "${GREEN}🏁 All benchmarks completed successfully!${NC}"
