# Simplified CDC Architecture: Single Source of Truth

## Key Insight

Instead of building client response AND CDC event separately, we store **both** in the CDC event, then Rust extracts the client response from a dedicated field.

## Simplified Architecture

```
┌─────────────────────────────────────────────────────────────┐
│               POSTGRESQL DATABASE                           │
│                                                             │
│  1. app.create_customer(input_payload)                     │
│  2. core.create_customer() - business logic                │
│  3. app.log_mutation_event() - SINGLE source of truth      │
│     • Stores client_response (what client gets)            │
│     • Stores before/after (for CDC consumers)              │
│     • Stores metadata (for audit)                          │
│  4. RETURN event.client_response::text                     │
│                                                             │
└─────────────────┬───────────────────────────────────────────┘
                  │ (JSONB as text string)
                  ▼
┌─────────────────────────────────────────────────────────────┐
│                  RUST TRANSFORMER                           │
│  • Receives: client_response field directly                 │
│  • Transforms: snake_case → camelCase                       │
│  • Injects: __typename for GraphQL cache                    │
│  • Returns to client immediately                            │
└─────────────────────────────────────────────────────────────┘

                  ┌────────────────────────────┐
                  │   CDC CONSUMERS (Async)    │
                  │                            │
                  │  Read full event:          │
                  │  • before/after (diff)     │
                  │  • metadata (audit)        │
                  │  • client_response (FYI)   │
                  └────────────────────────────┘
```

## New CDC Event Structure

```sql
CREATE TABLE app.mutation_events (
    event_id BIGSERIAL PRIMARY KEY,
    event_type TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id UUID,
    operation TEXT NOT NULL,

    -- What client receives (extracted by Rust)
    client_response JSONB NOT NULL,

    -- What CDC consumers need (before/after diff)
    before_state JSONB,
    after_state JSONB,

    -- Audit metadata
    metadata JSONB,
    source JSONB,

    event_timestamp TIMESTAMPTZ DEFAULT NOW(),
    transaction_id BIGINT
);
```

## Example: Create Customer

### PostgreSQL Function (Simplified)

```sql
CREATE OR REPLACE FUNCTION app.create_customer(
    input_payload JSONB
) RETURNS TEXT AS $$
DECLARE
    v_customer_id UUID;
    v_customer_data JSONB;
    v_event_id BIGINT;
BEGIN
    -- 1. Execute business logic
    v_customer_id := core.create_customer(
        input_payload->>'email',
        input_payload->>'password_hash',
        input_payload->>'first_name',
        input_payload->>'last_name'
    );

    -- 2. Get complete customer data
    SELECT data INTO v_customer_data FROM tv_customer WHERE id = v_customer_id;

    -- 3. Log mutation event (SINGLE source of truth)
    v_event_id := app.log_mutation_event(
        'CUSTOMER_CREATED',              -- event_type
        'customer',                       -- entity_type
        v_customer_id,                    -- entity_id
        'CREATE',                         -- operation

        -- Client response (what GraphQL client receives)
        jsonb_build_object(
            'success', true,
            'code', 'SUCCESS',
            'message', 'Customer created successfully',
            'customer', v_customer_data
        ),

        -- CDC data (for event consumers)
        NULL,                             -- before_state
        v_customer_data,                  -- after_state

        -- Metadata (for audit)
        jsonb_build_object(
            'created_at', NOW(),
            'created_by', current_user,
            'source', 'graphql_api'
        )
    );

    -- 4. Return client_response directly (Rust will transform)
    RETURN (
        SELECT client_response::text
        FROM app.mutation_events
        WHERE event_id = v_event_id
    );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
```

### New log_mutation_event Function

