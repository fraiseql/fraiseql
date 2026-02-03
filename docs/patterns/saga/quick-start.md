# Getting Started with Sagas

**Audience:** Backend developers implementing distributed transactions
**Prerequisites:** Understanding of GraphQL federation and basic transaction concepts
**Estimated Reading Time:** 15-20 minutes

---

## Table of Contents

1. [What Are Sagas?](#what-are-sagas)
2. [Why Use Sagas?](#why-use-sagas)
3. [Core Concepts](#core-concepts)
4. [Your First Saga](#your-first-saga)
5. [Testing Your Saga](#testing-your-saga)
6. [Common Patterns](#common-patterns)
7. [Troubleshooting](#troubleshooting)

---

## What Are Sagas?

A **saga** is a pattern for managing distributed transactions across multiple services in a microservices architecture. Instead of relying on a single database transaction, sagas coordinate actions across multiple services while maintaining data consistency.

Sagas break a long-running business transaction into a series of **steps**:

- **Forward steps**: Perform the business operation
- **Compensation steps**: Undo the operation if something fails downstream

### Example: Order Processing

When a customer places an order, multiple services might be involved:

```

1. Debit customer's account (Payment Service)
2. Reserve inventory (Inventory Service)
3. Schedule delivery (Shipping Service)
4. Update order status (Order Service)
```

If step 3 fails (no delivery available), a saga automatically:

- Compensates step 2 (returns inventory to shelf)
- Compensates step 1 (refunds customer)
- Updates order status to "cancelled"

---

## Why Use Sagas?

### Problems They Solve

**Traditional Distributed Transactions (2-Phase Commit)**

- Slow: Locks resources for extended periods
- Fragile: Coordinator failure can leave system in inconsistent state
- Expensive: Requires support from all involved systems

**Sagas Solve This By:**
- ✅ **Eventual Consistency**: Guarantees consistency across services
- ✅ **No Distributed Locks**: Better performance and availability
- ✅ **Automatic Recovery**: Handles failures gracefully
- ✅ **Clear Audit Trail**: Every step is recorded

### When to Use Sagas

| Scenario | Use Saga? | Alternative |
|----------|-----------|-------------|
| Cross-service transaction | ✅ Yes | Distributed transaction (2PC) |
| Single-service operation | ❌ No | Standard database transaction |
| Long-running process | ✅ Yes | Event streaming |
| Financial transfers | ✅ Yes | Immediate consistency (if possible) |
| Order fulfillment | ✅ Yes | Choreography pattern |

---

## Core Concepts

### Saga Coordinator

The **saga coordinator** orchestrates the execution of all steps in a saga. It:

- Receives the initial saga request
- Executes forward steps in sequence
- Handles failures and triggers compensation
- Maintains saga state in a saga store

```rust
// The coordinator manages the entire saga lifecycle
let coordinator = SagaCoordinator::new(metadata, store);
let saga = coordinator.execute(steps).await?;
```

### Saga Steps

Each step has two parts:

```rust
SagaStep {
    forward: Mutation,        // Do the operation
    compensation: Mutation,   // Undo the operation
}
```

**Forward Step Example**: Charge customer's credit card
**Compensation Step Example**: Refund the charge

### Saga Store

The **saga store** persists saga state to disk. This allows:

- Saga recovery after failures
- Audit trail of all operations
- Idempotency (running same step twice produces same result)

Supported backends:

- PostgreSQL (recommended for production)
- MySQL
- SQLite (development/testing)

---

## Your First Saga

### Step 1: Define Your Saga Orchestrator

Create a file `orders/saga_coordinator.rs`:

```rust
use fraiseql_core::federation::saga::{
    SagaCoordinator, SagaStep, SagaStore,
};

pub async fn create_order_saga(
    user_id: String,
    items: Vec<OrderItem>,
    payment_info: PaymentInfo,
) -> Result<Order> {
    // Initialize saga store
    let store = PostgresSagaStore::new(connection_pool).await?;

    // Create coordinator
    let coordinator = SagaCoordinator::new(metadata, store);

    // Define steps
    let steps = vec![
        SagaStep {
            // Step 1: Debit payment
            forward: Mutation {
                service: "payment",
                operation: "chargeCard",
                variables: json!({
                    "amount": calculate_total(&items),
                    "card": payment_info,
                }),
            },
            compensation: Mutation {
                service: "payment",
                operation: "refundCharge",
                variables: json!({ "charge_id": "{payment_charge_id}" }),
            },
        },
        SagaStep {
            // Step 2: Reserve inventory
            forward: Mutation {
                service: "inventory",
                operation: "reserveItems",
                variables: json!({
                    "items": items,
                    "order_id": "{order_id}",
                }),
            },
            compensation: Mutation {
                service: "inventory",
                operation: "releaseReservation",
                variables: json!({ "order_id": "{order_id}" }),
            },
        },
        SagaStep {
            // Step 3: Create order record
            forward: Mutation {
                service: "orders",
                operation: "createOrder",
                variables: json!({
                    "user_id": user_id,
                    "items": items,
                    "status": "confirmed",
                }),
            },
            compensation: Mutation {
                service: "orders",
                operation: "cancelOrder",
                variables: json!({ "order_id": "{order_id}" }),
            },
        },
    ];

    // Execute saga
    let result = coordinator.execute(steps).await?;
    Ok(result.data["createOrder"].clone())
}
```

### Step 2: Configure the Saga Store

In `Cargo.toml`:

```toml
[dependencies]
fraiseql-core = { version = "2.0", features = ["saga-postgres"] }
```

In your application setup:

```rust
// Configure PostgreSQL saga store
let saga_store = PostgresSagaStore::new(
    PostgresSagaStoreConfig {
        connection_string: "postgres://user:pass@localhost/fraiseql",
        max_pool_size: 10,
        migration_path: "migrations/",
    }
).await?;
```

### Step 3: Call Your Saga

```rust
#[tokio::main]
async fn main() -> Result<()> {
    let order = create_order_saga(
        "user-123".to_string(),
        vec![
            OrderItem { sku: "BOOK-001", qty: 2 },
            OrderItem { sku: "PEN-005", qty: 5 },
        ],
        PaymentInfo {
            card_number: "4111111111111111",
            exp_month: 12,
            exp_year: 2025,
        },
    ).await?;

    println!("Order created: {:?}", order);
    Ok(())
}
```

---

## Testing Your Saga

### Unit Test: Success Path

```rust
#[tokio::test]
async fn test_saga_success_path() {
    // Arrange
    let store = InMemorySagaStore::new(); // Use in-memory for tests
    let coordinator = SagaCoordinator::new(metadata, store);

    // Act
    let result = coordinator.execute(steps).await;

    // Assert
    assert!(result.is_ok());
    let saga_state = result.unwrap();
    assert_eq!(saga_state.status, SagaStatus::Completed);
    assert_eq!(saga_state.completed_steps, 3);
}
```

### Integration Test: Failure and Compensation

```rust
#[tokio::test]
async fn test_saga_compensation_on_failure() {
    // Arrange: Setup mock services where Step 2 fails
    let store = PostgresSagaStore::new(config).await.unwrap();
    let coordinator = SagaCoordinator::new(metadata_with_mocks, store);

    // Act: Execute saga
    let result = coordinator.execute(steps_with_failing_step_2).await;

    // Assert: Compensation was triggered
    assert!(result.is_err());

    // Verify compensation ran
    let compensation_logs = store.get_compensation_logs("saga-id").await?;
    assert_eq!(compensation_logs.len(), 1);
    assert_eq!(compensation_logs[0].step_index, 1); // Step 1 was compensated
}
```

### Chaos Testing: Network Failures

```rust
#[tokio::test]
async fn test_saga_recovery_after_network_failure() {
    // Arrange
    let store = PostgresSagaStore::new(config).await.unwrap();
    let coordinator_with_retry = SagaCoordinator::new(
        metadata,
        store,
    ).with_max_retries(3)
     .with_retry_delay(Duration::from_millis(100));

    // Act: Execute saga that temporarily fails
    let result = coordinator_with_retry.execute(steps).await;

    // Assert: Saga recovered and succeeded
    assert!(result.is_ok());
}
```

---

## Common Patterns

### Pattern 1: Request-Reply (Synchronous)

The saga waits for each step to complete before moving to the next.

**Use When:**
- You need immediate feedback to the user
- Steps have dependencies on earlier results
- Saga typically completes in seconds

**Example:**
```
User places order → Payment processed → Inventory reserved → Response sent
                    (wait)              (wait)
```

### Pattern 2: Fire-and-Forget (Asynchronous)

The saga coordinator returns immediately; steps execute in the background.

**Use When:**
- Steps are independent or loosely coupled
- User can wait for async completion
- Saga may take minutes/hours

**Example:**
```rust
let saga_id = coordinator.execute_async(steps).await?;
// Return saga_id to user immediately
// User can check status later with saga_id
```

### Pattern 3: Choreography (Event-Driven)

Services listen for events and trigger actions independently (no central coordinator).

**Use When:**
- Services are truly independent
- You want to avoid a central point of failure
- Compensation logic is simple

**Note:** FraiseQL's saga pattern uses **orchestration** (centralized coordinator) which is easier to reason about and test.

---

## Troubleshooting

### Problem: Saga Hangs

**Symptoms:**
- Saga never completes
- Coordinator waits indefinitely

**Solutions:**
1. Set timeouts on forward and compensation mutations
2. Check saga store connection
3. Review service health (is the service responding?)
4. Look for circular dependencies in compensation steps

```rust
// Add timeouts
let coordinator = SagaCoordinator::new(metadata, store)
    .with_timeout(Duration::from_secs(30));
```

### Problem: Duplicate Mutations

**Symptoms:**
- Charge appears twice on customer credit card
- Inventory count wrong

**Solutions:**
1. Ensure idempotent operations (safe to run multiple times)
2. Add request IDs to track duplicate requests
3. Check saga step retries

```rust
// Use idempotent operation IDs
let step = SagaStep {
    forward: Mutation {
        idempotency_key: "charge-order-123",
        operation: "chargeCard",
        ...
    },
    ...
};
```

### Problem: Compensation Never Runs

**Symptoms:**
- Saga fails but forward side effects remain
- Inventory not returned

**Solutions:**
1. Check compensation step syntax
2. Verify compensation mutation targets correct service
3. Review saga store for failed compensations
4. Ensure compensation service is reachable

```rust
// Check compensation logs
let logs = store.get_compensation_logs(saga_id).await?;
for log in logs {
    if log.status == CompensationStatus::Failed {
        println!("Failed compensation: {:?}", log.error);
    }
}
```

### Problem: Recovery Not Working

**Symptoms:**
- Saga fails, manual restart required
- No automatic retry happening

**Solutions:**
1. Check if saga store is properly configured
2. Verify recovery manager is running
3. Check database migration status
4. Review recovery manager configuration

```rust
// Verify saga store has persistence
let saga = store.get_saga(saga_id).await?;
assert!(saga.is_some(), "Saga should be persisted");

// Trigger manual recovery if needed
coordinator.recover_failed_sagas().await?;
```

---

## Next Steps

Now that you understand saga basics:

1. **Read [patterns.md](patterns.md)** for advanced coordination patterns
2. **Review [federation sagas](../../integrations/federation/sagas.md)** to integrate sagas with Apollo Federation
3. **Check [SAGA API Reference](../../reference/SAGA_API.md)** for detailed API documentation

## See Also

- [Saga State Machines](patterns.md#saga-state-machines)
- [Error Handling Strategies](patterns.md#error-handling-strategies)
- [Performance Optimization](patterns.md#performance-optimization)

---

**Last Updated:** 2026-01-29
**Maintainer:** FraiseQL Federation Team
