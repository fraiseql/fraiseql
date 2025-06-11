#!/bin/bash
# Start PostgreSQL container in daemon mode for tests

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

CONTAINER_NAME="fraiseql-postgres"
VOLUME_NAME="fraiseql-postgres-data"
IMAGE="docker.io/library/postgres:16-alpine"

echo -e "${GREEN}Starting PostgreSQL with Podman (daemon mode)...${NC}"

# Check if Podman is installed
if ! command -v podman &> /dev/null; then
    echo -e "${RED}Error: Podman is not installed${NC}"
    exit 1
fi

# Check Podman version for pasta support
PODMAN_VERSION=$(podman --version | awk '{print $3}')
MAJOR_VERSION=$(echo $PODMAN_VERSION | cut -d. -f1)

if [ "$MAJOR_VERSION" -lt 5 ]; then
    echo -e "${YELLOW}Warning: Podman version $PODMAN_VERSION detected. Pasta networking is default in Podman 5.0+${NC}"
fi

# Stop and remove existing container if it exists
if podman ps -a --format "{{.Names}}" | grep -q "^${CONTAINER_NAME}$"; then
    echo -e "${YELLOW}Stopping existing container...${NC}"
    podman stop $CONTAINER_NAME >/dev/null 2>&1 || true
    podman rm $CONTAINER_NAME >/dev/null 2>&1 || true
fi

# Create volume if it doesn't exist
if ! podman volume exists $VOLUME_NAME; then
    echo -e "${GREEN}Creating volume $VOLUME_NAME...${NC}"
    podman volume create $VOLUME_NAME
fi

# Start PostgreSQL container with pasta networking (default in Podman 5.0+)
echo -e "${GREEN}Starting PostgreSQL container...${NC}"
podman run -d \
    --name $CONTAINER_NAME \
    --restart unless-stopped \
    -e POSTGRES_USER=fraiseql \
    -e POSTGRES_PASSWORD=fraiseql \
    -e POSTGRES_DB=fraiseql_demo \
    -e PGPORT=5433 \
    -p 5433:5433 \
    -v $VOLUME_NAME:/var/lib/postgresql/data:Z \
    -v ./examples/mutations_demo/init.sql:/docker-entrypoint-initdb.d/01-init.sql:Z,ro \
    --health-cmd "pg_isready -U fraiseql -p 5433" \
    --health-interval 5s \
    --health-timeout 5s \
    --health-retries 5 \
    $IMAGE

# Wait for PostgreSQL to be ready
echo -e "${YELLOW}Waiting for PostgreSQL to be ready...${NC}"
for i in {1..30}; do
    if podman exec $CONTAINER_NAME pg_isready -U fraiseql -p 5433 >/dev/null 2>&1; then
        echo -e "${GREEN}PostgreSQL is ready!${NC}"
        break
    fi
    echo -n "."
    sleep 1
done

# Check if PostgreSQL is running
if ! podman exec $CONTAINER_NAME pg_isready -U fraiseql -p 5433 >/dev/null 2>&1; then
    echo -e "${RED}PostgreSQL failed to start!${NC}"
    echo -e "${YELLOW}Container logs:${NC}"
    podman logs $CONTAINER_NAME
    exit 1
fi

echo -e "${GREEN}PostgreSQL is running in daemon mode!${NC}"
echo "Connection details:"
echo "  Host: localhost"
echo "  Port: 5433"
echo "  User: fraiseql"
echo "  Password: fraiseql"
echo "  Database: fraiseql_demo"
echo ""
echo -e "Connection string: ${GREEN}postgresql://fraiseql:fraiseql@localhost:5433/fraiseql_demo${NC}"
echo ""
echo "To stop the container, run: podman stop $CONTAINER_NAME"
echo "To view logs, run: podman logs $CONTAINER_NAME"
