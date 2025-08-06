#!/bin/bash
# FraiseQL Performance Benchmark - Unified Socket Architecture
# This benchmark uses containers with both PostgreSQL and the app running inside,
# connected via Unix socket for maximum performance measurement accuracy.

set -e

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}╔══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║       FraiseQL Performance Benchmark - Socket Edition        ║${NC}"
echo -e "${BLUE}║     PostgreSQL + App in same container via Unix Socket      ║${NC}"
echo -e "${BLUE}╚══════════════════════════════════════════════════════════════╝${NC}"
echo

# Set Podman environment variables
export DOCKER_HOST=unix:///run/user/$(id -u)/podman/podman.sock
export TESTCONTAINERS_PODMAN=true
export TESTCONTAINERS_RYUK_DISABLED=true

# Check if podman is running
if ! podman info &> /dev/null; then
    echo -e "${RED}Error: Podman is not running or not accessible${NC}"
    echo -e "${YELLOW}Please ensure the podman socket is active:${NC}"
    echo -e "  systemctl --user start podman.socket"
    exit 1
fi

echo -e "${GREEN}✓ Podman is running${NC}"

# Detect system profile
echo -e "\n${YELLOW}🔍 Detecting system capabilities...${NC}"
python3 detect_benchmark_profile.py

# Read the detected profile
if [ -f benchmark_profile.json ]; then
    PROFILE=$(python3 -c "import json; print(json.load(open('benchmark_profile.json'))['profile'])")
    USERS=$(python3 -c "import json; print(json.load(open('benchmark_profile.json'))['data_scale']['users'])")
    PRODUCTS=$(python3 -c "import json; print(json.load(open('benchmark_profile.json'))['data_scale']['products'])")
    ORDERS=$(python3 -c "import json; print(json.load(open('benchmark_profile.json'))['data_scale']['orders'])")

    echo -e "\n${GREEN}📊 Selected profile: $PROFILE${NC}"
    echo -e "   Users: ${BLUE}${USERS}${NC}"
    echo -e "   Products: ${BLUE}${PRODUCTS}${NC}"
    echo -e "   Orders: ${BLUE}${ORDERS}${NC}"

    # Export for SQL script
    export BENCHMARK_USERS=$USERS
    export BENCHMARK_PRODUCTS=$PRODUCTS
    export BENCHMARK_ORDERS=$ORDERS
else
    echo -e "${RED}Failed to detect profile, using defaults${NC}"
    export BENCHMARK_USERS=1000
    export BENCHMARK_PRODUCTS=5000
    export BENCHMARK_ORDERS=2000
fi

# Generate adaptive seed data
echo -e "\n${YELLOW}📝 Generating adaptive seed data...${NC}"
./create_adaptive_seed.sh

# Clean up any existing containers
echo -e "\n${YELLOW}🧹 Cleaning up existing containers...${NC}"
podman stop benchmark-fraiseql benchmark-strawberry 2>/dev/null || true
podman rm benchmark-fraiseql benchmark-strawberry 2>/dev/null || true

# Make scripts executable
chmod +x unified-socket/start-unified.sh

# Build containers
echo -e "\n${YELLOW}🔨 Building containers...${NC}"

echo -e "${YELLOW}Building FraiseQL unified container...${NC}"
podman build -t benchmark-fraiseql -f unified-socket/Dockerfile.fraiseql . || {
    echo -e "${RED}Failed to build FraiseQL container${NC}"
    exit 1
}

echo -e "${YELLOW}Building Strawberry unified container...${NC}"
podman build -t benchmark-strawberry -f unified-socket/Dockerfile.strawberry . || {
    echo -e "${RED}Failed to build Strawberry container${NC}"
    exit 1
}

# Start containers
echo -e "\n${YELLOW}🚀 Starting containers...${NC}"

echo -e "${YELLOW}Starting FraiseQL container...${NC}"
podman run -d \
    --name benchmark-fraiseql \
    -p 8001:8000 \
    -e BENCHMARK_USERS=$BENCHMARK_USERS \
    -e BENCHMARK_PRODUCTS=$BENCHMARK_PRODUCTS \
    -e BENCHMARK_ORDERS=$BENCHMARK_ORDERS \
    --tmpfs /tmp:rw,noexec,nosuid,size=1g \
    benchmark-fraiseql

