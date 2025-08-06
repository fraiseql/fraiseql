#!/bin/bash
# Simple benchmark runner with separate containers

set -e

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}FraiseQL Performance Benchmark${NC}"
echo -e "${BLUE}==============================${NC}"

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

# Start PostgreSQL
echo -e "\n${YELLOW}Starting PostgreSQL...${NC}"
podman run -d \
    --name bench-postgres \
    -e POSTGRES_USER=benchmark \
    -e POSTGRES_PASSWORD=benchmark \
    -e POSTGRES_DB=benchmark_db \
    -p 5433:5432 \
    postgres:15-alpine

# Wait for PostgreSQL
echo -e "${YELLOW}Waiting for PostgreSQL...${NC}"
sleep 10

# Initialize database
echo -e "\n${YELLOW}Initializing database...${NC}"
podman exec bench-postgres psql -U benchmark -d benchmark_db -f - < shared/database/schema.sql
podman exec bench-postgres psql -U benchmark -d benchmark_db -f - < shared/database/fraiseql-views.sql
podman exec bench-postgres psql -U benchmark -d benchmark_db -f - < /tmp/seed-data-generated.sql

# Build and run FraiseQL
echo -e "\n${YELLOW}Starting FraiseQL...${NC}"
cd fraiseql
podman build -t fraiseql-bench -f- . << 'EOF'
FROM python:3.11-slim
WORKDIR /app
RUN apt-get update && apt-get install -y gcc postgresql-client && rm -rf /var/lib/apt/lists/*
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt
COPY . .
CMD ["uvicorn", "benchmark_app:app", "--host", "0.0.0.0", "--port", "8000"]
EOF

podman run -d \
    --name bench-fraiseql \
    -e DATABASE_URL=postgresql://benchmark:benchmark@host.containers.internal:5433/benchmark_db \
    -p 8001:8000 \
    fraiseql-bench

cd ..

# Build and run Strawberry
echo -e "\n${YELLOW}Starting Strawberry...${NC}"
cd strawberry-sqlalchemy
podman build -t strawberry-bench -f- . << 'EOF'
FROM python:3.11-slim
WORKDIR /app
RUN apt-get update && apt-get install -y gcc && rm -rf /var/lib/apt/lists/*
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt
COPY . .
CMD ["uvicorn", "app:app", "--host", "0.0.0.0", "--port", "8000", "--workers", "4"]
EOF

podman run -d \
    --name bench-strawberry \
    -e DATABASE_URL=postgresql://benchmark:benchmark@host.containers.internal:5433/benchmark_db \
    -p 8002:8000 \
    strawberry-bench

cd ..

# Wait for services
echo -e "\n${YELLOW}Waiting for services...${NC}"
sleep 20

# Check health
echo -e "\n${YELLOW}Checking services...${NC}"
curl -s http://localhost:8001/health || echo "FraiseQL not ready"
curl -s http://localhost:8002/health || echo "Strawberry not ready"

# Run benchmark
echo -e "\n${GREEN}Running benchmark...${NC}"

# Set iterations based on profile
case $PROFILE in
    minimal) export BENCHMARK_ITERATIONS=10; export BENCHMARK_WARMUP=5 ;;
    small) export BENCHMARK_ITERATIONS=20; export BENCHMARK_WARMUP=5 ;;
    medium) export BENCHMARK_ITERATIONS=30; export BENCHMARK_WARMUP=10 ;;
    large) export BENCHMARK_ITERATIONS=40; export BENCHMARK_WARMUP=10 ;;
    xlarge) export BENCHMARK_ITERATIONS=50; export BENCHMARK_WARMUP=10 ;;
esac

python3 benchmark_runner.py

# Cleanup
echo -e "\n${YELLOW}Cleaning up...${NC}"
podman stop bench-postgres bench-fraiseql bench-strawberry 2>/dev/null || true
podman rm bench-postgres bench-fraiseql bench-strawberry 2>/dev/null || true

echo -e "\n${GREEN}Benchmark complete!${NC}"
