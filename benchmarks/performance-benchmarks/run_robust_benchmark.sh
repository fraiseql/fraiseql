#!/bin/bash
# Robust benchmark runner with proper health checks

set -e

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}FraiseQL Performance Benchmark - Robust Version${NC}"
echo -e "${BLUE}==============================================${NC}"

# Clean up function
cleanup() {
    echo -e "\n${YELLOW}Cleaning up...${NC}"
    podman stop bench-postgres bench-fraiseql bench-strawberry 2>/dev/null || true
    podman rm bench-postgres bench-fraiseql bench-strawberry 2>/dev/null || true
}

# Set trap for cleanup on exit
trap cleanup EXIT

# Detect profile
echo -e "\n${YELLOW}Detecting system profile...${NC}"
python3 detect_benchmark_profile.py

# Read profile
if [ -f benchmark_profile.json ]; then
    PROFILE=$(python3 -c "import json; print(json.load(open('benchmark_profile.json'))['profile'])")
    USERS=$(python3 -c "import json; print(json.load(open('benchmark_profile.json'))['data_scale']['users'])")
    PRODUCTS=$(python3 -c "import json; print(json.load(open('benchmark_profile.json'))['data_scale']['products'])")
    ORDERS=$(python3 -c "import json; print(json.load(open('benchmark_profile.json'))['data_scale']['orders'])")

    echo -e "${GREEN}Profile: $PROFILE${NC}"
    echo -e "Scale: ${USERS} users, ${PRODUCTS} products, ${ORDERS} orders"

    export BENCHMARK_USERS=$USERS
    export BENCHMARK_PRODUCTS=$PRODUCTS
    export BENCHMARK_ORDERS=$ORDERS
fi

# Generate seed data
echo -e "\n${YELLOW}Generating seed data...${NC}"
./create_adaptive_seed.sh

# Clean up any existing containers
cleanup

# Start PostgreSQL
echo -e "\n${YELLOW}Starting PostgreSQL...${NC}"
podman run -d \
    --name bench-postgres \
    -e POSTGRES_USER=benchmark \
    -e POSTGRES_PASSWORD=benchmark \
    -e POSTGRES_DB=benchmark_db \
    -p 5433:5432 \
    postgres:15-alpine

# Wait for PostgreSQL to be ready
echo -e "${YELLOW}Waiting for PostgreSQL to be ready...${NC}"
for i in {1..30}; do
    if podman exec bench-postgres pg_isready -U benchmark &> /dev/null; then
        echo -e "${GREEN}✓ PostgreSQL is ready${NC}"
        break
    fi
    echo -n "."
    sleep 1
done

# Initialize database
echo -e "\n${YELLOW}Initializing database schema...${NC}"
podman exec bench-postgres psql -U benchmark -d benchmark_db -f - < shared/database/schema.sql

echo -e "${YELLOW}Loading FraiseQL views...${NC}"
podman exec bench-postgres psql -U benchmark -d benchmark_db -f - < shared/database/fraiseql-views.sql

echo -e "${YELLOW}Loading seed data (this may take a moment)...${NC}"
podman exec bench-postgres psql -U benchmark -d benchmark_db -f - < /tmp/seed-data-generated.sql

# Verify data was loaded
echo -e "\n${YELLOW}Verifying data load...${NC}"
USER_COUNT=$(podman exec bench-postgres psql -U benchmark -d benchmark_db -t -c "SELECT COUNT(*) FROM benchmark.users")
echo -e "Users loaded: ${GREEN}$USER_COUNT${NC}"

