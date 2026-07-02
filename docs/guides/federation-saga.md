# Cross-Subgraph Mutations: Saga Pattern

> **Stable, behind the opt-in `saga` Cargo feature** on `fraiseql-federation` (#429).
> The full round-trip is wired and driven by the **runtime Rust API**
> `SagaCoordinator` + `SagaCoordinatorStep`: forward execution over local SQL or
> remote HTTPS (with optional mTLS), automatic compensation in reverse order (local or
> remote), concurrency-safe on-restart recovery (`SELECT … FOR UPDATE SKIP LOCKED`
> leasing), per-step retry-with-backoff + timeout, and cross-subgraph `@requires`
> pre-fetch.
>
> **Authoring:** sagas are constructed **programmatically at runtime** (build
> `SagaCoordinatorStep`s and pass them to `SagaCoordinator::create_saga`). The
> Python-decorator authoring and the `[federation.saga]` TOML shown below are a **planned
> convenience layer that is not yet wired** — they describe the target authoring
> ergonomics, not current behaviour.

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

The `SagaCoordinator` orchestrates the full saga: the **forward** phase (local or
remote steps) and, on failure, automatic **compensation** in reverse order. (The Python
decorator above is the planned authoring layer; today you build the equivalent
`SagaCoordinatorStep`s in Rust and call `create_saga`.)

## Execution Flow

```
mutation createOrder($input: OrderInput!) { ... }
    │
    ▼ SagaCoordinator (fraiseql-federation)
    │
    │ Forward phase (sequential):
    ├── Step 1: inventory-service.reserveInventory($input)
    │     └── OK → persist to tb_federation_saga_steps; advance to step 2
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
                          (partial rollback) → Failed + PartiallyCompensated
```

`SagaState` is `Pending`/`Executing`/`Completed`/`Failed`/`Compensating`/`Compensated`,
plus `Cancelled` (an operator cancels via `cancel_saga`, which rolls back completed steps
first). A rollback that only partly succeeds leaves the saga `Failed` and reports
`CompensationStatus::PartiallyCompensated` rather than fabricating a full `Compensated`.
State persists to the `tb_federation_saga*` tables before each transition, enabling
on-restart recovery.

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

Saga state is durable. `SagaRecoveryManager::run_iteration` /
`start_background_loop` (the `saga` feature) re-drive sagas that a crash or restart
left in-flight:

1. Each tick **claims** stuck (`Executing`) and pending sagas atomically via
   `UPDATE … WHERE pk_ IN (SELECT … FOR UPDATE SKIP LOCKED)`, leasing each to the
   recovering worker — so two recovery workers (or a worker racing a live coordinator)
   claim **disjoint** sets and never double-drive the same saga.
2. Claimed sagas are replayed through `execute_saga` to a terminal
   `Completed`/`Failed` state; a crashed worker's lease lapses and its claims become
   reclaimable.
3. Terminal, stale sagas are cleaned up.

No manual intervention is required for crash recovery.

## Observability

```bash
# Check active saga states
SELECT state, COUNT(*) FROM tb_federation_sagas GROUP BY state;

# Per-step progress for one saga
SELECT step_number, subgraph, mutation_type, state
FROM tb_federation_saga_steps fss
JOIN tb_federation_sagas fs ON fss.saga_pk_ = fs.pk_
WHERE fs.id = '<saga-id>' ORDER BY step_number;
```

> Prometheus saga metrics (`fraiseql_saga_steps_total`, `_compensations_total`,
> `_duration_seconds`) are **planned** — not yet emitted. Use the tables above and the
> structured `tracing` spans/logs the coordinator emits for now.

## Configuration

> **Planned — not yet wired.** The `[federation.saga]` TOML keys below are the target
> config surface. Today the equivalent knobs are set programmatically: per-step retry and
> timeout via `RetryPolicy` (`SagaCoordinator::with_retry_policy`), the compensation
> strategy via `CompensationStrategy`, remote dispatch via `with_http_client` /
> `with_http_client_mtls` / `with_subgraph`, and `@requires` pre-fetch via
> `with_entity_resolver`.

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
- Compensation is **best-effort**: if a step's inverse fails (or it has no registered
  compensation), the saga is left `Failed` and reported `PartiallyCompensated` rather than
  a fabricated full `Compensated` — manual intervention may be required.
- The `tb_federation_saga*` tables grow over time; prune terminal sagas with the store's
  `cleanup_stale_sagas(hours)` / `delete_completed_sagas()` (the recovery loop also cleans
  up stale terminal sagas each tick).