```sql
CREATE OR REPLACE FUNCTION app.log_mutation_event(
    p_event_type TEXT,
    p_entity_type TEXT,
    p_entity_id UUID,
    p_operation TEXT,
    p_client_response JSONB,    -- NEW: what client receives
    p_before_state JSONB,
    p_after_state JSONB,
    p_metadata JSONB
) RETURNS BIGINT AS $$
DECLARE
    v_event_id BIGINT;
BEGIN
    INSERT INTO app.mutation_events (
        event_type,
        entity_type,
        entity_id,
        operation,
        client_response,
        before_state,
        after_state,
        metadata,
        source,
        transaction_id
    ) VALUES (
        p_event_type,
        p_entity_type,
        p_entity_id,
        p_operation,
        p_client_response,
        p_before_state,
        p_after_state,
        p_metadata,
        jsonb_build_object(
            'db', current_database(),
            'schema', 'public',
            'table', p_entity_type || 's',
            'txId', txid_current()
        ),
        txid_current()
    )
    RETURNING event_id INTO v_event_id;

    RETURN v_event_id;
END;
$$ LANGUAGE plpgsql;
```

## Complete Event in Database

```json
{
  "event_id": 12345,
  "event_type": "CUSTOMER_CREATED",
  "entity_type": "customer",
  "entity_id": "d4c8a3f2-1234-5678-9abc-def012345678",
  "operation": "CREATE",

  "client_response": {
    "success": true,
    "code": "SUCCESS",
    "message": "Customer created successfully",
    "customer": {
      "id": "d4c8a3f2-1234-5678-9abc-def012345678",
      "email": "alice@example.com",
      "first_name": "Alice",
      "last_name": "Johnson",
      "created_at": "2025-10-16T10:30:00Z"
    }
  },

  "before_state": null,
  "after_state": {
    "id": "d4c8a3f2-1234-5678-9abc-def012345678",
    "email": "alice@example.com",
    "first_name": "Alice",
    "last_name": "Johnson",
    "created_at": "2025-10-16T10:30:00Z"
  },

  "metadata": {
    "created_at": "2025-10-16T10:30:00Z",
    "created_by": "app_user",
    "source": "graphql_api"
  },

  "source": {
    "db": "ecommerce_dev",
    "schema": "public",
    "table": "customers",
    "txId": 98765
  },

  "event_timestamp": "2025-10-16T10:30:00.123Z",
  "transaction_id": 98765
}
```

## Rust Layer (Unchanged!)

Rust receives `client_response` directly as text:

```json
{
  "success": true,
  "code": "SUCCESS",
  "message": "Customer created successfully",
  "customer": {
    "id": "d4c8a3f2-1234-5678-9abc-def012345678",
    "email": "alice@example.com",
    "first_name": "Alice",
    "last_name": "Johnson",
    "created_at": "2025-10-16T10:30:00Z"
  }
}
```

Transforms to:

```json
{
  "success": true,
  "code": "SUCCESS",
  "message": "Customer created successfully",
  "customer": {
    "id": "d4c8a3f2-1234-5678-9abc-def012345678",
    "__typename": "Customer",
    "email": "alice@example.com",
    "firstName": "Alice",
    "lastName": "Johnson",
    "createdAt": "2025-10-16T10:30:00Z"
  }
}
```

## Key Simplifications

### Before (Dual-Path):
```sql
-- Build response
v_response := build_mutation_response(...);

-- Log CDC event
PERFORM log_cdc_event(...);

-- Return response
RETURN v_response;
```

### After (Single Source):
```sql
-- Log everything once
v_event_id := log_mutation_event(
    ...,
    client_response,  -- What client gets
    before_state,     -- What CDC needs
    after_state       -- What CDC needs
);

-- Return client_response directly
RETURN (SELECT client_response::text FROM mutation_events WHERE event_id = v_event_id);
```

## Benefits

### 1. **Single Source of Truth**
- One INSERT contains everything
- No risk of client_response vs CDC data diverging
- Simpler mental model

### 2. **Simpler PostgreSQL Functions**
- No `build_mutation_response()` helper needed
- No `PERFORM` for async logging
- Just: log event, return client_response field

### 3. **Easier Debugging**
- See EXACTLY what client received in CDC log
- Reproduce issues by replaying client_response
- Audit trail includes client response

