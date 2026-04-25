# Cross-Subgraph Mutations: Saga Pattern

The saga pattern coordinates mutations that must write to multiple subgraphs. Each step
has a forward action and a compensation action. If any step fails, compensation runs in
reverse order to roll back completed steps.

## When to Use

Use the saga pattern when a single GraphQL mutation touches data in more than one subgraph
and you need best-effort rollback on failure. Typical examples:

- `createOrder`: decrement inventory → charge payment → create shipment
- `transferFunds`: debit source account → credit destination account
- `enrollUser`: create profile → provision resources → send welcome email

**When NOT to use**: if you only need fire-and-forget (no rollback) or if all writes
are local to a single subgraph, use a normal mutation instead.

## Schema Definition (Python authoring)

```python
import fraiseql

@fraiseql.mutation(
    sql_source="create_order",
    operation="create",
    saga=[
        fraiseql.SagaStep(
            subgraph="inventory-service",
            forward_operation="reserveInventory",
            compensation_operation="releaseInventory",
        ),
        fraiseql.SagaStep(
            subgraph="billing-service",
            forward_operation="chargePayment",
            compensation_operation="refundPayment",
        ),
        fraiseql.SagaStep(
            subgraph="fulfillment-service",
            forward_operation="createShipment",
            compensation_operation="cancelShipment",
        ),
    ],
)
def create_order(self, *, order_input: OrderInput) -> OrderResult:
    ...
```

The compiled schema embeds the saga plan; the FraiseQL runtime orchestrates the
forward and compensation phases without additional application code.

## Execution Flow

```
mutation createOrder($input: OrderInput!) { ... }
    │
    ▼ SagaCoordinator (fraiseql-federation)
    │
    │ Forward phase (sequential):
    ├── Step 1: inventory-service.reserveInventory($input)
    │     └── OK → write to tb_saga_log; advance to step 2
    ├── Step 2: billing-service.chargePayment($input)
    │     └── FAIL → trigger compensation
    │
    │ Compensation phase (reverse order):
    └── Step 1 compensation: inventory-service.releaseInventory($input)
          └── OK → saga state = Compensated; return error to client
```

The client receives a GraphQL error indicating which step failed and whether
compensation succeeded. The `saga_id` is included in the error extensions for
debugging.

## State Machine

```
Pending ──► Executing ──► Completed
                │
                ▼
             Failed ──► Compensating ──► Compensated
                                │
                                ▼
                          CompensationFailed  ← all compensations ran, some failed
```

States are persisted to `tb_saga_log` before each transition, enabling recovery on
server restart without replaying steps that already completed.

## Compensation Contract

Each compensation function must:

1. **Accept the same input** as the forward step (FraiseQL passes original variables)
2. **Undo the forward step completely** (e.g., delete what was created)
3. **Be idempotent** — safe to call multiple times (network retries happen)
4. **Return `mutation_response`** type (same as any mutation)
5. **Not raise fatal errors** — compensation failures are collected and reported,
   but do not prevent other compensations from running

Example compensation SQL function:

```sql
-- Forward: reserveInventory
CREATE OR REPLACE FUNCTION reserve_inventory(p_order_id UUID, p_items JSONB)
RETURNS mutation_response AS $$
BEGIN
    -- ... decrement stock ...
    RETURN ROW(p_order_id, 'inventory_reservation', now(), TRUE, NULL)::mutation_response;
END;
$$ LANGUAGE plpgsql;

-- Compensation: releaseInventory (must be idempotent)
CREATE OR REPLACE FUNCTION release_inventory(p_order_id UUID, p_items JSONB)
RETURNS mutation_response AS $$
BEGIN
    -- Use ON CONFLICT DO NOTHING or check-then-update to be idempotent
    UPDATE inventory SET reserved = reserved - qty
    FROM jsonb_array_elements(p_items) AS item(qty, sku_id)
    WHERE sku = item->>'sku_id'
    AND reserved >= (item->>'qty')::int;
    -- If already released (idempotent call), this is a no-op — that is correct.
    RETURN ROW(p_order_id, 'inventory_release', now(), TRUE, NULL)::mutation_response;
END;
$$ LANGUAGE plpgsql;
```

## Recovery on Restart

Saga state is durable. If FraiseQL restarts mid-saga:

1. `SagaRecoveryManager` scans `tb_saga_log` for sagas in `Executing` or `Compensating`
2. In-flight sagas are resumed from the last committed step
3. Steps marked `Completed` in the log are skipped (not re-executed)
4. The recovery scan runs at server startup, before accepting traffic

No manual intervention is required for crash recovery.

## Observability

```bash
# Check active saga states
SELECT state, COUNT(*) FROM tb_saga_log GROUP BY state;

# Prometheus metrics
fraiseql_saga_steps_total{subgraph="billing-service", status="success"}
fraiseql_saga_steps_total{subgraph="billing-service", status="failed"}
fraiseql_saga_compensations_total          # how often rollback is triggered
fraiseql_saga_duration_seconds{quantile}   # end-to-end saga latency
```

## Configuration

```toml
# fraiseql.toml
[federation.saga]
enabled = true
# Maximum time to wait for a single step before marking it timed out
step_timeout_secs = 30
# Maximum time to wait for a single compensation step
compensation_timeout_secs = 15
# Number of retry attempts for transient step failures before giving up
step_max_retries = 2
```

## Limitations

- Sagas are **eventually consistent**, not strongly consistent. Between steps,
  partial state is visible to other queries (e.g., inventory decremented but payment
  not yet charged).
- Compensation is **best-effort**: if a compensation fails, `CompensationFailed` state
  is recorded and an alert is raised — manual intervention may be required.
- Maximum saga size: 50 steps (configurable via `federation.saga.max_steps`).
- The `tb_saga_log` table grows over time; prune completed sagas with:
  `DELETE FROM tb_saga_log WHERE state IN ('Completed','Compensated') AND updated_at < NOW() - INTERVAL '30 days'`
