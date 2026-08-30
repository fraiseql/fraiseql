#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "🏦 FraiseQL Saga Example - Manual Compensation (Banking Transfer)"
echo "=================================================================="

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

# The committed compose publishes the router on 4005 and PostgreSQL on 5433 --
# and 5433 is a common choice for a second local PostgreSQL. A machine that
# already uses either can only run this stack behind a port override, at which
# point a test that hardcodes its endpoint cannot be pointed at it. Pair
# ROUTER_URL with COMPOSE_FILE to add an override file:
#
#   COMPOSE_FILE=docker-compose.yml:/tmp/ovr.yml \
#   ROUTER_URL=http://localhost:14005/graphql ./test-saga.sh
ROUTER_URL="${ROUTER_URL:-http://localhost:4005/graphql}"

print_status() { echo -e "${GREEN}✓${NC} $1"; }
print_error() { echo -e "${RED}✗${NC} $1"; }
print_info() { echo -e "${YELLOW}ℹ${NC} $1"; }

cleanup() {
    print_info "Cleaning up..."
    docker compose down -v || true
}

trap cleanup EXIT

# Start services
# --build, not a bare `up -d`: compose only builds when the image is absent, so
# after an edit to server.py a plain `up -d` silently retests the previous image.
print_info "Starting services..."
docker compose up -d --build

# Wait for health.
#
# Read the container's health state rather than grepping the `ps` table:
# `grep -q "bank-service.*healthy"` matched `Up (unhealthy)` too — `.*` absorbs the
# `(un` — so this loop announced healthy services that had never come up (#1073).
# The loop also has to FAIL when the wait runs out; it used to fall through to the
# first test, which then failed somewhere unrelated.
print_info "Waiting for services to become healthy..."
bank_healthy=0
for _ in $(seq 1 30); do
    cid=$(docker compose ps -q bank-service 2>/dev/null || true)
    if [ -n "$cid" ] && [ "$(docker inspect \
            -f '{{if .State.Health}}{{.State.Health.Status}}{{else}}no-healthcheck{{end}}' \
            "$cid" 2>/dev/null)" = "healthy" ]; then
        print_status "Services are healthy"
        bank_healthy=1
        break
    fi
    sleep 2
done
if [ "$bank_healthy" -ne 1 ]; then
    print_error "bank-service did not become healthy after 60s"
    docker compose ps
    exit 1
fi

# Wait for the router by asking it the question the tests are about to ask.
#
# The router declares no healthcheck, and bank-service being healthy says nothing
# about it. Without this the script slept 3 seconds and went straight to Test 1,
# so a router that had not finished starting -- or had exited on a bad config,
# which is exactly what it did until #1259 -- was reported as "Failed to get
# account" rather than as a router that never came up.
print_info "Waiting for the router to answer at $ROUTER_URL..."
router_up=0
for _ in $(seq 1 30); do
    if curl -sf -X POST "$ROUTER_URL" \
        -H "Content-Type: application/json" \
        -d '{"query":"{__typename}"}' > /dev/null 2>&1; then
        print_status "router is answering"
        router_up=1
        break
    fi
    sleep 2
done
if [ "$router_up" -ne 1 ]; then
    print_error "router did not answer at $ROUTER_URL after 60s"
    docker compose logs apollo-router | tail -20
    exit 1
fi

# Test 1: Get initial account balances
print_info "Test 1: Getting account balances..."
RESPONSE=$(curl -s -X POST "$ROUTER_URL" \
    -H "Content-Type: application/json" \
    -d '{"query":"query{account(accountId:\"acc-001\"){id accountHolder balance}}"}')

if echo "$RESPONSE" | jq -e '.data.account.balance' > /dev/null; then
    BALANCE=$(echo "$RESPONSE" | jq -r '.data.account.balance')
    print_status "Account ACC-001 balance: \$$BALANCE"
else
    print_error "Failed to get account"
    exit 1
fi

