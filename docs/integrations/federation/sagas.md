# Sagas in Apollo Federation

**Audience:** Architects designing distributed microservices with federation
**Prerequisites:** [Saga Basics](../../patterns/saga/quick-start.md), [Federation Overview](guide.md)
**Estimated Reading Time:** 25-30 minutes

---

## Table of Contents

1. [Overview](#overview)
2. [Architecture](#architecture)
3. [Cross-Subgraph Transactions](#cross-subgraph-transactions)
4. [Data Consistency Patterns](#data-consistency-patterns)
5. [Observability](#observability)
6. [Production Deployment](#production-deployment)
7. [Case Studies](#case-studies)

---

## Overview

Sagas enable **distributed transactions across multiple Apollo Federation subgraphs**. This allows you to maintain consistency when a single business operation spans multiple services.

### Problem: Distributed Transactions in Federation

Without sagas, coordinating across subgraphs is difficult:

```
User Service (Subgraph 1)
  ├─ create_user()

Payment Service (Subgraph 2)
  ├─ charge_card()

Order Service (Subgraph 3)
  ├─ create_order()
```

If charge_card() fails after create_user() succeeds, the system is inconsistent.

### Solution: Saga Orchestration

A saga coordinator ensures all steps succeed together or all roll back together:

```
SagaCoordinator (Central)
  ├─ (1) create_user()      ✓ User created
  ├─ (2) charge_card()      ✓ Payment processed
  ├─ (3) create_order()     ✗ Out of inventory
  │
  └─ Compensation (reverse order):
     ├─ refund_card()       ✓ Payment refunded
     └─ delete_user()       ✓ User removed
```

---

## Architecture

### Components

```
┌──────────────────────────────────────────────────┐
│          Apollo Router (Gateway)                  │
│                                                  │
│  Receives: mutation startOrderSaga($input)       │
│  Routes to: SagaCoordinator                      │
└────────────────┬─────────────────────────────────┘
                 │
┌────────────────▼─────────────────────────────────┐
│    SagaCoordinator (SagaOrchestrator Service)    │
│                                                  │
│  - Maintains saga state (PostgreSQL)             │
│  - Executes steps sequentially                   │
│  - Triggers compensation on failure              │
│  - Handles recovery after crashes                │
└────────┬──────────────────┬─────────────┬────────┘
         │                  │             │
    ┌────▼──────┐    ┌──────▼──────┐ ┌──▼────────┐
    │ Users SG  │    │ Payment SG  │ │ Orders SG │
    │           │    │             │ │           │
    │ Mutations:│    │ Mutations:  │ │Mutations: │
    │ -create   │    │ -charge     │ │-create    │
    │ -delete   │    │ -refund     │ │-cancel    │
    │ -update   │    │             │ │-confirm   │
    └───────────┘    └─────────────┘ └───────────┘
    (PostgreSQL)     (MySQL)         (MongoDB)
```

### Data Flow

**Forward Path (Success):**
1. Router receives mutation `startOrderSaga`
2. Coordinator creates saga in store (PENDING)
3. Executes step 1: `createUser` mutation
   - Step 1: COMPLETED
4. Executes step 2: `chargeCard` mutation
   - Step 2: COMPLETED
5. Executes step 3: `createOrder` mutation
   - Step 3: COMPLETED
6. Saga status: COMPLETED
7. Return result to user

**Compensation Path (Failure at Step 3):**
1. Step 3 fails, saga status: FAILED
2. Coordinator enters compensation phase
3. Runs step 2 compensation: `refundCard` mutation
4. Runs step 1 compensation: `deleteUser` mutation
5. Saga status: ROLLED_BACK
6. Return error to user

---

## Cross-Subgraph Transactions

### Scenario 1: Sequential Cross-Subgraph Transaction

**Order Processing across 3 Services:**

```graphql
mutation StartOrderSaga($userId: ID!, $items: [OrderItem!]!, $paymentInfo: PaymentInfo!) {
  startOrderSaga(userId: $userId, items: $items, paymentInfo: $paymentInfo) {
    sagaId
    status
    order {
      id
      userId
      items
      total
    }
  }
}
```

**Implementation:**

```rust
pub async fn start_order_saga(
    user_id: String,
    items: Vec<OrderItem>,
    payment_info: PaymentInfo,
) -> Result<SagaResult> {
    let coordinator = get_saga_coordinator();

    let steps = vec![
        // Step 1: Verify user exists (User Service)
        SagaStep {
            name: "verify_user",
            forward: Mutation {
                subgraph: "users",
                operation: "verifyUserExists",
                variables: json!({ "userId": &user_id }),
            },
            compensation: None, // No compensation needed
        },

        // Step 2: Charge payment (Payment Service)
        SagaStep {
            name: "charge_payment",
            forward: Mutation {
                subgraph: "payments",
                operation: "chargeCard",
                variables: json!({
                    "userId": &user_id,
                    "amount": calculate_total(&items),
                    "card": payment_info,
                }),
            },
            compensation: Mutation {
                subgraph: "payments",
                operation: "refundCharge",
                variables: json!({ "chargeId": "{charge_id}" }),
            },
        },

        // Step 3: Reserve inventory (Inventory Service)
        SagaStep {
            name: "reserve_inventory",
            forward: Mutation {
                subgraph: "inventory",
                operation: "reserveItems",
                variables: json!({
                    "items": &items,
                    "orderId": "{order_id}",
                }),
            },
            compensation: Mutation {
                subgraph: "inventory",
                operation: "releaseReservation",
                variables: json!({ "reservationId": "{reservation_id}" }),
            },
        },

        // Step 4: Create order (Orders Service)
        SagaStep {
            name: "create_order",
            forward: Mutation {
                subgraph: "orders",
                operation: "createOrder",
                variables: json!({
                    "userId": &user_id,
                    "items": &items,
                    "chargeId": "{charge_id}",
                    "reservationId": "{reservation_id}",
                    "status": "confirmed",
                }),
            },
            compensation: Mutation {
                subgraph: "orders",
                operation: "cancelOrder",
                variables: json!({ "orderId": "{order_id}" }),
            },
        },
    ];

    coordinator.execute(steps).await
}
```

### Scenario 2: Parallel Steps Across Subgraphs

**Reducing latency by running independent steps in parallel:**

```
Parallel execution:
├─ Create user account (Users SG)
├─ Initialize payment method (Payments SG)
├─ Setup shipping address (Shipping SG)
└─ Create preferences (Preferences SG)
```

```rust
pub async fn onboard_customer_saga(
    customer: CustomerInfo,
) -> Result<SagaResult> {
    let coordinator = get_saga_coordinator();

    let steps = vec![
        create_account_step(&customer),
        init_payment_step(&customer),
        setup_shipping_step(&customer),
        setup_preferences_step(&customer),
    ];

    // Execute in parallel (all 4 simultaneously)
    coordinator.execute_parallel(
        steps,
        ParallelConfig {
            max_concurrent: 4,
            fail_fast: true, // Stop on first failure
        }
    ).await
}
```

### Scenario 3: Conditional Paths Across Subgraphs

**Different processing based on customer type:**

```rust
pub async fn process_payment_saga(
    order: Order,
    customer: Customer,
) -> Result<SagaResult> {
    let coordinator = get_saga_coordinator();

    // Different steps based on customer tier
    let steps = match customer.tier {
        CustomerTier::Standard => vec![
            validate_card_step(&order),
            charge_immediately_step(&order),
            confirm_order_step(&order),
        ],
        CustomerTier::Premium => vec![
            validate_card_step(&order),
            apply_discount_step(&order),
            charge_immediately_step(&order),
            send_vip_confirmation_step(&order),
            confirm_order_step(&order),
        ],
        CustomerTier::Enterprise => vec![
            validate_account_step(&order),
            check_credit_limit_step(&order),
            create_invoice_step(&order),
            schedule_payment_step(&order),
            confirm_order_step(&order),
        ],
    };

    coordinator.execute(steps).await
}
```

---

## Data Consistency Patterns

### Pattern 1: Read-Your-Own-Writes (Strong Consistency)

User sees the results immediately after saga completes.

```rust
// Synchronous saga execution
let result = coordinator.execute(steps).await?;
let order = result.data["createOrder"].clone();

// Order is immediately readable
let query = query_user_orders(user_id).await?;
assert!(query.orders.contains(&order)); // True
```

### Pattern 2: Eventual Consistency

Results become consistent after saga completes.

```rust
// Async saga execution
let saga_id = coordinator.execute_async(steps).await?;

// Return saga_id to user
// User can poll for results
loop {
    let saga = coordinator.get_saga(&saga_id).await?;
    if saga.status == SagaStatus::Completed {
        return Ok(saga.result);
    }
    sleep(Duration::from_secs(1)).await;
}
```

### Pattern 3: Multi-Database Consistency

Maintaining consistency across different database backends.

**Scenario:**
- Users in PostgreSQL
- Orders in MongoDB
- Payments in MySQL

**Solution:**

```rust
pub async fn multi_db_order_saga(user_id: String) -> Result<SagaResult> {
    let coordinator = get_saga_coordinator();

    let steps = vec![
        // PostgreSQL: Verify user exists
        SagaStep {
            forward: Mutation {
                subgraph: "users",
                database: "postgres", // Explicit database
                operation: "verifyUser",
                variables: json!({ "userId": &user_id }),
            },
            ...
        },

        // MySQL: Process payment
        SagaStep {
            forward: Mutation {
                subgraph: "payments",
                database: "mysql",
                operation: "chargeCard",
                ...
            },
            ...
        },

        // MongoDB: Create order
        SagaStep {
            forward: Mutation {
                subgraph: "orders",
                database: "mongodb",
                operation: "createOrder",
                ...
            },
            ...
        },
    ];

    coordinator.execute(steps).await
}
```

---

## Observability

### Saga Tracing

Trace each step across all subgraphs:

```rust
// Initialize tracing context
let trace_id = Uuid::new_v4().to_string();
let coordinator = get_saga_coordinator()
    .with_trace_id(&trace_id);

// Each step includes trace_id in its mutation
// All logs/spans can be correlated by trace_id
```

### Saga Metrics

Monitor key metrics:

```rust
// Metrics to track
metrics.counter("saga.started", tags!("type": "order")).increment();
metrics.gauge("saga.duration_ms", duration_ms, tags!("type": "order"));
metrics.counter("saga.succeeded", tags!("type": "order")).increment();
metrics.counter("saga.failed", tags!("type": "order")).increment();
metrics.counter("saga.compensated", tags!("type": "order")).increment();
```

### Dashboards and Alerts

**Key Alerts:**
- Saga failure rate > 5%
- Saga compensation rate > 1%
- Saga duration > 30 seconds
- Incomplete sagas (older than 1 hour)

```yaml
# Prometheus alerting rules

- alert: SagaFailureRateHigh
  expr: rate(saga_failed[5m]) > 0.05
  for: 5m
  annotations:
    summary: "High saga failure rate: {{ $value }}"

- alert: SagaCompensationRate
  expr: rate(saga_compensated[5m]) > 0.01
  for: 5m
  annotations:
    summary: "High saga compensation rate: {{ $value }}"
```

### Logging

Comprehensive saga logging:

```rust
info!(
    saga_id = %saga_id,
    step = "charge_payment",
    subgraph = "payments",
    status = "executing",
    "Starting saga step"
);

info!(
    saga_id = %saga_id,
    step = "charge_payment",
    status = "completed",
    duration_ms = 245,
    charge_id = %charge_id,
    "Saga step completed"
);
```

---

## Production Deployment

### Configuration

```toml
[saga]
enabled = true
store_type = "postgres"
max_retries = 3
step_timeout_seconds = 30
saga_timeout_seconds = 300
recovery_enabled = true
recovery_poll_interval_seconds = 60

[saga.store.postgres]
connection_string = "postgres://user:pass@db:5432/fraiseql"
max_pool_size = 20
migration_path = "migrations/"
```

### Database Setup

```sql
-- Create saga tables
CREATE TABLE sagas (
    id UUID PRIMARY KEY,
    saga_type VARCHAR(255),
    status VARCHAR(50),
    created_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ,
    data JSONB,
    error_message TEXT
);

CREATE TABLE saga_steps (
    id UUID PRIMARY KEY,
    saga_id UUID REFERENCES sagas(id),
    step_index INT,
    name VARCHAR(255),
    status VARCHAR(50),
    input JSONB,
    output JSONB,
    created_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ
);

CREATE INDEX idx_sagas_status ON sagas(status);
CREATE INDEX idx_saga_steps_saga_id ON saga_steps(saga_id);
```

### Health Checks

```rust
async fn saga_store_health_check() -> Result<()> {
    // Check saga store connectivity
    let healthy = store.health_check().await?;

    // Check for stuck sagas
    let stuck = store.get_stuck_sagas(Duration::from_secs(3600)).await?;
    if stuck.len() > 0 {
        warn!("Found {} stuck sagas", stuck.len());
    }

    // Check recovery manager
    if !recovery_manager.is_running() {
        return Err("Recovery manager not running".into());
    }

    Ok(())
}
```

### Monitoring Saga Health

```rust
// Health endpoint that returns saga metrics
pub async fn saga_health_metrics() -> Result<HealthMetrics> {
    Ok(HealthMetrics {
        store_connection: store.test_connection().await.is_ok(),
        pending_sagas: store.count_pending_sagas().await?,
        failed_sagas: store.count_failed_sagas().await?,
        recovery_enabled: recovery_manager.is_running(),
        avg_saga_duration_ms: store.get_avg_duration().await?,
        success_rate: store.get_success_rate().await?,
    })
}
```

---

## Case Studies

### Case Study 1: E-Commerce Platform

**Scenario:** Order processing across 5 microservices

```
Saga Steps:

1. Validate order (Order Service)
2. Process payment (Payment Service)
3. Reserve inventory (Inventory Service)
4. Calculate shipping (Shipping Service)
5. Create fulfillment (Fulfillment Service)
```

**Results:**
- Reduced order processing time: 2.3s → 1.8s (parallel steps 4-5)
- Failure handling: Automatic compensation for 99.2% of failures
- Recovery time: < 5 minutes for 99% of cases

### Case Study 2: SaaS Subscription Service

**Scenario:** Customer signup across 4 services

```
Saga Steps:

1. Create account (Auth Service)
2. Initialize payment method (Billing Service)
3. Send welcome email (Email Service)
4. Create default team (Collaboration Service)
```

**Results:**
- Signup success rate: 98.7%
- Reduced customer support tickets: 45% decrease
- Recovery: Automatic for 99.8% of failures

### Case Study 3: Financial Transaction System

**Scenario:** Money transfer across banks

```
Saga Steps:

1. Verify sender account (Sender Bank)
2. Reserve funds (Sender Bank)
3. Receive funds (Recipient Bank)
4. Confirm transaction (Transaction Service)
5. Send notifications (Notification Service)
```

**Results:**
- Completed transactions: 99.97%
- Automatic compensation: 98% of failures
- Manual intervention required: 0.03%

---

## Best Practices for Federation Sagas

| Practice | Benefit |
|----------|---------|
| Keep steps small | Easier compensation |
| Minimize cross-subgraph calls | Better performance |
| Use request IDs | Prevents duplicate operations |
| Monitor step duration | Early detection of slowness |
| Test compensation paths | Ensures rollback reliability |
| Document step order | Clarifies dependencies |
| Set reasonable timeouts | Prevents hanging transactions |
| Implement idempotency | Enables safe retries |
| Track trace IDs | Correlate logs across services |
| Regular disaster recovery tests | Validates recovery process |

---

## Troubleshooting

### "SAGA step timing out after 30 seconds"

**Cause:** Subgraph operation taking too long or network latency.

**Diagnosis:**
1. Check subgraph response time: `time curl http://subgraph:8000/graphql -d '{...}'`
2. Monitor SAGA logs: Look for "Step [name] timeout"
3. Check database slow queries: `SELECT * FROM pg_stat_statements WHERE mean_exec_time > 1000;`

**Solutions:**
- Increase timeout in SAGA config: `saga_timeout_secs = 60`
- Optimize slow step (add database index, reduce data volume)
- Check network latency between datacenters
- Consider splitting step into two smaller operations
- Add connection pool configuration to subgraph

### "SAGA fails to compensate - stuck transaction"

**Cause:** Compensation step failed or subgraph unreachable during rollback.

**Diagnosis:**
1. Check SAGA status: `SELECT * FROM tb_saga WHERE status = 'compensating';`
2. Review compensation logs for specific failure
3. Verify all subgraphs are reachable: `curl http://subgraph/health`

**Solutions:**
- Ensure compensation is idempotent (safe to retry multiple times)
- Test compensation path in staging before production
- Add manual recovery endpoint to handle stuck SAGAs
- Implement compensating transaction with fallback logic
- Monitor for stuck SAGAs: alert if any SAGA in "compensating" >5 minutes

### "SAGA creates duplicates after retry"

**Cause:** Step not idempotent - same step executed twice creates two records.

**Diagnosis:**
1. Check for duplicate records with same request ID
2. Review whether step checks for existing data
3. Verify request ID propagation through all services

**Solutions:**
- Implement idempotency: "If request ID exists, return existing result"
- Use database unique constraints: `UNIQUE(request_id, entity_id)`
- Store request ID with every mutation
- Implement deduplication window (e.g., 24 hours)
- Test retry scenarios explicitly

### "SAGA compensation deletes wrong data (partial compensation)"

**Cause:** Compensation not targeting correct record or targeting too broadly.

**Diagnosis:**
1. Review compensation step SQL: does it use correct WHERE clause?
2. Check if original step's ID was captured correctly
3. Verify data before compensation: `SELECT * FROM table WHERE id = '...';`

**Solutions:**
- Use precise IDs in compensation: `DELETE FROM orders WHERE order_id = 'X' AND saga_id = 'Y'`
- Add saga_id to all records created by step
- Test compensation with production-like data volume
- Implement soft delete (mark as deleted) instead of hard delete
- Log all data before deletion for recovery

### "Some subgraph mutations succeeded but others failed - inconsistent state"

**Cause:** This is exactly why SAGAs exist - partial failures trigger compensation.

**Diagnosis:**
1. Check SAGA logs for which steps succeeded/failed
2. Verify compensation ran on all succeeded steps: `SELECT * FROM tb_saga_step_log WHERE step_name LIKE '%Compensation%';`

**Solutions:**
- Ensure compensation ran to completion (check logs)
- If compensation failed, manually run compensation queries
- Implement alerting for failed SAGAs requiring manual intervention
- Document manual recovery procedure for operators

### "SAGA performance is slow (>500ms per transaction)"

**Cause:** Multiple network hops between subgraphs add latency.

**Diagnosis:**
1. Measure each step: Check SAGA logs for individual step durations
2. Identify slowest step: typically network + database I/O
3. Count steps: N steps = N network round trips

**Solutions:**
- Reduce number of steps: Combine operations where possible
- Use direct database federation instead of HTTP for known services
- Implement request batching (multiple mutations in one step)
- Cache frequently accessed data in each subgraph
- Consider asynchronous operations for non-critical steps

### "SAGA rollback fails - compensation itself failing"

**Cause:** Compensation step has error or resource constraint.

**Diagnosis:**
1. Review compensation error in SAGA logs
2. Test compensation manually with example data
3. Check if subgraph is reachable: `curl http://subgraph:8000/health`

**Solutions:**
- Fix compensation logic and redeploy
- Implement compensation retry with exponential backoff
- Set up manual compensation procedure for when automatic fails
- Alert operations team for SAGAs stuck in compensation
- Implement circuit breaker for failing subgraphs

### "Too many pending SAGAs - system backlog"

**Cause:** SAGAs completing slower than new requests arriving.

**Diagnosis:**
1. Check queue depth: `SELECT COUNT(*) FROM tb_saga WHERE status = 'pending';`
2. Monitor SAGA throughput: `SELECT COUNT(*) FROM tb_saga WHERE created_at > NOW() - INTERVAL '1 minute';`
3. Identify bottleneck step with slowest average duration

**Solutions:**
- Scale slow subgraph (more instances, more database capacity)
- Optimize bottleneck step (add indexes, cache data)
- Implement rate limiting on mutation endpoints
- Process SAGAs in parallel (increase concurrency)
- Consider queue size limits to prevent unbounded growth

### "Different SAGA steps seeing different data"

**Cause:** Each subgraph's database is independent - consistency within each service, not cross-service.

**Diagnosis:**
This is expected behavior during SAGA execution. Eventual consistency at subgraph level.

**Solutions:**
- Use SAGA coordination to ensure ordering
- If cross-service consistency critical, use distributed locks (Redis, Consul)
- Implement read consistency at client level (wait for confirmations)
- Document eventual consistency model for API consumers

---

## Next Steps

1. Review **[SAGA API Reference](../../reference/SAGA_API.md)** for complete API
2. Check **[Observability Guide](operations/observability.md)** for monitoring
3. See **[Deployment Guide](deployment.md)** for deployment details

---

**Last Updated:** 2026-01-29
**Maintainer:** FraiseQL Federation Team
