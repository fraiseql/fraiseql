#!/bin/bash
# Final benchmark runner - simplified and working

set -e

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}╔══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║           FraiseQL Performance Benchmark Suite               ║${NC}"
echo -e "${BLUE}╚══════════════════════════════════════════════════════════════╝${NC}"

# Clean up function
cleanup() {
    echo -e "\n${YELLOW}Cleaning up containers...${NC}"
    podman stop postgres-bench fraiseql-bench strawberry-bench 2>/dev/null || true
    podman rm postgres-bench fraiseql-bench strawberry-bench 2>/dev/null || true
}

# Clean up first
cleanup

# Detect profile
echo -e "\n${YELLOW}🔍 Detecting system capabilities...${NC}"
python3 detect_benchmark_profile.py

# Read profile
if [ -f benchmark_profile.json ]; then
    PROFILE=$(python3 -c "import json; print(json.load(open('benchmark_profile.json'))['profile'])")
    USERS=$(python3 -c "import json; print(json.load(open('benchmark_profile.json'))['data_scale']['users'])")
    PRODUCTS=$(python3 -c "import json; print(json.load(open('benchmark_profile.json'))['data_scale']['products'])")
    ORDERS=$(python3 -c "import json; print(json.load(open('benchmark_profile.json'))['data_scale']['orders'])")

    echo -e "${GREEN}📊 Profile: $PROFILE${NC}"
    echo -e "   👥 Users: ${BLUE}${USERS}${NC}"
    echo -e "   📦 Products: ${BLUE}${PRODUCTS}${NC}"
    echo -e "   🛒 Orders: ${BLUE}${ORDERS}${NC}"

    export BENCHMARK_USERS=$USERS
    export BENCHMARK_PRODUCTS=$PRODUCTS
    export BENCHMARK_ORDERS=$ORDERS
else
    echo -e "${RED}Failed to detect profile${NC}"
    exit 1
fi

# Generate seed data
echo -e "\n${YELLOW}📝 Generating adaptive seed data...${NC}"
./create_adaptive_seed.sh

# Start PostgreSQL
echo -e "\n${YELLOW}🐘 Starting PostgreSQL...${NC}"
podman run -d \
    --name postgres-bench \
    -e POSTGRES_PASSWORD=benchmark \
    -e POSTGRES_USER=benchmark \
    -e POSTGRES_DB=benchmark_db \
    -p 5433:5432 \
    postgres:15-alpine

# Wait for PostgreSQL
echo -e "${YELLOW}⏳ Waiting for PostgreSQL...${NC}"
sleep 10

# Copy and run SQL files
echo -e "\n${YELLOW}🗄️ Setting up database...${NC}"
podman cp shared/database/schema.sql postgres-bench:/tmp/
podman cp shared/database/fraiseql-views.sql postgres-bench:/tmp/
podman cp /tmp/seed-data-generated.sql postgres-bench:/tmp/

echo -e "  Creating schema..."
podman exec postgres-bench psql -U benchmark -d benchmark_db -f /tmp/schema.sql

echo -e "  Creating views..."
podman exec postgres-bench psql -U benchmark -d benchmark_db -f /tmp/fraiseql-views.sql

echo -e "  Loading data (this will take a moment)..."
podman exec postgres-bench psql -U benchmark -d benchmark_db -f /tmp/seed-data-generated.sql

# Verify
USER_COUNT=$(podman exec postgres-bench psql -U benchmark -d benchmark_db -t -c "SELECT COUNT(*) FROM benchmark.users" | tr -d ' ')
echo -e "${GREEN}✓ Data loaded: ${USER_COUNT} users${NC}"

# Run FraiseQL directly from source
echo -e "\n${YELLOW}🚀 Starting FraiseQL...${NC}"
cd fraiseql
podman run -d \
    --name fraiseql-bench \
    -e DATABASE_URL=postgresql://benchmark:benchmark@host.containers.internal:5433/benchmark_db \
    -p 8001:8000 \
    -v $(pwd):/app:z \
    -w /app \
    python:3.11-slim \
    bash -c "apt-get update && apt-get install -y gcc curl && pip install -r requirements.txt && uvicorn benchmark_app:app --host 0.0.0.0 --port 8000"

cd ..

# Run Strawberry
echo -e "\n${YELLOW}🍓 Starting Strawberry...${NC}"
cd strawberry-sqlalchemy
podman run -d \
    --name strawberry-bench \
    -e DATABASE_URL=postgresql://benchmark:benchmark@host.containers.internal:5433/benchmark_db \
    -p 8002:8000 \
    -v $(pwd):/app:z \
    -w /app \
    python:3.11-slim \
    bash -c "apt-get update && apt-get install -y gcc curl && pip install -r requirements.txt && uvicorn app:app --host 0.0.0.0 --port 8000"

cd ..

# Wait for services
echo -e "\n${YELLOW}⏳ Waiting for services to start (this takes a minute)...${NC}"
sleep 60

# Check services
check_service() {
    local name=$1
    local port=$2

    echo -n "  Checking $name... "
    if curl -s "http://localhost:$port/health" 2>/dev/null | grep -q "healthy"; then
        echo -e "${GREEN}✓ Ready${NC}"
        return 0
    else
        echo -e "${RED}✗ Not ready${NC}"
        echo "  Logs:"
        podman logs ${name}-bench --tail 10
        return 1
    fi
}

echo -e "\n${YELLOW}🏥 Health checks:${NC}"
check_service "fraiseql" 8001
check_service "strawberry" 8002

# Test queries
echo -e "\n${YELLOW}🧪 Testing GraphQL endpoints...${NC}"
echo -n "  FraiseQL: "
curl -s -X POST -H "Content-Type: application/json" \
    -d '{"query": "{ users(limit: 1) { id } }"}' \
    http://localhost:8001/graphql | head -c 50
echo

echo -n "  Strawberry: "
curl -s -X POST -H "Content-Type: application/json" \
    -d '{"query": "{ users(limit: 1) { id } }"}' \
    http://localhost:8002/graphql | head -c 50
echo

# Run benchmark
echo -e "\n\n${GREEN}🏁 Running performance benchmark...${NC}"

# Set iterations
case $PROFILE in
    minimal) export BENCHMARK_ITERATIONS=10; export BENCHMARK_WARMUP=5 ;;
    small) export BENCHMARK_ITERATIONS=20; export BENCHMARK_WARMUP=5 ;;
    medium) export BENCHMARK_ITERATIONS=30; export BENCHMARK_WARMUP=10 ;;
    large) export BENCHMARK_ITERATIONS=40; export BENCHMARK_WARMUP=10 ;;
    xlarge) export BENCHMARK_ITERATIONS=50; export BENCHMARK_WARMUP=10 ;;
esac

echo -e "Iterations: ${BLUE}${BENCHMARK_ITERATIONS}${NC}, Warmup: ${BLUE}${BENCHMARK_WARMUP}${NC}"

python3 benchmark_runner.py

# Final cleanup
echo -e "\n${GREEN}✅ Benchmark complete!${NC}"
echo -e "\n${YELLOW}Press Enter to clean up containers...${NC}"
read

cleanup

echo -e "${GREEN}Done!${NC}"