# Build FraiseQL
echo -e "\n${YELLOW}Building FraiseQL...${NC}"
cd fraiseql
podman build -t fraiseql-bench -f- . << 'EOF'
FROM python:3.11-slim
WORKDIR /app
RUN apt-get update && apt-get install -y gcc curl && rm -rf /var/lib/apt/lists/*
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt
COPY . .
HEALTHCHECK --interval=5s --timeout=3s --start-period=30s --retries=10 \
    CMD curl -f http://localhost:8000/health || exit 1
CMD ["uvicorn", "benchmark_app:app", "--host", "0.0.0.0", "--port", "8000"]
EOF

# Start FraiseQL
echo -e "\n${YELLOW}Starting FraiseQL...${NC}"
podman run -d \
    --name bench-fraiseql \
    -e DATABASE_URL=postgresql://benchmark:benchmark@host.containers.internal:5433/benchmark_db \
    -p 8001:8000 \
    fraiseql-bench

cd ..

# Build Strawberry
echo -e "\n${YELLOW}Building Strawberry...${NC}"
cd strawberry-sqlalchemy
podman build -t strawberry-bench -f- . << 'EOF'
FROM python:3.11-slim
WORKDIR /app
RUN apt-get update && apt-get install -y gcc curl && rm -rf /var/lib/apt/lists/*
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt
COPY . .
HEALTHCHECK --interval=5s --timeout=3s --start-period=30s --retries=10 \
    CMD curl -f http://localhost:8000/health || exit 1
CMD ["uvicorn", "app:app", "--host", "0.0.0.0", "--port", "8000", "--workers", "1"]
EOF

# Start Strawberry
echo -e "\n${YELLOW}Starting Strawberry...${NC}"
podman run -d \
    --name bench-strawberry \
    -e DATABASE_URL=postgresql://benchmark:benchmark@host.containers.internal:5433/benchmark_db \
    -p 8002:8000 \
    strawberry-bench

cd ..

# Wait for services with proper health checks
echo -e "\n${YELLOW}Waiting for services to be healthy...${NC}"

wait_for_service() {
    local name=$1
    local port=$2
    local max_attempts=60

    echo -e "\nChecking $name..."
    for i in $(seq 1 $max_attempts); do
        if curl -s "http://localhost:$port/health" | grep -q "healthy"; then
            echo -e "${GREEN}✓ $name is healthy${NC}"
            return 0
        fi

        # Check if container is still running
        if ! podman ps | grep -q "bench-$name"; then
            echo -e "${RED}✗ $name container stopped${NC}"
            podman logs bench-$name --tail 20
            return 1
        fi

        if [ $((i % 10)) -eq 0 ]; then
            echo -e "  Still waiting... ($i/$max_attempts)"
        fi
        sleep 1
    done

    echo -e "${RED}✗ $name failed to become healthy${NC}"
    podman logs bench-$name --tail 20
    return 1
}

wait_for_service "fraiseql" 8001 || exit 1
wait_for_service "strawberry" 8002 || exit 1

# Quick GraphQL test
echo -e "\n${YELLOW}Testing GraphQL endpoints...${NC}"

test_endpoint() {
    local name=$1
    local port=$2

    echo -n "  Testing $name... "
    RESPONSE=$(curl -s -X POST \
        -H "Content-Type: application/json" \
        -d '{"query": "{ __typename }"}' \
        "http://localhost:$port/graphql")

    if echo "$RESPONSE" | grep -q "__typename"; then
        echo -e "${GREEN}✓${NC}"
    else
        echo -e "${RED}✗${NC}"
        echo "    Response: $RESPONSE"
    fi
}

test_endpoint "FraiseQL" 8001
test_endpoint "Strawberry" 8002

# Run benchmark
echo -e "\n${GREEN}Running performance benchmark...${NC}"

# Set iterations based on profile
case $PROFILE in
    minimal) export BENCHMARK_ITERATIONS=10; export BENCHMARK_WARMUP=5 ;;
    small) export BENCHMARK_ITERATIONS=20; export BENCHMARK_WARMUP=5 ;;
    medium) export BENCHMARK_ITERATIONS=30; export BENCHMARK_WARMUP=10 ;;
    large) export BENCHMARK_ITERATIONS=40; export BENCHMARK_WARMUP=10 ;;
    xlarge) export BENCHMARK_ITERATIONS=50; export BENCHMARK_WARMUP=10 ;;
esac

python3 benchmark_runner.py

echo -e "\n${GREEN}Benchmark complete!${NC}"
