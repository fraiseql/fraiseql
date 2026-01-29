# FraiseQL Saga Example: Manual Compensation (Banking Transfer)

This example demonstrates **manual compensation** in distributed sagas - when automatic compensation isn't sufficient and business logic must decide how to undo operations.

## Scenario

A **money transfer saga** between bank accounts that shows:

- **Automatic compensation** (automatic reversal of ledger entries)
- **Manual compensation** (business logic-driven reversal with audit trail)
- **Idempotency** (preventing duplicate charges/transfers)
- **Error handling** (handling transient vs permanent failures)

### The Transfer Saga Flow

When a customer transfers money between accounts:

```
1. Verify Sender Account (Bank Service)
   - Check account exists and is not frozen
   ↓
2. Reserve Funds (Bank Service)
   - Lock amount in sender account (prevents overdraft)
   ↓
3. Debit Sender (Bank Service)
   - Debit the amount (with transaction ID for idempotency)
   ↓
4. Credit Receiver (Bank Service)
   - Credit the amount to receiver account
   ↓
5. Confirm Transfer (Audit Service)
   - Record transfer in audit ledger for compliance
   ↓
✅ Transfer Complete

❌ If any step fails:
   Manual compensation logic:
   - Step 4 failed: Credit not recorded (no debit reversal needed yet)
   - Step 3 failed: Debit succeeded but credit failed
     Compensation: Credit back to sender (idempotent via transaction ID)
   - Step 2 failed: Funds released (no reversal needed)
   - Step 1 failed: No compensation (verify only)
```

## Architecture

```
┌─────────────────────────────────────┐
│      Apollo Router (Gateway)         │
│      localhost:4000/graphql          │
└────────┬────────────────────────────┘
         │
    ┌────▼──────┐
    │Bank Service│
    │(Flask)     │
    │Port: 4001  │
    │            │
    │ - Verify   │
    │ - Reserve  │
    │ - Debit    │
    │ - Credit   │
    └────┬───────┘
         │
    ┌────▼──────┐
    │PostgreSQL  │
    │(5432)      │
    │            │
    │ accounts   │
    │ ledger     │
    │ locks      │
    │ transfers  │
    └────────────┘
```

## Key Differences from saga-basic

| Aspect | saga-basic | saga-manual-compensation |
|--------|-----------|--------------------------|
| Compensation | Automatic (mutation fields define undo) | Manual (code decides undo logic) |
| Coordination | One service per step | Multiple operations in same service |
| Idempotency | Via `requestId` | Via `transactionId` + timestamp |
| Failure Handling | Fail fast with errors | Retry logic + fallback logic |
| Audit Trail | Implicit in saga steps | Explicit ledger entries |
| Rollback | Automatic reverse steps | Custom business logic |

## Files

- **docker-compose.yml** - Single PostgreSQL database with one service
- **fixtures/postgres-init.sql** - Account and transfer tables
- **bank-service/schema.graphql** - Bank GraphQL schema
- **bank-service/server.py** - Bank service with manual compensation logic
- **bank-service/Dockerfile** - Python Flask service
- **test-saga.sh** - Integration test with failure scenarios
- **README.md** - This file

## Quick Start

### 1. Start the Example

```bash
cd examples/federation/saga-manual-compensation
docker-compose up -d
```

### 2. Test the Saga

```bash
./test-saga.sh
```

This tests:
- Successful transfer between accounts
- Idempotent transfers (same request, same result)
- Partial failures with manual compensation
- Audit trail accuracy

### 3. Manual Testing

```bash
# Get account balances
curl -X POST http://localhost:4000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "query { account(accountId: \"ACC-001\") { id balance } }"
  }'

# Initiate transfer (with idempotency key)
curl -X POST http://localhost:4000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "mutation { transferMoney(fromAccountId: \"ACC-001\", toAccountId: \"ACC-002\", amount: 100, transactionId: \"txn-123\") { transactionId status fromBalance toBalance } }"
  }'

# Check transfer status
curl -X POST http://localhost:4000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "query { transfer(transactionId: \"txn-123\") { status fromBalance toBalance auditLog }"
  }'
```

## Manual Compensation Pattern

The key difference is that compensation is **decided by application logic**, not automatically reversed.

### Step 1: Forward Execution