echo -e "${YELLOW}Starting Strawberry container...${NC}"
podman run -d \
    --name benchmark-strawberry \
    -p 8002:8000 \
    -e BENCHMARK_USERS=$BENCHMARK_USERS \
    -e BENCHMARK_PRODUCTS=$BENCHMARK_PRODUCTS \
    -e BENCHMARK_ORDERS=$BENCHMARK_ORDERS \
    --tmpfs /tmp:rw,noexec,nosuid,size=1g \
    benchmark-strawberry

# Wait for containers to be ready
echo -e "\n${YELLOW}⏳ Waiting for containers to initialize...${NC}"
echo -e "This may take a few minutes as PostgreSQL initializes and data is generated..."

# Function to check if service is ready
check_service_ready() {
    local service_name=$1
    local port=$2
    local container_name=$3
    local max_attempts=180  # 3 minutes
    local attempt=0

    echo -e "\n${YELLOW}Waiting for $service_name...${NC}"

    while [ $attempt -lt $max_attempts ]; do
        if curl -s "http://localhost:$port/health" &> /dev/null; then
            response=$(curl -s "http://localhost:$port/health")
            if [[ "$response" == *"healthy"* ]]; then
                echo -e "${GREEN}✓ $service_name is ready${NC}"
                return 0
            fi
        fi

        if [ $((attempt % 20)) -eq 0 ] && [ $attempt -gt 0 ]; then
            echo -e "${YELLOW}Still waiting... (${attempt}s)${NC}"
            # Show last few log lines
            echo -e "${BLUE}Recent logs:${NC}"
            podman logs --tail 5 $container_name 2>&1 | sed 's/^/  /'
        else
            echo -n "."
        fi

        sleep 1
        attempt=$((attempt + 1))
    done

    echo -e "\n${RED}✗ $service_name failed to become ready${NC}"
    echo -e "${YELLOW}Full logs:${NC}"
    podman logs --tail 50 $container_name
    return 1
}

# Wait for both services
check_service_ready "FraiseQL" 8001 "benchmark-fraiseql" || exit 1
check_service_ready "Strawberry" 8002 "benchmark-strawberry" || exit 1

echo -e "\n${GREEN}✅ All services are ready!${NC}"

# Run the benchmark
echo -e "\n${YELLOW}📊 Running performance benchmark...${NC}"

# Adjust iterations based on profile
case $PROFILE in
    minimal)
        export BENCHMARK_ITERATIONS=10
        export BENCHMARK_WARMUP=5
        ;;
    small)
        export BENCHMARK_ITERATIONS=20
        export BENCHMARK_WARMUP=5
        ;;
    medium)
        export BENCHMARK_ITERATIONS=30
        export BENCHMARK_WARMUP=10
        ;;
    large)
        export BENCHMARK_ITERATIONS=40
        export BENCHMARK_WARMUP=10
        ;;
    xlarge)
        export BENCHMARK_ITERATIONS=50
        export BENCHMARK_WARMUP=10
        ;;
esac

echo -e "Profile: ${BLUE}$PROFILE${NC}"
echo -e "Iterations: ${BLUE}${BENCHMARK_ITERATIONS}${NC}, Warmup: ${BLUE}${BENCHMARK_WARMUP}${NC}"

# Run the benchmark test
python3 benchmark_runner.py

# Show summary
echo -e "\n${GREEN}✅ Benchmark complete!${NC}"
echo -e "\nResults are saved in the current directory with timestamp."

# Cleanup prompt
echo -e "\n${YELLOW}Would you like to stop and remove the containers? (y/n)${NC}"
read -p "> " cleanup_answer

if [[ "$cleanup_answer" == "y" ]] || [[ "$cleanup_answer" == "Y" ]]; then
    echo -e "${YELLOW}Cleaning up...${NC}"
    podman stop benchmark-fraiseql benchmark-strawberry 2>/dev/null || true
    podman rm benchmark-fraiseql benchmark-strawberry 2>/dev/null || true
    echo -e "${GREEN}✓ Cleanup complete${NC}"
else
    echo -e "${YELLOW}Containers are still running:${NC}"
    echo -e "  FraiseQL: http://localhost:8001"
    echo -e "  Strawberry: http://localhost:8002"
    echo -e "\nTo stop them later, run:"
    echo -e "  podman stop benchmark-fraiseql benchmark-strawberry"
    echo -e "  podman rm benchmark-fraiseql benchmark-strawberry"
fi
