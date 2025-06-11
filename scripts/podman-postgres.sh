#!/bin/bash
# Script to run PostgreSQL with Podman using pasta networking

set -e

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${GREEN}Starting PostgreSQL with Podman (pasta networking)...${NC}"

# Check if podman is installed
if ! command -v podman &> /dev/null; then
    echo -e "${RED}Error: Podman is not installed${NC}"
    exit 1
fi

# Check Podman version for pasta support
PODMAN_VERSION=$(podman --version | awk '{print $3}')
MAJOR_VERSION=$(echo $PODMAN_VERSION | cut -d. -f1)

if [ "$MAJOR_VERSION" -lt 5 ]; then
    echo -e "${YELLOW}Warning: Podman version $PODMAN_VERSION detected. Pasta networking requires Podman 5.0+${NC}"
    echo -e "${YELLOW}The container will still work but may use slirp4netns instead of pasta.${NC}"
fi

# Function to stop the container
stop_postgres() {
    echo -e "\n${YELLOW}Stopping PostgreSQL container...${NC}"
    podman stop fraiseql-postgres 2>/dev/null || true
    podman rm fraiseql-postgres 2>/dev/null || true
}

# Trap to ensure cleanup on script exit
trap stop_postgres EXIT

# Stop any existing container
stop_postgres

# Create volume if it doesn't exist
podman volume create fraiseql-postgres-data 2>/dev/null || true

# Run PostgreSQL with pasta networking (default in Podman 5.0+)
echo -e "${GREEN}Starting PostgreSQL container...${NC}"
podman run -d \
    --name fraiseql-postgres \
    --network pasta \
    -e POSTGRES_USER=fraiseql \
    -e POSTGRES_PASSWORD=fraiseql \
    -e POSTGRES_DB=fraiseql_demo \
    -e PGPORT=5433 \
    -p 5433:5433 \
    -v fraiseql-postgres-data:/var/lib/postgresql/data:Z \
    -v ./examples/mutations_demo/init.sql:/docker-entrypoint-initdb.d/01-init.sql:Z,ro \
    --health-cmd="pg_isready -U fraiseql" \
    --health-interval=5s \
    --health-timeout=5s \
    --health-retries=5 \
    docker.io/library/postgres:16-alpine

# Wait for container to be healthy
echo -e "${YELLOW}Waiting for PostgreSQL to be ready...${NC}"
RETRIES=30
while [ $RETRIES -gt 0 ]; do
    if podman healthcheck run fraiseql-postgres &>/dev/null; then
        echo -e "${GREEN}PostgreSQL is ready!${NC}"
        break
    fi
    echo -n "."
    sleep 1
    RETRIES=$((RETRIES-1))
done

if [ $RETRIES -eq 0 ]; then
    echo -e "\n${RED}PostgreSQL failed to start properly${NC}"
    podman logs fraiseql-postgres
    exit 1
fi

# Show connection info
echo -e "\n${GREEN}PostgreSQL is running!${NC}"
echo -e "Connection details:"
echo -e "  Host: localhost"
echo -e "  Port: 5433"
echo -e "  User: fraiseql"
echo -e "  Password: fraiseql"
echo -e "  Database: fraiseql_demo"
echo -e "\nConnection string: ${GREEN}postgresql://fraiseql:fraiseql@localhost:5433/fraiseql_demo${NC}"

# Show logs
echo -e "\n${YELLOW}Container logs:${NC}"
podman logs --tail 10 fraiseql-postgres

echo -e "\n${YELLOW}Press Ctrl+C to stop the container${NC}"

# Keep script running
wait