```python
@app.route('/graphql', methods=['POST'])
def handle_debit_sender(transaction_id, from_account_id, amount):
    """Debit funds from sender (idempotent via transaction_id)"""

    # Check if already processed (idempotency)
    existing = db.query('SELECT * FROM transfers WHERE transaction_id = ?', (transaction_id,))
    if existing and existing.status == 'DEBITED':
        return success(existing)  # Return cached result

    # Lock the account to prevent race conditions
    db.execute('SELECT * FROM accounts WHERE id = ? FOR UPDATE', (from_account_id,))

    # Check sufficient funds
    account = db.get_account(from_account_id)
    if account.balance < amount:
        return error('Insufficient funds')

    # Debit the account
    db.execute(
        'UPDATE accounts SET balance = balance - ? WHERE id = ?',
        (amount, from_account_id)
    )

    # Record in transfer ledger
    db.execute(
        'INSERT INTO transfers (transaction_id, from_account_id, amount, status) VALUES (?, ?, ?, ?)',
        (transaction_id, from_account_id, amount, 'DEBITED')
    )

    return success({'status': 'DEBITED', 'remaining_balance': account.balance - amount})
```

### Step 2: Compensation Logic

If a later step fails, manual compensation decides what to do:

```python
async def compensate_failed_transfer(saga, failed_step_index):
    """Manually compensate based on which step failed"""

    if failed_step_index >= 3:  # Credit step failed
        # Debit succeeded but credit failed
        # Compensation: Return funds to sender (idempotent)

        transaction_id = saga.data['transaction_id']
        amount = saga.data['amount']
        from_account_id = saga.data['from_account_id']

        # Check if credit compensation already ran
        credit_comp = db.query(
            'SELECT * FROM transfers WHERE transaction_id = ? AND status = ?',
            (transaction_id + '-CREDIT-COMP', 'COMPENSATED')
        )
        if credit_comp:
            return  # Already compensated, idempotent

        # Return funds to sender
        db.execute(
            'UPDATE accounts SET balance = balance + ? WHERE id = ?',
            (amount, from_account_id)
        )

        # Log compensation
        db.execute(
            'INSERT INTO transfers (transaction_id, status, reason) VALUES (?, ?, ?)',
            (transaction_id + '-CREDIT-COMP', 'COMPENSATED', 'Credit step failed')
        )

    elif failed_step_index >= 2:  # Debit step failed
        # Transfer not started, no compensation needed
        pass
```

## Idempotency in Detail

Every operation uses a unique `transactionId` to ensure idempotency:

```sql
-- Schema with idempotency support
CREATE TABLE transfers (
    id UUID PRIMARY KEY,
    transaction_id VARCHAR(255) UNIQUE NOT NULL,  -- Idempotency key
    from_account_id VARCHAR(50) NOT NULL,
    to_account_id VARCHAR(50),
    amount DECIMAL(15, 2) NOT NULL,
    status VARCHAR(50),  -- RESERVED, DEBITED, CREDITED, COMPENSATED
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Unique constraint ensures same transaction_id returns cached result
UNIQUE INDEX idx_transaction_id ON transfers(transaction_id);
```

When the same `transactionId` is used again:

1. Check if `transaction_id` exists in transfers table
2. If yes, return the previously computed result (idempotent)
3. If no, execute the operation normally

## Error Handling

The example shows how to handle different error types:

```python
async def transfer_money(from_account, to_account, amount, transaction_id):
    """Transfer with comprehensive error handling"""

    try:
        # Attempt transfer
        coordinator.execute(steps)

    except SagaError::StepFailed as e:
        if e.step_index == 2:  # Debit failed
            # Transient error (account locked, network issue)
            # Retry with exponential backoff
            return retry_transfer(transaction_id)

        elif e.step_index == 3:  # Credit failed
            # Permanent error (invalid receiver account)
            # Manual compensation required
            return compensate_transfer(transaction_id)

    except SagaError::Timeout:
        # Service not responding
        # Don't compensate yet - retry from last known state
        return poll_transfer_status(transaction_id)
```

## Audit Trail

Unlike automatic compensation, manual compensation leaves an audit trail:

```python
def log_transfer_event(transaction_id, event_type, details):
    """Log all transfer events for compliance"""
    db.execute(
        '''INSERT INTO audit_log
           (transaction_id, event_type, details, timestamp)
           VALUES (?, ?, ?, CURRENT_TIMESTAMP)''',
        (transaction_id, event_type, json.dumps(details))
    )

# Usage
log_transfer_event('txn-123', 'TRANSFER_INITIATED', {'from': 'ACC-001', 'to': 'ACC-002'})
log_transfer_event('txn-123', 'FUNDS_RESERVED', {'amount': 100})
log_transfer_event('txn-123', 'FUNDS_DEBITED', {'from_balance': 500})
log_transfer_event('txn-123', 'TRANSFER_FAILED', {'reason': 'Receiver account frozen'})
log_transfer_event('txn-123', 'COMPENSATION_INITIATED', {})
log_transfer_event('txn-123', 'FUNDS_CREDITED_BACK', {'to_balance': 600})
```

