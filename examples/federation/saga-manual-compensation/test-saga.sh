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

print_status() { echo -e "${GREEN}✓${NC} $1"; }
print_error() { echo -e "${RED}✗${NC} $1"; }
print_info() { echo -e "${YELLOW}ℹ${NC} $1"; }

cleanup() {
    print_info "Cleaning up..."
    docker-compose down -v || true
}

trap cleanup EXIT

# Start services
print_info "Starting services..."
docker-compose up -d

# Wait for health
print_info "Waiting for services to become healthy..."
for i in {1..30}; do
    if docker-compose ps | grep -q "bank-service.*healthy"; then
        print_status "Services are healthy"
        break
    fi
    sleep 2
done

sleep 3

# Test 1: Get initial account balances
print_info "Test 1: Getting account balances..."
RESPONSE=$(curl -s -X POST http://localhost:4005/graphql \
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
RESPONSE=$(curl -s -X POST http://localhost:4005/graphql \
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
RESPONSE=$(curl -s -X POST http://localhost:4005/graphql \
    -H "Content-Type: application/json" \
    -d "{\"query\":\"mutation{transferMoney(fromAccountId:\\\"acc-001\\\" toAccountId:\\\"acc-002\\\" amount:100 transactionId:\\\"$TXN_ID\\\"){transactionId status}}\"}")

if echo "$RESPONSE" | jq -e '.data.transferMoney.message' | grep -q "already processed"; then
    print_status "Idempotency works - duplicate request returned cached result"
else
    print_error "Idempotency failed"
    exit 1
fi

# Test 4: Insufficient funds
print_info "Test 4: Testing failure path (insufficient funds)..."
TXN_ID2="txn-fail-$(date +%s)"
RESPONSE=$(curl -s -X POST http://localhost:4005/graphql \
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
curl -s -X POST http://localhost:4005/graphql \
    -H "Content-Type: application/json" \
    -d "{\"query\":\"mutation{transferMoney(fromAccountId:\\\"acc-001\\\" toAccountId:\\\"acc-003\\\" amount:50 transactionId:\\\"$TXN_ID3\\\"){status}}\"}" > /dev/null

# Compensate it
RESPONSE=$(curl -s -X POST http://localhost:4005/graphql \
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