### 4. **No Performance Change**
- Still single INSERT (~1ms)
- Still returns JSONB::text directly to Rust
- Still ultra-direct path (no Python parsing)

### 5. **Backward Compatible CDC Consumers**
- CDC consumers still get `before_state`/`after_state`
- Plus bonus: can see what client received (`client_response`)

## Trade-offs

### Slightly Larger Events
- Before: Only stored CDC diff (before/after)
- After: Also stores client_response (~duplicate of after_state)
- **Cost**: ~50-100 bytes per event (negligible)
- **Benefit**: Perfect audit trail + simpler code

### Event Log Query Cost
- SELECT from mutation_events on every mutation
- **Mitigation**: event_id is PRIMARY KEY (instant lookup)
- **Cost**: < 0.1ms (negligible vs 35ms business logic)

## Implementation Changes

### Files to Update:

1. **`0013_cdc_logging.sql`** - Change table schema:
   - Add `client_response JSONB NOT NULL`
   - Rename `payload` → separate `before_state`/`after_state`
   - Update `log_mutation_event()` signature

2. **All `*_with_cdc.sql` mutation functions**:
   - Replace `build_mutation_response()` + `PERFORM log_cdc_event()`
   - With single `log_mutation_event()` + return client_response

3. **Remove** `0012_mutation_utils.sql`:
   - No longer need `build_mutation_response()`
   - Everything goes through `log_mutation_event()`

## Example: Update Order (Simplified)

```sql
CREATE OR REPLACE FUNCTION app.update_order(
    order_id UUID,
    input_payload JSONB
) RETURNS TEXT AS $$
DECLARE
    v_before_data JSONB;
    v_after_data JSONB;
    v_event_id BIGINT;
BEGIN
    -- Get before state
    SELECT data INTO v_before_data FROM tv_order WHERE id = order_id;

    IF v_before_data IS NULL THEN
        -- Error case: still log as event!
        v_event_id := app.log_mutation_event(
            'ORDER_UPDATE_FAILED',
            'order',
            order_id,
            'UPDATE',
            jsonb_build_object(
                'success', false,
                'code', 'NOT_FOUND',
                'message', 'Order not found',
                'order_id', order_id
            ),
            NULL, NULL,
            jsonb_build_object('error', 'not_found')
        );

        RETURN (SELECT client_response::text FROM mutation_events WHERE event_id = v_event_id);
    END IF;

    -- Execute business logic
    PERFORM core.update_order(order_id, ...);

    -- Get after state
    SELECT data INTO v_after_data FROM tv_order WHERE id = order_id;

    -- Log mutation event (success case)
    v_event_id := app.log_mutation_event(
        'ORDER_UPDATED',
        'order',
        order_id,
        'UPDATE',
        jsonb_build_object(
            'success', true,
            'code', 'SUCCESS',
            'message', 'Order updated successfully',
            'order', v_after_data
        ),
        v_before_data,
        v_after_data,
        jsonb_build_object(
            'updated_by', current_user,
            'fields_updated', input_payload
        )
    );

    -- Return client response
    RETURN (SELECT client_response::text FROM mutation_events WHERE event_id = v_event_id);
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
```

## Recommendation

**YES, implement this simplification!**

### Why:
1. ✅ Simpler code (single INSERT instead of build + log)
2. ✅ Single source of truth (no divergence possible)
3. ✅ Better audit trail (includes exact client response)
4. ✅ Same performance (< 0.1ms overhead for event_id lookup)
5. ✅ More debuggable (replay exact client responses)

### Cost:
- Slightly larger CDC events (~50-100 bytes per mutation)
- This is negligible compared to benefits

### Migration Path:
1. Update `0013_cdc_logging.sql` with new schema
2. Update all mutation functions to use simplified pattern
3. Remove `0012_mutation_utils.sql` (no longer needed)
4. Update Python layer to expect TEXT return (already planned)

**This is a clear win for simplicity with no performance cost!**