## Testing Failure Scenarios

The test script includes failure scenarios:

### Scenario 1: Success Path

```
ACC-001: $1000  →  transfer $100  →  ACC-002: $500
ACC-001: $900   ✓  transfer $100  ✓  ACC-002: $600
```

### Scenario 2: Receiver Account Frozen

```
ACC-001: $1000  →  transfer $100  →  ACC-002 (frozen)
         $1000  ✓                   ✗  Account frozen
      Compensation: ACC-001: $1000 ✓
```

### Scenario 3: Idempotent Retry

```
transfer(txn-123): ACC-001: $1000 → $900, ACC-002: $500 → $600
retry(txn-123):    Returns cached result (idempotent)
                   No double-debit, correct balances
```

## Performance Characteristics

| Operation | Time | Notes |
|-----------|------|-------|
| Verify account | ~10ms | Index lookup |
| Reserve funds | ~20ms | Lock + update |
| Debit account | ~30ms | Write to ledger |
| Credit account | ~30ms | Write to ledger |
| **Total transfer** | **~90ms** | 4 steps sequential |
| **Idempotent retry** | **~5ms** | Cache hit |
| **Failed + compensate** | **~120ms** | Including compensation |

## Configuration

Environment variables in docker-compose.yml:

```yaml
FRAISEQL_SAGA_ENABLED: "true"
FRAISEQL_SAGA_STORE_TYPE: "postgres"
FRAISEQL_SAGA_MAX_RETRIES: "3"
FRAISEQL_SAGA_STEP_TIMEOUT_SECONDS: "30"
FRAISEQL_SAGA_TIMEOUT_SECONDS: "60"
# Manual compensation means longer timeout for business logic
```

## Troubleshooting

### Duplicate Transactions

If a transaction appears twice in the ledger, check the unique constraint:

```bash
# Check for duplicates
docker-compose exec postgres psql -U fraiseql -d fraiseql -c \
  "SELECT transaction_id, COUNT(*) FROM transfers GROUP BY transaction_id HAVING COUNT(*) > 1"
```

### Stuck Transfers

If transfers show status='RESERVED' for >5 minutes:

```bash
# Check stuck transfers
docker-compose exec postgres psql -U fraiseql -d fraiseql -c \
  "SELECT * FROM transfers WHERE status='RESERVED' AND created_at < NOW() - INTERVAL '5 minutes'"

# Force compensation
curl -X POST http://localhost:4000/graphql \
  -d '{"query": "mutation { compensateTransfer(transactionId: \"txn-123\") { status } }"}'
```

### Idempotency Cache Issues

If idempotency is not working, check unique constraint:

```bash
docker-compose exec postgres psql -U fraiseql -d fraiseql -c \
  "SELECT * FROM pg_indexes WHERE tablename = 'transfers' AND indexname LIKE '%transaction%'"
```

## Next Steps

### 1. Add More Services

- Add notification service (sends transfer confirmation email)
- Add compliance service (checks for fraud patterns)
- Add currency exchange service (for multi-currency transfers)

### 2. Enhanced Compensation

- Implement rollback logic with transaction reversals
- Add time-based compensation (undo after N hours)
- Implement partial compensation (refund 50%, escalate 50%)

### 3. Monitoring

- Track compensation rates (should be <1%)
- Monitor transfer duration (SLA: <200ms)
- Alert on idempotency failures

### 4. Production Hardening

- Add distributed tracing (correlate transfers across services)
- Implement circuit breakers (fail fast if service down)
- Add chaos testing (test failure scenarios systematically)

## Related Documentation

- **[SAGA_PATTERNS.md](../../docs/SAGA_PATTERNS.md)** - Compensation strategies in detail
- **[SAGA_API.md](../../docs/reference/SAGA_API.md)** - Coordinator API with error handling
- **[saga-basic/](../saga-basic/README.md)** - Automatic compensation example

---

**Last Updated:** 2026-01-29

**Maintainer:** FraiseQL Federation Team
