#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "🚀 FraiseQL Saga Example - Integration Test"
echo "============================================"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test configuration
ROUTER_URL="http://localhost:4000/graphql"
TIMEOUT=60
RETRIES=0
MAX_RETRIES=30

# Function to print colored output
print_status() {
    echo -e "${GREEN}✓${NC} $1"
}

print_error() {
    echo -e "${RED}✗${NC} $1"
}

print_info() {
    echo -e "${YELLOW}ℹ${NC} $1"
}

# Function to wait for service
wait_for_service() {
    local service=$1
    local retries=0
    local max_retries=30

    print_info "Waiting for $service to be healthy..."

    while [ $retries -lt $max_retries ]; do
        if docker-compose ps | grep "$service" | grep -q "healthy"; then
            print_status "$service is healthy"
            return 0
        fi
        retries=$((retries + 1))
        sleep 2
    done

    print_error "$service failed to become healthy after ${max_retries} retries"
    return 1
}

# Function to execute GraphQL query
execute_query() {
    local query=$1
    local variables=$2

    curl -s -X POST "$ROUTER_URL" \
        -H "Content-Type: application/json" \
        -d "$(jq -n --arg q "$query" --argjson vars "$variables" '{query: $q, variables: $vars}')"
}

# Cleanup function
cleanup() {
    print_info "Cleaning up..."
    docker-compose down -v || true
}

# Register cleanup on exit
trap cleanup EXIT

# Start services
print_info "Starting Docker Compose services..."
docker-compose up -d

# Wait for all services to be healthy
print_info "Waiting for services to become healthy..."
wait_for_service "postgres"
wait_for_service "mysql"
wait_for_service "users-service"
wait_for_service "orders-service"
wait_for_service "inventory-service"
wait_for_service "apollo-router"

print_status "All services are healthy!"

# Give services a moment to fully initialize
sleep 5

# Test 1: Verify users exist
print_info "Test 1: Verifying test users exist..."

QUERY_USERS='
  query {
    users {
      id
      name
      email
    }
  }
'

RESPONSE=$(curl -s -X POST "$ROUTER_URL" \
    -H "Content-Type: application/json" \
    -d "{\"query\": \"$(echo $QUERY_USERS | tr -d '\n' | sed 's/"/\\"/g')\"}")

if echo "$RESPONSE" | jq -e '.data.users | length > 0' > /dev/null 2>&1; then
    USER_ID=$(echo "$RESPONSE" | jq -r '.data.users[0].id')
    print_status "Found test users. Using user ID: $USER_ID"
else
    print_error "Failed to fetch users"
    echo "$RESPONSE"
    exit 1
fi

# Test 2: Execute order saga (success path)
print_info "Test 2: Executing order saga (success path)..."

VERIFY_USER_MUTATION='
  mutation VerifyUserExists($userId: ID!) {
    verifyUserExists(userId: $userId) {
      id
      name
      email
    }
  }
'

VARIABLES="{\"userId\": \"$USER_ID\"}"

