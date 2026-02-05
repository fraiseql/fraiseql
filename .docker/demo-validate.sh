#!/bin/bash

# FraiseQL Demo Stack Validation Script
# Verifies that all services in the demo stack are running and healthy

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "🔍 FraiseQL Demo Stack Validation"
echo "=================================="
echo ""

# Check if docker is installed
if ! command -v docker &> /dev/null; then
    echo -e "${RED}❌ Docker is not installed${NC}"
    exit 1
fi

# Check if docker compose is available
if ! docker compose version &> /dev/null; then
    echo -e "${RED}❌ Docker Compose is not available${NC}"
    exit 1
fi

echo -e "${GREEN}✅ Docker is installed${NC}"
echo ""

# Check if compose file exists
if [ ! -f "docker/docker-compose.demo.yml" ]; then
    echo -e "${RED}❌ docker/docker-compose.demo.yml not found${NC}"
    echo "   Run this script from the FraiseQL root directory"
    exit 1
fi

echo -e "${GREEN}✅ Demo compose file found${NC}"
echo ""

# Get service status
echo "📊 Service Status:"
docker compose -f docker/docker-compose.demo.yml ps

echo ""
echo "🧪 Health Checks:"
echo ""

# Check FraiseQL Server
echo -n "  FraiseQL Server (localhost:8000): "
if curl -s http://localhost:8000/health > /dev/null 2>&1; then
    echo -e "${GREEN}✅ Healthy${NC}"
else
    echo -e "${YELLOW}⏳ Not ready yet${NC}"
fi

# Check GraphQL IDE
echo -n "  GraphQL IDE (localhost:3000): "
if curl -s http://localhost:3000 > /dev/null 2>&1; then
    echo -e "${GREEN}✅ Healthy${NC}"
else
    echo -e "${YELLOW}⏳ Not ready yet${NC}"
fi

# Check Tutorial
echo -n "  Tutorial Server (localhost:3001): "
if curl -s http://localhost:3001/health > /dev/null 2>&1; then
    echo -e "${GREEN}✅ Healthy${NC}"
else
    echo -e "${YELLOW}⏳ Not ready yet${NC}"
fi

# Check PostgreSQL
echo -n "  PostgreSQL Database: "
if docker compose -f docker/docker-compose.demo.yml exec -T postgres-blog pg_isready -U fraiseql > /dev/null 2>&1; then
    echo -e "${GREEN}✅ Healthy${NC}"
else
    echo -e "${YELLOW}⏳ Not ready yet${NC}"
fi

echo ""

# Test GraphQL query
echo "🚀 Testing GraphQL Query:"
echo ""

QUERY='{
  "query": "{ users(limit: 1) { id name email } }"
}'

RESPONSE=$(curl -s -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d "$QUERY")

if echo "$RESPONSE" | grep -q "id"; then
    echo -e "${GREEN}✅ GraphQL query executed successfully${NC}"
    echo "   Response: $RESPONSE"
else
    echo -e "${YELLOW}⏳ GraphQL server may not be ready yet${NC}"
    echo "   Response: $RESPONSE"
fi

echo ""

# Database verification
echo "💾 Database Status:"
USERS=$(docker compose -f docker/docker-compose.demo.yml exec -T postgres-blog psql -U fraiseql -d blog_fraiseql -c "SELECT COUNT(*) FROM users;" 2>/dev/null || echo "N/A")
POSTS=$(docker compose -f docker/docker-compose.demo.yml exec -T postgres-blog psql -U fraiseql -d blog_fraiseql -c "SELECT COUNT(*) FROM posts;" 2>/dev/null || echo "N/A")

echo "  Users: $USERS"
echo "  Posts: $POSTS"

echo ""
echo "📝 Next Steps:"
echo "  1. Open GraphQL IDE: http://localhost:3000"
echo "  2. Open Tutorial: http://localhost:3001"
echo "  3. Try a query: { users(limit: 10) { id name email } }"
echo "  4. Read: docs/docker-quickstart.md"
echo ""

# Summary
if curl -s http://localhost:8000/health > /dev/null 2>&1; then
    echo -e "${GREEN}✅ Demo stack is ready!${NC}"
    exit 0
else
    echo -e "${YELLOW}⏳ Demo stack is starting, please wait 10-15 seconds${NC}"
    exit 0
fi