# Test 2: Successful transfer
print_info "Test 2: Executing successful transfer (ACC-001 -> ACC-002, \$100)..."
TXN_ID="txn-$(date +%s)"
RESPONSE=$(curl -s -X POST "$ROUTER_URL" \
    -H "Content-Type: application/json" \
    -d "{\"query\":\"mutation{transferMoney(fromAccountId:\\\"acc-001\\\" toAccountId:\\\"acc-002\\\" amount:100 transactionId:\\\"$TXN_ID\\\"){transactionId status fromBalance toBalance}}\"}")

if echo "$RESPONSE" | jq -e '.data.transferMoney.status' | grep -q "completed"; then
    FROM_BALANCE=$(echo "$RESPONSE" | jq -r '.data.transferMoney.fromBalance')
    TO_BALANCE=$(echo "$RESPONSE" | jq -r '.data.transferMoney.toBalance')
    print_status "Transfer successful - FROM: \$$FROM_BALANCE, TO: \$$TO_BALANCE"
else
    print_error "Transfer failed"
    echo "$RESPONSE"
    exit 1
fi

# Test 3: Idempotent retry
print_info "Test 3: Testing idempotency (retry same transfer)..."
RESPONSE=$(curl -s -X POST "$ROUTER_URL" \
    -H "Content-Type: application/json" \
    -d "{\"query\":\"mutation{transferMoney(fromAccountId:\\\"acc-001\\\" toAccountId:\\\"acc-002\\\" amount:100 transactionId:\\\"$TXN_ID\\\"){transactionId status message}}\"}")

if echo "$RESPONSE" | jq -e '.data.transferMoney.message' | grep -q "already processed"; then
    print_status "Idempotency works - duplicate request returned cached result"
else
    print_error "Idempotency failed"
    exit 1
fi

# Test 4: Insufficient funds
print_info "Test 4: Testing failure path (insufficient funds)..."
TXN_ID2="txn-fail-$(date +%s)"
RESPONSE=$(curl -s -X POST "$ROUTER_URL" \
    -H "Content-Type: application/json" \
    -d "{\"query\":\"mutation{transferMoney(fromAccountId:\\\"acc-002\\\" toAccountId:\\\"acc-001\\\" amount:10000 transactionId:\\\"$TXN_ID2\\\"){status}}\"}")

if echo "$RESPONSE" | jq -e '.errors' > /dev/null 2>&1; then
    print_status "Insufficient funds error caught correctly"
else
    print_error "Should have failed on insufficient funds"
    exit 1
fi

# Test 5: Manual compensation
print_info "Test 5: Testing manual compensation..."
TXN_ID3="txn-compensate-$(date +%s)"
# Create a transfer
curl -s -X POST "$ROUTER_URL" \
    -H "Content-Type: application/json" \
    -d "{\"query\":\"mutation{transferMoney(fromAccountId:\\\"acc-001\\\" toAccountId:\\\"acc-003\\\" amount:50 transactionId:\\\"$TXN_ID3\\\"){status}}\"}" > /dev/null

# Compensate it
RESPONSE=$(curl -s -X POST "$ROUTER_URL" \
    -H "Content-Type: application/json" \
    -d "{\"query\":\"mutation{compensateTransfer(transactionId:\\\"$TXN_ID3\\\"){status}}\"}")

if echo "$RESPONSE" | jq -e '.data.compensateTransfer.status' | grep -q "compensated"; then
    print_status "Manual compensation works - funds returned"
else
    print_error "Compensation failed"
    exit 1
fi

echo ""
echo "✅ All tests passed!"
echo ""
echo "📊 Test Summary:"
echo "  ✓ Account queries work"
echo "  ✓ Successful transfers execute"
echo "  ✓ Idempotency prevents duplicate transfers"
echo "  ✓ Error handling for insufficient funds"
echo "  ✓ Manual compensation logic works"
echo ""
echo "🎉 Manual Compensation Saga Example is working!"
