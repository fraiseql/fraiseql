<!-- Skip to main content -->
---
title: Saga Patterns and Best Practices
description: 1. [Saga Patterns](#saga-patterns)
keywords: ["workflow", "saas", "realtime", "ecommerce", "analytics", "federation"]
tags: ["documentation", "reference"]
---

# Saga Patterns and Best Practices

**Audience:** Backend developers designing distributed transaction flows
**Prerequisites:** [Getting Started with Sagas](quick-start.md)
**Estimated Reading Time:** 20-25 minutes

---

## Table of Contents

1. [Saga Patterns](#saga-patterns)
2. [Compensation Strategies](#compensation-strategies)
3. [Recovery Patterns](#recovery-patterns)
4. [Idempotency](#idempotency)
5. [State Machines](#state-machines)
6. [Error Handling](#error-handling)
7. [Performance Optimization](#performance-optimization)

---

## Saga Patterns

### Pattern 1: Sequential Saga

Execute steps one after another. Each step depends on the previous one's success.

#### When to Use

- Steps must execute in order
- Later steps need data from earlier ones
- Rollback is straightforward

#### Example: Order Processing

**Diagram:** System architecture visualization

```d2
<!-- Code example in D2 Diagram -->
direction: down

Validate: "Step 1: Validate order\n✓" {
  shape: box
  style.fill: "#c8e6c9"
}

Charge: "Step 2: Charge payment\n✓" {
  shape: box
  style.fill: "#c8e6c9"
}

Reserve: "Step 3: Reserve inventory\n✓" {
  shape: box
  style.fill: "#c8e6c9"
}

Ship: "Step 4: Ship items\n✓" {
  shape: box
  style.fill: "#c8e6c9"
}

Validate -> Charge
Charge -> Reserve
Reserve -> Ship
```text
<!-- Code example in TEXT -->

**If Step 3 fails:**

**Diagram:** System architecture visualization

```d2
<!-- Code example in D2 Diagram -->
direction: up

UndoReserve: "Compensation: Undo reservation\n✓" {
  shape: box
  style.fill: "#ffccbc"
}

UndoCharge: "Compensation: Refund charge\n✓" {
  shape: box
  style.fill: "#ffccbc"
}

ValidatePasses: "Step 1: Validation passes\n✓" {
  shape: box
  style.fill: "#c8e6c9"
}

Error: "❌ Step 3 Failed\n(insufficient inventory)" {
  shape: box
  style.fill: "#ffebee"
}

Error -> UndoReserve
UndoReserve -> UndoCharge
UndoCharge -> ValidatePasses
```text
<!-- Code example in TEXT -->

#### Code Example

```rust
<!-- Code example in RUST -->
async fn sequential_order_saga(order: Order) -> Result<SagaResult> {
    let steps = vec![
        validate_order_step(&order),
        charge_payment_step(&order),
        reserve_inventory_step(&order),
        ship_items_step(&order),
    ];

    coordinator.execute(steps).await
}
```text
<!-- Code example in TEXT -->

### Pattern 2: Parallel Saga

Execute independent steps concurrently.

#### When to Use

- Steps are independent (no data dependencies)
- You need better performance
- Coordination complexity is acceptable

#### Example: Customer Onboarding

**Diagram:** System architecture visualization

```d2
<!-- Code example in D2 Diagram -->
direction: right

Start: "Customer\nOnboarding\nSaga" {
  shape: box
  style.fill: "#e3f2fd"
}

Account: "Create user\naccount" {
  shape: box
  style.fill: "#c8e6c9"
}

Payment: "Initialize\npayment method" {
  shape: box
  style.fill: "#c8e6c9"
}

Email: "Send welcome\nemail" {
  shape: box
  style.fill: "#c8e6c9"
}

Newsletter: "Subscribe to\nnewsletter" {
  shape: box
  style.fill: "#c8e6c9"
}

Complete: "All tasks\ncomplete\n✓" {
  shape: box
  style.fill: "#e8f5e9"
}

Start -> Account
Start -> Payment
Start -> Email
Start -> Newsletter
Account -> Complete
Payment -> Complete
Email -> Complete
Newsletter -> Complete

note: "(All 4 run simultaneously - independent steps)"
```text
<!-- Code example in TEXT -->

#### Code Example

```rust
<!-- Code example in RUST -->
async fn parallel_onboarding_saga(user: User) -> Result<SagaResult> {
    let steps = vec![
        create_account_step(&user),
        init_payment_step(&user),
        send_email_step(&user),
        subscribe_newsletter_step(&user),
    ];

    // Execute in parallel with compensation ordering
    coordinator.execute_parallel(
        steps,
        ParallelConfig {
            max_concurrent: 4,
            fail_fast: false, // Wait for all to complete
        }
    ).await
}
```text
<!-- Code example in TEXT -->

### Pattern 3: Branching Saga

Execute different paths based on conditions.

#### When to Use

- Different paths for different customer types
- Premium vs standard workflows
- Region-specific business logic

#### Example: Payment Processing

```text
<!-- Code example in TEXT -->
If amount < $100:
  └── Use payment service A (fast)
Else if amount < $1000:
  └── Use payment service B (requires verification)
Else:
  └── Use payment service C (manual approval)
```text
<!-- Code example in TEXT -->

#### Code Example

```rust
<!-- Code example in RUST -->
async fn conditional_payment_saga(
    order: Order,
    payment_info: PaymentInfo,
) -> Result<SagaResult> {
    let steps = if order.total < 100.0 {
        // Quick path
        vec![
            charge_payment_quick_step(&order, &payment_info),
            confirm_order_step(&order),
        ]
    } else if order.total < 1000.0 {
        // Verification path
        vec![
            request_verification_step(&payment_info),
            charge_payment_verified_step(&order, &payment_info),
            confirm_order_step(&order),
        ]
    } else {
        // Manual approval path
        vec![
            submit_for_approval_step(&order),
            wait_for_approval_step(&order),
            charge_payment_step(&order),
            confirm_order_step(&order),
        ]
    };

    coordinator.execute(steps).await
}
```text
<!-- Code example in TEXT -->

### Pattern 4: Nested Saga

A saga that calls other sagas (composition).

#### When to Use

- Reusing saga logic across different processes
- Complex business processes need breaking down
- Testing individual sub-sagas

#### Example: Enterprise Order Processing

```text
<!-- Code example in TEXT -->
Main Saga: Process Order
├── Sub-Saga: Process Payment
│   ├── Check fraud detection
│   ├── Charge card
│   └── Send payment confirmation
├── Sub-Saga: Fulfill Order
│   ├── Reserve inventory
│   ├── Schedule shipping
│   └── Send tracking info
└── Sub-Saga: Update Analytics
    ├── Record transaction
    ├── Update customer stats
    └── Trigger recommendations
```text
<!-- Code example in TEXT -->

#### Code Example

```rust
<!-- Code example in RUST -->
async fn process_order_saga(order: Order) -> Result<SagaResult> {
    let coordinator = SagaCoordinator::new(metadata, store);

    // Sub-saga 1: Payment
    let payment_saga = coordinator.create_saga("process_payment", vec![
        charge_card_step(&order),
        send_confirmation_step(&order),
    ]);

    // Sub-saga 2: Fulfillment
    let fulfillment_saga = coordinator.create_saga("fulfill_order", vec![
        reserve_inventory_step(&order),
        schedule_shipping_step(&order),
    ]);

    // Execute sub-sagas sequentially
    payment_saga.execute().await?;
    fulfillment_saga.execute().await?;

    Ok(SagaResult::success())
}
```text
<!-- Code example in TEXT -->

---

## Compensation Strategies

### Strategy 1: Automatic Compensation

The coordinator automatically runs compensation for failed steps.

#### Pros

- Minimal code
- Predictable
- Easy to test

### Cons

- Less control
- May not fit complex business logic

#### Example

```rust
<!-- Code example in RUST -->
SagaStep {
    forward: Mutation {
        operation: "chargeCard",
        ...
    },
    compensation: Mutation {
        operation: "refundCharge",  // Automatically runs if later step fails
        ...
    },
}
```text
<!-- Code example in TEXT -->

### Strategy 2: Manual Compensation

Application explicitly triggers compensation based on business logic.

#### Pros

- Full control
- Can implement custom logic
- Better for complex scenarios

### Cons

- More code
- Risk of forgetting steps
- Harder to test

#### Example

```rust
<!-- Code example in RUST -->
async fn process_with_manual_compensation(order: Order) -> Result<()> {
    match coordinator.execute(steps).await {
        Ok(result) => {
            println!("Order processed: {}", result.order_id);
            Ok(())
        },
        Err(e) => {
            // Manual compensation logic
            match e.failed_step {
                "charge_card" => {
                    payment_service.refund(&order).await?;
                },
                "reserve_inventory" => {
                    inventory_service.release(&order).await?;
                    payment_service.refund(&order).await?;
                },
                _ => {}
            }
            Err(e)
        }
    }
}
```text
<!-- Code example in TEXT -->

### Strategy 3: Saga Compensator Service

A dedicated service handles all compensations.

#### Pros

- Separation of concerns
- Reusable compensation logic
- Easier to maintain

### Cons

- Additional service to deploy
- Network calls for compensation

#### Example

```rust
<!-- Code example in RUST -->
// Dedicated compensator service
pub struct SagaCompensator {
    payment_service: PaymentClient,
    inventory_service: InventoryClient,
    shipping_service: ShippingClient,
}

impl SagaCompensator {
    pub async fn compensate(&self, failed_saga: FailedSaga) -> Result<()> {
        for step in failed_saga.completed_steps.iter().rev() {
            match step.operation {
                "charge_card" => {
                    self.payment_service.refund(&step).await?;
                },
                "reserve_inventory" => {
                    self.inventory_service.release(&step).await?;
                },
                "schedule_shipping" => {
                    self.shipping_service.cancel(&step).await?;
                },
                _ => {}
            }
        }
        Ok(())
    }
}
```text
<!-- Code example in TEXT -->

---

## Recovery Patterns

### Pattern 1: Automatic Recovery

The system automatically retries failed sagas.

#### Configuration

```rust
<!-- Code example in RUST -->
let recovery_manager = RecoveryManager::new(config)
    .with_max_retries(3)
    .with_retry_delay(Duration::from_secs(5))
    .with_exponential_backoff(2.0);

recovery_manager.start_recovery_loop().await?;
```text
<!-- Code example in TEXT -->

### Behavior

1. Detects failed sagas in store
2. Retries from the point of failure
3. Applies exponential backoff
4. Gives up after max retries

### Pattern 2: Manual Recovery

Operators manually trigger recovery for specific sagas.

#### Use When

- Automatic recovery failed
- Saga is in inconsistent state
- Manual intervention required

#### Example

```rust
<!-- Code example in RUST -->
// Get failed saga
let saga = store.get_saga("saga-123").await?;

// Manually trigger recovery
recovery_manager.recover_saga(&saga).await?;

// Check recovery result
let updated_saga = store.get_saga("saga-123").await?;
println!("Status: {:?}", updated_saga.status);
```text
<!-- Code example in TEXT -->

### Pattern 3: Crash Recovery

Recovery after system restart.

#### How It Works

1. System boots
2. Recovery manager starts
3. Scans saga store for incomplete sagas
4. Resumes from last completed step
5. Reruns compensation or forward steps as needed

### Key Points

- Idempotency is critical (same step runs twice safely)
- Request IDs prevent duplicate side effects
- Last known state is restored

### Configuration

```rust
<!-- Code example in RUST -->
// Automatic crash recovery on startup
let recovery = RecoveryManager::new(store)
    .with_crash_recovery()
    .start()
    .await?;
```text
<!-- Code example in TEXT -->

---

## Idempotency

**Critical for sagas:** Operations must be safe to repeat.

### Principle: Request ID

Every saga step gets a unique request ID. If the same request ID runs again, it returns the cached result.

```rust
<!-- Code example in RUST -->
let step = SagaStep {
    forward: Mutation {
        request_id: "charge-order-123",  // Unique per request
        operation: "chargeCard",
        variables: json!({"amount": 99.99}),
    },
    ...
};
```text
<!-- Code example in TEXT -->

### Implementing Idempotency

#### In Payment Service

```rust
<!-- Code example in RUST -->
async fn charge_card(
    request_id: String,
    amount: Decimal,
) -> Result<ChargeResult> {
    // Check if we've already processed this request
    if let Some(cached) = charge_cache.get(&request_id).await {
        return Ok(cached);
    }

    // Process charge
    let result = process_charge(amount).await?;

    // Cache the result
    charge_cache.set(&request_id, &result).await?;

    Ok(result)
}
```text
<!-- Code example in TEXT -->

### Avoiding Idempotency Issues

#### Bad

```rust
<!-- Code example in RUST -->
// Not idempotent - running twice doubles the charge
async fn add_credit(user_id: &str, amount: f64) -> Result<()> {
    let current_balance = db.get_balance(user_id).await?;
    db.set_balance(user_id, current_balance + amount).await?;
    Ok(())
}
```text
<!-- Code example in TEXT -->

### Good

```rust
<!-- Code example in RUST -->
// Idempotent - running twice has same effect
async fn set_credit(
    user_id: &str,
    transaction_id: &str,
    amount: f64,
) -> Result<()> {
    // Check if transaction already applied
    if db.transaction_exists(transaction_id).await? {
        return Ok(()); // Already processed
    }

    let current_balance = db.get_balance(user_id).await?;
    db.set_balance(user_id, current_balance + amount).await?;
    db.record_transaction(transaction_id).await?;
    Ok(())
}
```text
<!-- Code example in TEXT -->

---

## State Machines

Sagas progress through well-defined states.

### Saga States

```text
<!-- Code example in TEXT -->
Start
  ↓
Executing (→ Forward Step 1, Step 2, ...)
  ├─ Success → Completed
  ├─ Failure → Compensating
        ↓
   (← Compensation Step N, ... Step 1)
   ├─ Success → Rolled Back
   └─ Failure → Failed / Needs Manual Recovery

Step States:
├─ Pending (not started)
├─ Executing (in progress)
├─ Succeeded (completed)
├─ Failed (returned error)
├─ Retrying (being retried)
└─ Compensating (being undone)
```text
<!-- Code example in TEXT -->

### Querying State

```rust
<!-- Code example in RUST -->
let saga = store.get_saga(saga_id).await?;

match saga.status {
    SagaStatus::Executing => {
        println!("Saga in progress: {:?}", saga.current_step);
    },
    SagaStatus::Completed => {
        println!("Saga completed successfully");
    },
    SagaStatus::RolledBack => {
        println!("Saga rolled back due to failure");
    },
    SagaStatus::Failed => {
        println!("Saga failed - manual intervention needed");
    },
}
```text
<!-- Code example in TEXT -->

---

## Error Handling

### Transient vs Permanent Errors

**Transient Errors** (retry-able):

- Network timeout
- Service temporarily unavailable
- Database connection dropped

**Permanent Errors** (don't retry):

- Validation failure
- Authorization error
- Business rule violation

### Retry Strategy

```rust
<!-- Code example in RUST -->
pub struct RetryPolicy {
    pub max_retries: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
    pub backoff_multiplier: f64,
}

impl RetryPolicy {
    pub fn exponential_backoff(attempt: u32) -> Duration {
        let delay_ms = (100 * (self.backoff_multiplier.powi(attempt as i32))) as u64;
        Duration::from_millis(delay_ms.min(self.max_delay.as_millis() as u64))
    }
}
```text
<!-- Code example in TEXT -->

### Error Recovery

```rust
<!-- Code example in RUST -->
async fn execute_with_error_recovery(
    step: SagaStep,
) -> Result<StepResult> {
    let mut attempts = 0;
    let max_attempts = 3;

    loop {
        match execute_step(&step).await {
            Ok(result) => return Ok(result),
            Err(e) if e.is_transient() && attempts < max_attempts => {
                attempts += 1;
                let delay = calculate_backoff_delay(attempts);
                tokio::time::sleep(delay).await;
                continue;
            },
            Err(e) => return Err(e),
        }
    }
}
```text
<!-- Code example in TEXT -->

---

## Performance Optimization

### Optimization 1: Caching Saga Results

Avoid re-executing completed sagas.

```rust
<!-- Code example in RUST -->
let cache = SagaResultCache::new(Duration::from_secs(3600));

async fn execute_with_cache(
    saga_request: SagaRequest,
) -> Result<SagaResult> {
    let cache_key = format!("saga:{}:{:?}", saga_request.type, saga_request.params);

    if let Some(cached) = cache.get(&cache_key).await {
        return Ok(cached);
    }

    let result = coordinator.execute(saga_request.steps).await?;
    cache.set(&cache_key, &result).await?;

    Ok(result)
}
```text
<!-- Code example in TEXT -->

### Optimization 2: Timeout Configuration

Prevent sagas from hanging.

```rust
<!-- Code example in RUST -->
let coordinator = SagaCoordinator::new(metadata, store)
    .with_step_timeout(Duration::from_secs(30))
    .with_saga_timeout(Duration::from_secs(300));
```text
<!-- Code example in TEXT -->

### Optimization 3: Connection Pooling

Reuse database connections.

```rust
<!-- Code example in RUST -->
let pool = PgPoolOptions::new()
    .max_connections(20)
    .acquire_timeout(Duration::from_secs(5))
    .connect(&database_url)
    .await?;

let store = PostgresSagaStore::with_pool(pool);
```text
<!-- Code example in TEXT -->

### Optimization 4: Batch Operations

Process multiple sagas together.

```rust
<!-- Code example in RUST -->
// Instead of: coordinator.execute(saga1), coordinator.execute(saga2), ...
// Use:
let results = coordinator.execute_batch(vec![saga1, saga2, saga3]).await?;
```text
<!-- Code example in TEXT -->

---

## Best Practices Summary

| Practice | Benefit |
|----------|---------|
| Always make operations idempotent | Enables safe retries and recovery |
| Use request IDs | Prevents duplicate side effects |
| Set reasonable timeouts | Prevents hanging sagas |
| Monitor saga metrics | Early detection of problems |
| Test compensation paths | Ensures rollback works |
| Keep steps small | Easier debugging and recovery |
| Document step dependencies | Clarifies ordering requirements |
| Use async/parallel where possible | Improves performance |

---

## Next Steps

1. **[Federation Sagas](../../integrations/federation/sagas.md)** - Integrate sagas with Apollo Federation
2. **[SAGA API Reference](../../reference/SAGA_API.md)** - Complete API reference
3. **[Quick Start](quick-start.md)** - Review basics if needed

---

**Last Updated:** 2026-01-29
**Maintainer:** FraiseQL Federation Team