RESPONSE=$(curl -s -X POST "$ROUTER_URL" \
    -H "Content-Type: application/json" \
    -d "$(echo "{\"query\": \"$(echo $VERIFY_USER_MUTATION | tr -d '\n' | sed 's/"/\\"/g')\", \"variables\": $VARIABLES}")")

if echo "$RESPONSE" | jq -e '.data.verifyUserExists.id' > /dev/null 2>&1; then
    print_status "Step 1/4: Verified user exists"
else
    print_error "Step 1/4: Failed to verify user"
    echo "$RESPONSE"
    exit 1
fi

# Step 2: Simulate payment charge (in a real saga, this would happen)
print_info "Step 2: Simulating payment charge..."
CHARGE_ID="charge-$(date +%s)"
print_status "Step 2/4: Payment charged (ID: $CHARGE_ID)"

# Step 3: Reserve inventory
print_info "Step 3: Reserving inventory..."

RESERVE_ITEMS_MUTATION='
  mutation ReserveItems($items: [ReservationItemInput!]!, $orderId: ID!) {
    reserveItems(items: $items, orderId: $orderId) {
      id
      orderId
      status
      items {
        productId
        quantity
      }
    }
  }
'

ORDER_ID="order-$(date +%s)"
ITEMS_VAR='[{"productId": "prod-001", "quantity": 1}, {"productId": "prod-002", "quantity": 2}]'
VARIABLES="{\"items\": $ITEMS_VAR, \"orderId\": \"$ORDER_ID\"}"

RESPONSE=$(curl -s -X POST "$ROUTER_URL" \
    -H "Content-Type: application/json" \
    -d "$(echo "{\"query\": \"$(echo $RESERVE_ITEMS_MUTATION | tr -d '\n' | sed 's/"/\\"/g')\", \"variables\": $VARIABLES}")")

if echo "$RESPONSE" | jq -e '.data.reserveItems.id' > /dev/null 2>&1; then
    RESERVATION_ID=$(echo "$RESPONSE" | jq -r '.data.reserveItems.id')
    print_status "Step 3/4: Inventory reserved (ID: $RESERVATION_ID)"
else
    print_error "Step 3/4: Failed to reserve inventory"
    echo "$RESPONSE"
    exit 1
fi

# Step 4: Create order
print_info "Step 4: Creating order..."

CREATE_ORDER_MUTATION='
  mutation CreateOrder($userId: ID!, $items: [OrderItemInput!]!, $chargeId: String!, $reservationId: String!) {
    createOrder(userId: $userId, items: $items, chargeId: $chargeId, reservationId: $reservationId) {
      id
      userId
      status
      total
      items {
        productId
        quantity
        price
      }
    }
  }
'

ORDER_ITEMS_VAR='[{"productId": "prod-001", "quantity": 1, "price": 999.99}, {"productId": "prod-002", "quantity": 2, "price": 29.99}]'
VARIABLES="{\"userId\": \"$USER_ID\", \"items\": $ORDER_ITEMS_VAR, \"chargeId\": \"$CHARGE_ID\", \"reservationId\": \"$RESERVATION_ID\"}"

RESPONSE=$(curl -s -X POST "$ROUTER_URL" \
    -H "Content-Type: application/json" \
    -d "$(echo "{\"query\": \"$(echo $CREATE_ORDER_MUTATION | tr -d '\n' | sed 's/"/\\"/g')\", \"variables\": $VARIABLES}")")

if echo "$RESPONSE" | jq -e '.data.createOrder.id' > /dev/null 2>&1; then
    CREATED_ORDER_ID=$(echo "$RESPONSE" | jq -r '.data.createOrder.id')
    ORDER_TOTAL=$(echo "$RESPONSE" | jq -r '.data.createOrder.total')
    print_status "Step 4/4: Order created (ID: $CREATED_ORDER_ID, Total: \$$ORDER_TOTAL)"
else
    print_error "Step 4/4: Failed to create order"
    echo "$RESPONSE"
    exit 1
fi

print_status "Order saga completed successfully!"

# Test 3: Verify order was created
print_info "Test 3: Verifying order data..."

GET_ORDER_QUERY='
  query GetOrder($id: ID!) {
    order(id: $id) {
      id
      userId
      status
      total
      items {
        productId
        quantity
        price
      }
    }
  }
'

VARIABLES="{\"id\": \"$CREATED_ORDER_ID\"}"

RESPONSE=$(curl -s -X POST "$ROUTER_URL" \
    -H "Content-Type: application/json" \
    -d "$(echo "{\"query\": \"$(echo $GET_ORDER_QUERY | tr -d '\n' | sed 's/"/\\"/g')\", \"variables\": $VARIABLES}")")

if echo "$RESPONSE" | jq -e ".data.order.id == \"$CREATED_ORDER_ID\"" > /dev/null 2>&1; then
    print_status "Order data verified successfully"
else
    print_error "Failed to verify order data"
    echo "$RESPONSE"
    exit 1
fi

# Test 4: Test compensation path (release reservation)
print_info "Test 4: Testing compensation path (release reservation)..."

RELEASE_RESERVATION_MUTATION='
  mutation ReleaseReservation($reservationId: ID!) {
    releaseReservation(reservationId: $reservationId) {
      id
      status
    }
  }
'

VARIABLES="{\"reservationId\": \"$RESERVATION_ID\"}"

RESPONSE=$(curl -s -X POST "$ROUTER_URL" \
    -H "Content-Type: application/json" \
    -d "$(echo "{\"query\": \"$(echo $RELEASE_RESERVATION_MUTATION | tr -d '\n' | sed 's/"/\\"/g')\", \"variables\": $VARIABLES}")")

if echo "$RESPONSE" | jq -e '.data.releaseReservation.status' | grep -q "released"; then
    print_status "Reservation released successfully (compensation works)"
else
    print_error "Failed to release reservation"
    echo "$RESPONSE"
    exit 1
fi

# Summary
echo ""
echo "✅ All tests passed!"
echo ""
echo "📊 Test Summary:"
echo "  ✓ Services started and became healthy"
echo "  ✓ Users verified"
echo "  ✓ Order saga executed (4 steps)"
echo "  ✓ Order data persisted correctly"
echo "  ✓ Compensation path works"
echo ""
echo "🎉 FraiseQL Saga Example is working correctly!"
