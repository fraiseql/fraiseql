# Database Patterns

## The tv_ Pattern: Projected Tables for GraphQL

### Overview

The **tv_** (table view) pattern is FraiseQL's foundational architecture for efficient GraphQL queries. Despite the name, `tv_` tables are **actual PostgreSQL tables** (not VIEWs), serving as denormalized projections of normalized write tables.

**Key Principle**: Write to normalized tables, read from denormalized tv_ projections.

### Structure

Every `tv_` table follows this exact structure:

```sql
CREATE TABLE tv_entity_name (
    -- Real columns for efficient filtering and indexing
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,

    -- Additional filter columns (indexed, fast queries)
    status TEXT,
    created_at TIMESTAMPTZ,
    user_id UUID,
    -- ... other frequently filtered fields

    -- Complete denormalized payload as JSONB
    data JSONB NOT NULL,

    -- Metadata
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Indexes on real columns (fast filtering)
CREATE INDEX idx_tv_entity_tenant ON tv_entity_name (tenant_id, created_at DESC);
CREATE INDEX idx_tv_entity_status ON tv_entity_name (status, tenant_id);

-- Optional: GIN index for JSONB queries
CREATE INDEX idx_tv_entity_data ON tv_entity_name USING GIN (data);
```

### Why This Pattern?

| Aspect | tv_ Table (Actual Table) | Traditional VIEW | Materialized VIEW |
|--------|-------------------------|------------------|-------------------|
| **Query speed** | Fastest (indexed) | Slow (computes on read) | Fast (pre-computed) |
| **Filtering** | Real columns (indexed) | Computed columns | Pre-computed |
| **Updates** | Trigger-based | N/A | Manual REFRESH |
| **Consistency** | Event-driven | Always fresh | Scheduled refresh |
| **GraphQL fit** | Perfect (JSONB data) | Complex queries | Static snapshots |

**Answer**: `tv_` tables are **real tables** with indexed columns for fast filtering and JSONB payloads for complete nested data.

### Example: Orders

**Normalized Write Tables** (OLTP, referential integrity with trinity pattern):
```sql
CREATE TABLE tb_order (
    pk_order SERIAL PRIMARY KEY,          -- Internal fast joins
    id UUID UNIQUE NOT NULL DEFAULT gen_random_uuid(),  -- Public API
    identifier TEXT UNIQUE,                -- Optional human-readable
    tenant_id UUID NOT NULL,
    user_id UUID NOT NULL,
    status TEXT NOT NULL,
    total DECIMAL(10,2),
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE tb_order_item (
    pk_order_item SERIAL PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT gen_random_uuid(),
    order_id UUID NOT NULL,                -- References tb_order(id), not pk_order
    product_id UUID NOT NULL,
    quantity INT NOT NULL,
    price DECIMAL(10,2),
    FOREIGN KEY (order_id) REFERENCES tb_order(id)
);
```

**Denormalized Read Table** (OLAP, GraphQL-optimized):
```sql
CREATE TABLE tv_order (
    -- Filter columns (indexed for fast WHERE clauses)
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    status TEXT,
    user_id UUID,
    total DECIMAL(10,2),
    created_at TIMESTAMPTZ,

    -- Complete nested payload (GraphQL-ready)
    data JSONB NOT NULL,

    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Essential indexes
CREATE INDEX idx_tv_order_tenant_created
    ON tv_order (tenant_id, created_at DESC);
CREATE INDEX idx_tv_order_status
    ON tv_order (status, tenant_id)
    WHERE status != 'cancelled';  -- Partial index for active orders
```

**Example `data` JSONB**:
```json
{
  "__typename": "Order",
  "id": "d613dfba-3440-4c90-bb7b-877175621e08",
  "status": "shipped",
  "total": 299.99,
  "createdAt": "2025-10-09T10:30:00Z",
  "user": {
    "id": "a1b2c3d4-...",
    "email": "customer@example.com",
    "name": "John Doe"
  },
  "items": [
    {
      "id": "item-1",
      "productName": "Widget Pro",
      "quantity": 2,
      "price": 149.99
    }
  ],
  "shipping": {
    "address": "123 Main St",
    "trackingNumber": "1Z999AA10123456784"
  }
}
```

### Synchronization Pattern

**Generated JSONB Columns** (not manual refresh):

tv_ tables use PostgreSQL's generated columns to automatically maintain denormalized JSONB data. This provides real-time consistency without manual refresh calls.

**Step 1: Create tv_ Table with Generated Column**

```sql
-- tv_ table with generated JSONB column (auto-updates on write)
CREATE TABLE tv_order (
    -- GraphQL identifier (matches tb_order.id)
    id UUID PRIMARY KEY,

    -- Filter columns (indexed for fast WHERE clauses)
    tenant_id UUID NOT NULL,
    status TEXT,
    user_id UUID,
    total DECIMAL(10,2),
    created_at TIMESTAMPTZ,

    -- Complete denormalized payload (auto-generated)
    data JSONB GENERATED ALWAYS AS (
        jsonb_build_object(
            '__typename', 'Order',
            'id', id,
            'status', status,
            'total', total,
            'createdAt', created_at,
            'user', (
                SELECT jsonb_build_object(
                    'id', u.id,
                    'email', u.email,
                    'name', u.name
                )
                FROM tb_user u
                WHERE u.id = tv_order.user_id
            ),
            'items', COALESCE(
                (
                    SELECT jsonb_agg(jsonb_build_object(
                        'id', i.id,
                        'productName', i.product_name,
                        'quantity', i.quantity,
                        'price', i.price
                    ) ORDER BY i.created_at)
                    FROM tb_order_item i
                    WHERE i.order_id = tv_order.id
                ),
                '[]'::jsonb
            )
        )
    ) STORED,

    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Populate from existing tb_ data
INSERT INTO tv_order (id, tenant_id, status, user_id, total, created_at)
SELECT id, tenant_id, status, user_id, total, created_at
FROM tb_order;
```

**Step 2: Automatic Synchronization via Triggers**

```sql
-- Trigger function to sync tb_order changes to tv_order
CREATE OR REPLACE FUNCTION sync_tv_order()
RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        -- Insert new row into tv_order
        INSERT INTO tv_order (id, tenant_id, status, user_id, total, created_at)
        VALUES (NEW.id, NEW.tenant_id, NEW.status, NEW.user_id, NEW.total, NEW.created_at);
        RETURN NEW;

    ELSIF TG_OP = 'UPDATE' THEN
        -- Update tv_order (data column auto-regenerates)
        UPDATE tv_order SET
            tenant_id = NEW.tenant_id,
            status = NEW.status,
            user_id = NEW.user_id,
            total = NEW.total,
            created_at = NEW.created_at,
            updated_at = NOW()
        WHERE id = NEW.id;
        RETURN NEW;

    ELSIF TG_OP = 'DELETE' THEN
        -- Remove from tv_order
        DELETE FROM tv_order WHERE id = OLD.id;
        RETURN OLD;
    END IF;

    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

-- Attach trigger to tb_order
CREATE TRIGGER trg_sync_tv_order
AFTER INSERT OR UPDATE OR DELETE ON tb_order
FOR EACH ROW EXECUTE FUNCTION sync_tv_order();

-- Also sync when related tables change (user info, order items)
CREATE OR REPLACE FUNCTION sync_tv_order_on_related_changes()
RETURNS TRIGGER AS $$
BEGIN
    -- When user changes, update all their orders
    UPDATE tv_order SET updated_at = NOW()
    WHERE user_id = COALESCE(NEW.id, OLD.id);

    -- When order items change, update the order
    IF TG_TABLE_NAME = 'tb_order_item' THEN
        UPDATE tv_order SET updated_at = NOW()
        WHERE id = COALESCE(NEW.order_id, OLD.order_id);
    END IF;

    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_sync_tv_order_on_user_change
AFTER UPDATE ON tb_user
FOR EACH ROW EXECUTE FUNCTION sync_tv_order_on_related_changes();

CREATE TRIGGER trg_sync_tv_order_on_item_change
AFTER INSERT OR UPDATE OR DELETE ON tb_order_item
FOR EACH ROW EXECUTE FUNCTION sync_tv_order_on_related_changes();
```

**Benefits of Generated Columns:**
- ✅ **Real-time consistency**: Data always up-to-date
- ✅ **No manual refresh**: Automatic via triggers
- ✅ **Performance**: No refresh function calls in mutations
- ✅ **Reliability**: PostgreSQL manages the generation

### GraphQL Query Pattern

**GraphQL Query**:
```graphql
query GetOrders($status: String) {
  orders(
    filters: {status: $status}
    orderBy: {field: "createdAt", direction: DESC}
    limit: 50
  ) {
    id
    status
    total
    user {
      email
      name
    }
    items {
      productName
      quantity
      price
    }
  }
}
```

**Generated SQL** (single query, no N+1):
```sql
SELECT data
FROM tv_order
WHERE tenant_id = $1
  AND status = $2
ORDER BY created_at DESC
LIMIT 50;
```

**Performance**:
- **50 orders with nested users + items**: Single query, 2-5ms
- **Traditional approach (N+1)**: 1 + 50 + (50 × avg_items) queries, 100-500ms
- **Speedup**: 20-100x faster

### Design Rules for tv_ Tables

#### 1. Real Columns for Filtering

**Include as real columns** (not just in JSONB):
- Primary key (`id`)
- Tenant isolation (`tenant_id`)
- Common filters (`status`, `user_id`, `created_at`)
- Sort keys (`created_at`, `updated_at`, `priority`)

**Why**: PostgreSQL can't efficiently index inside JSONB for complex queries.

```sql
-- ✅ GOOD: Real column with index
CREATE TABLE tv_order (
    id UUID PRIMARY KEY,       -- Required for GraphQL
    status TEXT,
    created_at TIMESTAMPTZ,
    data JSONB
);
CREATE INDEX idx_status_created ON tv_order (status, created_at DESC);

-- Query: Fast (uses index)
SELECT data FROM tv_order
WHERE status = 'shipped'
ORDER BY created_at DESC;

-- ❌ BAD: Status only in JSONB
CREATE TABLE tv_order_bad (
    data JSONB
);

-- Query: Slow (sequential scan)
SELECT data FROM tv_order_bad
WHERE data->>'status' = 'shipped'
ORDER BY (data->>'createdAt')::timestamptz DESC;
```

#### 2. JSONB `data` Column Structure

**Requirements**:
- Complete GraphQL response (all nested data)
- Include `__typename` for GraphQL unions/interfaces
- Use camelCase field names (GraphQL convention)
- Pre-compute expensive aggregations

**Example Structure**:
```json
{
  "__typename": "Order",          // ✅ Required for GraphQL
  "id": "...",                     // ✅ Always include
  "status": "shipped",             // ✅ Duplicate of real column (for consistency)
  "createdAt": "2025-10-09...",    // ✅ ISO 8601 format
  "user": { ... },                 // ✅ Complete nested object
  "items": [ ... ],                // ✅ Complete nested array
  "itemCount": 3,                  // ✅ Pre-computed aggregation
  "totalAmount": 299.99            // ✅ Pre-computed sum
}
```

#### 3. Indexing Strategy

**Standard Indexes** (every tv_ table):
```sql
-- Tenant + primary sort key (most common query)
CREATE INDEX idx_tv_entity_tenant_created
    ON tv_entity (tenant_id, created_at DESC);

-- Status-based filtering
CREATE INDEX idx_tv_entity_status
    ON tv_entity (status, tenant_id);

-- Optional: Partial indexes for hot paths
CREATE INDEX idx_tv_entity_active
    ON tv_entity (tenant_id, created_at DESC)
    WHERE status IN ('pending', 'active', 'processing');
```

**Advanced**: GIN index for JSONB queries (use sparingly):
```sql
-- Only if you query JSONB fields directly
CREATE INDEX idx_tv_entity_data_gin
    ON tv_entity USING GIN (data jsonb_path_ops);

-- Allows queries like:
SELECT * FROM tv_entity
WHERE data @> '{"user": {"role": "admin"}}';
```

#### 4. Naming Conventions

| Pattern | Example | Purpose |
|---------|---------|---------|
| `tb_*` | `tb_order` | Write tables (normalized, OLTP) |
| `tv_*` | `tv_order` | Read tables (denormalized, OLAP) |
| `v_*` | `v_order_summary` | Actual VIEWs (computed on read) |
| `mv_*` | `mv_daily_stats` | Materialized VIEWs (scheduled refresh) |

### Performance Characteristics

**tv_ Table Query Performance**:
```sql
-- Filtering on indexed real columns: 0.5-2ms
SELECT data FROM tv_order
WHERE tenant_id = $1
  AND status = 'shipped'
  AND created_at > NOW() - INTERVAL '7 days'
ORDER BY created_at DESC
LIMIT 50;

-- vs. Traditional JOIN approach: 50-200ms
SELECT o.*, u.email, array_agg(i.*)
FROM tb_order o
JOIN tb_user u ON u.id = o.user_id
LEFT JOIN tb_order_item i ON i.order_id = o.id
WHERE o.tenant_id = $1 AND o.status = 'shipped'
GROUP BY o.id, u.email;
```

**Trade-offs**:

| Aspect | Benefit | Cost |
|--------|---------|------|
| **Read speed** | 10-100x faster | N/A |
| **Write complexity** | N/A | Trigger overhead (2-10ms per write) |
| **Storage** | Duplicate data (2-3x) | Disk space |
| **Consistency** | Eventual (trigger-based) | Not real-time |

**Recommendation**: Use tv_ tables for all GraphQL queries. The read performance gain (10-100x) far outweighs the storage cost.

## Mutation Structure Pattern

### Overview

FraiseQL mutations follow a consistent 5-step pattern that ensures data integrity, audit trails, and synchronized tv_ tables.

**Standard Mutation Flow**:
1. **Validation** - Check business rules not enforced by types
2. **Existence Check** - Verify required records exist
3. **Business Logic** - Perform the mutation on tb_ tables
4. **Refresh tv_** - Rebuild denormalized projections
5. **Return Result** - Structured response with change tracking

### Complete Example: Update Order

**SQL Function Structure**:

```sql
CREATE OR REPLACE FUNCTION update_order(
    p_tenant_id UUID,
    p_user_id UUID,
    p_order_id UUID,
    p_status TEXT,
    p_notes TEXT DEFAULT NULL
)
RETURNS TABLE(
    id UUID,
    status TEXT,
    updated_fields TEXT[],
    message TEXT,
    object_data JSONB,
    extra_metadata JSONB
) AS $$
DECLARE
    v_old_order RECORD;
    v_updated_fields TEXT[] := '{}';
    v_change_status TEXT;
BEGIN
    -- =====================================================================
    -- STEP 1: VALIDATION
    -- =====================================================================

    -- Validate status transition
    IF p_status NOT IN ('pending', 'confirmed', 'shipped', 'delivered', 'cancelled') THEN
        RAISE EXCEPTION 'Invalid status: %. Must be one of: pending, confirmed, shipped, delivered, cancelled', p_status;
    END IF;

    -- Additional business rules
    IF p_status = 'shipped' AND p_notes IS NULL THEN
        RAISE EXCEPTION 'Tracking notes required when shipping order';
    END IF;

    -- =====================================================================
    -- STEP 2: EXISTENCE CHECK
    -- =====================================================================

    -- Check if order exists and belongs to tenant
    SELECT * INTO v_old_order
    FROM tb_order
    WHERE id = p_order_id
      AND tenant_id = p_tenant_id;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'Order % not found for tenant %', p_order_id, p_tenant_id;
    END IF;

    -- Validate state transitions
    IF v_old_order.status = 'cancelled' THEN
        RAISE EXCEPTION 'Cannot modify cancelled order';
    END IF;

    -- =====================================================================
    -- STEP 3: BUSINESS LOGIC (Mutation on tb_ tables)
    -- =====================================================================

    -- Track which fields changed
    IF v_old_order.status != p_status THEN
        v_updated_fields := array_append(v_updated_fields, 'status');
    END IF;

    IF COALESCE(v_old_order.notes, '') != COALESCE(p_notes, '') THEN
        v_updated_fields := array_append(v_updated_fields, 'notes');
    END IF;

    -- Determine change status
    IF array_length(v_updated_fields, 1) = 0 THEN
        v_change_status := 'noop:no_changes';
    ELSE
        v_change_status := 'updated';
    END IF;

    -- Perform the update
    UPDATE tb_order
    SET
        status = p_status,
        notes = p_notes,
        updated_at = NOW(),
        updated_by = p_user_id
    WHERE id = p_order_id;

    -- =====================================================================
    -- STEP 4: REFRESH tv_ TABLE
    -- =====================================================================

    -- Explicitly refresh the denormalized projection
    PERFORM refresh_tv_order(p_order_id);

    -- =====================================================================
    -- STEP 5: RETURN RESULT (with audit logging)
    -- =====================================================================

    -- Log to entity_change_log
    INSERT INTO core.tb_entity_change_log
        (tenant_id, user_id, object_type, object_id,
         modification_type, change_status, object_data, extra_metadata)
    VALUES
        (p_tenant_id, p_user_id, 'order', p_order_id,
         'UPDATE', v_change_status,
         jsonb_build_object(
             'before', row_to_json(v_old_order),
             'after', (SELECT row_to_json(tb_order) FROM tb_order WHERE id = p_order_id),
             'op', 'u'
         ),
         jsonb_build_object(
             'updated_fields', v_updated_fields,
             'input_params', jsonb_build_object(
                 'status', p_status,
                 'notes', p_notes
             )
         ));

    -- Return structured result
    RETURN QUERY
    SELECT
        p_order_id as id,
        v_change_status as status,
        v_updated_fields as updated_fields,
        format('Order updated: %s', array_to_string(v_updated_fields, ', ')) as message,
        (SELECT data FROM tv_order WHERE id = p_order_id) as object_data,
        jsonb_build_object('updated_fields', v_updated_fields) as extra_metadata;

END;
$$ LANGUAGE plpgsql;
```

### GraphQL Resolver Integration

**Python Resolver**:

```python
from uuid import UUID
from fraiseql import mutation
from fraiseql.db import execute_mutation

@mutation
async def update_order(
    info,
    id: UUID,
    status: str,
    notes: str | None = None
) -> MutationLogResult:
    """Update order status."""
    db = info.context["db"]
    tenant_id = info.context["tenant_id"]
    user_id = info.context["user_id"]

    # Call SQL function (5-step pattern executed)
    result = await db.execute_mutation(
        """
        SELECT * FROM update_order(
            p_tenant_id := $1,
            p_user_id := $2,
            p_order_id := $3,
            p_status := $4,
            p_notes := $5
        )
        """,
        tenant_id,
        user_id,
        id,
        status,
        notes
    )

    return MutationLogResult(
        status=result["status"],
        message=result["message"],
        op="update",
        entity="order",
        payload_before=result["object_data"].get("before"),
        payload_after=result["object_data"].get("after"),
        extra_metadata=result["extra_metadata"]
    )
```

### Create Pattern

**Create follows same 5-step pattern**:

```sql
CREATE OR REPLACE FUNCTION create_order(
    p_tenant_id UUID,
    p_user_id UUID,
    p_customer_id UUID,
    p_items JSONB  -- Array of {product_id, quantity, price}
)
RETURNS TABLE(
    id UUID,
    status TEXT,
    message TEXT,
    object_data JSONB
) AS $$
DECLARE
    v_order_id UUID;
    v_item JSONB;
BEGIN
    -- STEP 1: VALIDATION
    IF jsonb_array_length(p_items) = 0 THEN
        RAISE EXCEPTION 'Order must contain at least one item';
    END IF;

    -- Validate all products exist
    FOR v_item IN SELECT * FROM jsonb_array_elements(p_items)
    LOOP
        IF NOT EXISTS (SELECT 1 FROM tb_product WHERE id = (v_item->>'product_id')::UUID) THEN
            RAISE EXCEPTION 'Product % not found', v_item->>'product_id';
        END IF;
    END LOOP;

    -- STEP 2: EXISTENCE CHECK
    IF NOT EXISTS (SELECT 1 FROM tb_user WHERE id = p_customer_id AND tenant_id = p_tenant_id) THEN
        RAISE EXCEPTION 'Customer % not found', p_customer_id;
    END IF;

    -- STEP 3: BUSINESS LOGIC
    v_order_id := gen_random_uuid();

    -- Insert into tb_order
    INSERT INTO tb_order (id, tenant_id, user_id, status, created_by)
    VALUES (v_order_id, p_tenant_id, p_customer_id, 'pending', p_user_id);

    -- Insert items
    FOR v_item IN SELECT * FROM jsonb_array_elements(p_items)
    LOOP
        INSERT INTO tb_order_item (id, order_id, product_id, quantity, price)
        VALUES (
            gen_random_uuid(),
            v_order_id,
            (v_item->>'product_id')::UUID,
            (v_item->>'quantity')::INT,
            (v_item->>'price')::DECIMAL
        );
    END LOOP;

    -- Update total
    UPDATE tb_order
    SET total = (
        SELECT SUM(quantity * price)
        FROM tb_order_item
        WHERE order_id = v_order_id
    )
    WHERE id = v_order_id;

    -- STEP 4: REFRESH tv_
    PERFORM refresh_tv_order(v_order_id);

    -- STEP 5: RETURN RESULT
    INSERT INTO core.tb_entity_change_log
        (tenant_id, user_id, object_type, object_id,
         modification_type, change_status, object_data)
    VALUES
        (p_tenant_id, p_user_id, 'order', v_order_id,
         'INSERT', 'new',
         jsonb_build_object(
             'after', (SELECT row_to_json(tb_order) FROM tb_order WHERE id = v_order_id),
             'op', 'c'
         ));

    RETURN QUERY
    SELECT
        v_order_id as id,
        'new'::TEXT as status,
        'Order created successfully' as message,
        (SELECT data FROM tv_order WHERE id = v_order_id) as object_data;

END;
$$ LANGUAGE plpgsql;
```

### Delete Pattern

**Delete with soft-delete support**:

```sql
CREATE OR REPLACE FUNCTION delete_order(
    p_tenant_id UUID,
    p_user_id UUID,
    p_order_id UUID
)
RETURNS TABLE(
    id UUID,
    status TEXT,
    message TEXT
) AS $$
DECLARE
    v_old_order RECORD;
BEGIN
    -- STEP 1: VALIDATION
    -- (No specific validation for delete)

    -- STEP 2: EXISTENCE CHECK
    SELECT * INTO v_old_order
    FROM tb_order
    WHERE id = p_order_id
      AND tenant_id = p_tenant_id;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'Order % not found', p_order_id;
    END IF;

    -- Check if already deleted
    IF v_old_order.deleted_at IS NOT NULL THEN
        RETURN QUERY
        SELECT
            p_order_id as id,
            'noop:already_deleted'::TEXT as status,
            'Order already deleted' as message;
        RETURN;
    END IF;

    -- STEP 3: BUSINESS LOGIC (soft delete)
    UPDATE tb_order
    SET
        deleted_at = NOW(),
        deleted_by = p_user_id
    WHERE id = p_order_id;

    -- STEP 4: REFRESH tv_ (or remove from tv_)
    DELETE FROM tv_order WHERE id = p_order_id;

    -- STEP 5: RETURN RESULT
    INSERT INTO core.tb_entity_change_log
        (tenant_id, user_id, object_type, object_id,
         modification_type, change_status, object_data)
    VALUES
        (p_tenant_id, p_user_id, 'order', p_order_id,
         'DELETE', 'deleted',
         jsonb_build_object(
             'before', row_to_json(v_old_order),
             'op', 'd'
         ));

    RETURN QUERY
    SELECT
        p_order_id as id,
        'deleted'::TEXT as status,
        'Order deleted successfully' as message;

END;
$$ LANGUAGE plpgsql;
```

### Batch Refresh Pattern

**When mutations affect multiple tv_ rows**:

```sql
-- Refresh function accepting multiple IDs
CREATE OR REPLACE FUNCTION refresh_tv_order_batch(p_order_ids UUID[])
RETURNS void AS $$
BEGIN
    INSERT INTO tv_order (id, tenant_id, status, user_id, total, created_at, data)
    SELECT
        o.id,
        o.tenant_id,
        o.status,
        o.user_id,
        o.total,
        o.created_at,
        jsonb_build_object(
            '__typename', 'Order',
            'id', o.id,
            -- ... complete JSONB construction
        ) as data
    FROM tb_order o
    WHERE o.id = ANY(p_order_ids)
    ON CONFLICT (id) DO UPDATE SET
        status = EXCLUDED.status,
        data = EXCLUDED.data,
        updated_at = NOW();
END;
$$ LANGUAGE plpgsql;

-- Use in mutations affecting multiple orders
CREATE OR REPLACE FUNCTION bulk_ship_orders(
    p_tenant_id UUID,
    p_order_ids UUID[]
)
RETURNS TABLE(processed_count INT) AS $$
BEGIN
    -- STEP 3: Update all orders
    UPDATE tb_order
    SET status = 'shipped', updated_at = NOW()
    WHERE id = ANY(p_order_ids)
      AND tenant_id = p_tenant_id
      AND status = 'confirmed';

    -- STEP 4: Batch refresh
    PERFORM refresh_tv_order_batch(p_order_ids);

    -- STEP 5: Return count
    RETURN QUERY SELECT array_length(p_order_ids, 1) as processed_count;
END;
$$ LANGUAGE plpgsql;
```

### Best Practices

**Validation**:
- Validate business rules not enforced by database constraints
- Check state transitions (e.g., can't ship a cancelled order)
- Validate related entity existence
- Return clear error messages

**Existence Checks**:
- Always verify record exists before mutation
- Check tenant ownership (multi-tenancy security)
- Detect NOOP cases early (no changes to apply)

**Business Logic**:
- Track changed fields for audit trail
- Use atomic operations (single transaction)
- Handle cascading updates (e.g., recalculate totals)

**tv_ Refresh**:
- Always call refresh after tb_ mutations
- Use batch refresh for bulk operations
- Consider: DELETE from tv_ for soft-deleted records

**Return Results**:
- Always log to entity_change_log
- Return structured mutation result
- Include before/after snapshots
- Track no-op operations (important for debugging)

### Error Handling

**Structured Exceptions**:

```sql
-- Custom exception types
CREATE OR REPLACE FUNCTION update_order(...)
RETURNS TABLE(...) AS $$
BEGIN
    -- Validation errors
    IF p_status NOT IN (...) THEN
        RAISE EXCEPTION 'validation:invalid_status'
            USING DETAIL = format('Invalid status: %s', p_status);
    END IF;

    -- Not found errors
    IF NOT FOUND THEN
        RAISE EXCEPTION 'not_found:order'
            USING DETAIL = format('Order %s not found', p_order_id);
    END IF;

    -- Business rule violations
    IF v_old_order.status = 'shipped' THEN
        RAISE EXCEPTION 'conflict:already_shipped'
            USING DETAIL = 'Cannot modify shipped orders';
    END IF;

EXCEPTION
    WHEN OTHERS THEN
        -- Log error
        INSERT INTO core.tb_entity_change_log
            (tenant_id, object_type, object_id,
             modification_type, change_status, object_data)
        VALUES
            (p_tenant_id, 'order', p_order_id,
             'UPDATE', format('failed:%s', SQLERRM),
             jsonb_build_object('error', SQLERRM));
        RAISE;
END;
$$ LANGUAGE plpgsql;
```

**Benefits of 5-Step Pattern**:
- ✅ Consistent mutation structure across codebase
- ✅ Automatic audit trail for compliance
- ✅ tv_ tables always synchronized
- ✅ Clear error messages with context
- ✅ Explicit validation and existence checks
- ✅ No silent failures (NOOP operations tracked)

## JSONB Composition for N+1 Prevention

**Problem**: Nested GraphQL queries result in N+1 database queries.

**Traditional Approach** (N+1 problem):
```graphql
query {
  users {
    id
    name
    posts {  # Triggers 1 query per user
      id
      title
    }
  }
}
```

**Solution**: JSONB aggregation in database views.

**View Design**:
```sql
CREATE VIEW v_users_with_posts AS
SELECT
  u.id,
  u.email,
  u.name,
  u.created_at,
  jsonb_build_object(
    'id', u.id,
    'email', u.email,
    'name', u.name,
    'createdAt', u.created_at,
    'posts', (
      SELECT jsonb_agg(jsonb_build_object(
        'id', p.id,
        'title', p.title,
        'createdAt', p.created_at
      ) ORDER BY p.created_at DESC)
      FROM posts p
      WHERE p.user_id = u.id
    )
  ) as data
FROM users u;
```

**GraphQL Query** (single SQL query):
```graphql
query {
  users {
    id
    name
    posts {
      id
      title
    }
  }
}
```

**Performance**: Single database query regardless of nesting depth. No DataLoader setup required.

## View Composition Patterns

### Basic View

Simple entity view with JSONB output:

```sql
CREATE VIEW v_product AS
SELECT
  p.id,
  p.sku,
  p.name,
  p.price,
  jsonb_build_object(
    '__typename', 'Product',
    'id', p.id,
    'sku', p.sku,
    'name', p.name,
    'price', p.price,
    'categoryId', p.category_id
  ) as data
FROM products p
WHERE p.deleted_at IS NULL;
```

### Nested Aggregations

Multi-level nested data in single view:

```sql
CREATE VIEW v_order_complete AS
SELECT
  o.id,
  o.customer_id,
  o.status,
  jsonb_build_object(
    '__typename', 'Order',
    'id', o.id,
    'status', o.status,
    'total', o.total,
    'customer', (
      SELECT jsonb_build_object(
        'id', c.id,
        'name', c.name,
        'email', c.email
      )
      FROM customers c
      WHERE c.id = o.customer_id
    ),
    'items', (
      SELECT jsonb_agg(jsonb_build_object(
        'id', i.id,
        'productName', i.product_name,
        'quantity', i.quantity,
        'price', i.price
      ) ORDER BY i.created_at)
      FROM order_items i
      WHERE i.order_id = o.id
    ),
    'shipping', (
      SELECT jsonb_build_object(
        'address', s.address,
        'city', s.city,
        'status', s.status,
        'trackingNumber', s.tracking_number
      )
      FROM shipments s
      WHERE s.order_id = o.id
      LIMIT 1
    )
  ) as data
FROM orders o;
```

### Conditional Aggregations

Include data based on WHERE clauses in subqueries:

```sql
CREATE VIEW v_post_with_approved_comments AS
SELECT
  p.id,
  p.title,
  jsonb_build_object(
    '__typename', 'Post',
    'id', p.id,
    'title', p.title,
    'content', p.content,
    'approvedComments', (
      SELECT jsonb_agg(jsonb_build_object(
        'id', c.id,
        'text', c.text,
        'author', c.author_name
      ) ORDER BY c.created_at DESC)
      FROM comments c
      WHERE c.post_id = p.id
        AND c.status = 'approved'  -- Conditional filter
    ),
    'pendingCommentCount', (
      SELECT COUNT(*)
      FROM comments c
      WHERE c.post_id = p.id
        AND c.status = 'pending'
    )
  ) as data
FROM posts p;
```

## Materialized Views

**Purpose**: Pre-compute expensive aggregations.

**Creation**:
```sql
CREATE MATERIALIZED VIEW mv_user_stats AS
SELECT
  u.id,
  u.name,
  COUNT(DISTINCT p.id) as post_count,
  COUNT(DISTINCT c.id) as comment_count,
  MAX(p.created_at) as last_post_at,
  SUM(p.view_count) as total_views
FROM users u
LEFT JOIN posts p ON p.author_id = u.id
LEFT JOIN comments c ON c.user_id = u.id
GROUP BY u.id, u.name;

CREATE UNIQUE INDEX ON mv_user_stats (id);
```

**Refresh Strategy**:
```sql
-- Manual refresh
REFRESH MATERIALIZED VIEW CONCURRENTLY mv_user_stats;

-- Scheduled refresh (using pg_cron)
SELECT cron.schedule(
  'refresh-stats',
  '0 * * * *',  -- Every hour
  'REFRESH MATERIALIZED VIEW CONCURRENTLY mv_user_stats'
);
```

**Trade-offs**:

| Approach | Freshness | Query Speed | Complexity |
|----------|-----------|-------------|------------|
| Regular View | Real-time | Slower | Low |
| Materialized View | Scheduled | Fast | Medium |
| Incremental Update | Near real-time | Fast | High |

## Table-View Sync Pattern

**Purpose**: Maintain separate write tables and read views.

**Pattern**:
```sql
-- Write-optimized table (normalized)
CREATE TABLE orders (
  id UUID PRIMARY KEY,
  tenant_id UUID NOT NULL,
  user_id UUID NOT NULL,
  status VARCHAR(50),
  total DECIMAL(10,2),
  created_at TIMESTAMP DEFAULT NOW()
);

-- Read-optimized view (denormalized)
CREATE VIEW v_orders AS
SELECT
  o.id,
  o.tenant_id,
  o.status,
  o.total,
  jsonb_build_object(
    'id', o.id,
    'status', o.status,
    'total', o.total,
    'user', jsonb_build_object(
      'id', u.id,
      'email', u.email,
      'name', u.name
    ),
    'items', (
      SELECT jsonb_agg(jsonb_build_object(
        'id', i.id,
        'name', i.name,
        'quantity', i.quantity,
        'price', i.price
      ))
      FROM order_items i
      WHERE i.order_id = o.id
    )
  ) as data
FROM orders o
JOIN users u ON u.id = o.user_id;
```

**Benefits**:

- Write operations use normalized tables (data integrity)
- Read operations use denormalized views (performance)
- Schema changes don't break API (view acts as abstraction)

## Multi-Tenancy Patterns

### Row-Level Security

Tenant isolation at the database level:

```sql
-- Multi-tenant table with RLS
CREATE TABLE projects (
  id UUID PRIMARY KEY,
  tenant_id UUID NOT NULL,
  name VARCHAR(200) NOT NULL,
  description TEXT,
  created_at TIMESTAMP DEFAULT NOW()
);

-- Enable Row Level Security
ALTER TABLE projects ENABLE ROW LEVEL SECURITY;

-- Create policy for tenant isolation
CREATE POLICY tenant_isolation ON projects
  FOR ALL
  USING (tenant_id = current_setting('app.current_tenant_id')::UUID);

-- Tenant-aware view
CREATE VIEW v_projects AS
SELECT
  p.id,
  p.name,
  jsonb_build_object(
    '__typename', 'Project',
    'id', p.id,
    'name', p.name,
    'description', p.description,
    'createdAt', p.created_at
  ) as data
FROM projects p;

-- Set tenant context before queries
SELECT set_config('app.current_tenant_id', '123e4567-...', true);
```

### View-Level Tenant Filtering

Filter tenants in view definition:

```sql
CREATE VIEW v_tenant_orders AS
SELECT
  o.id,
  jsonb_build_object(
    '__typename', 'Order',
    'id', o.id,
    'status', o.status,
    'total', o.total
  ) as data
FROM orders o
WHERE o.tenant_id = current_setting('app.tenant_id')::UUID;
```

### Application-Level Filtering

Use QueryOptions for tenant filtering:

```python
from fraiseql import query

@query
async def get_orders(info, status: str | None = None) -> list[Order]:
    db = info.context["db"]
    tenant_id = info.context["tenant_id"]

    where = {"tenant_id": tenant_id}
    if status:
        where["status"] = status

    return await db.find("v_orders", where=where)
```

## Indexing Strategy

### JSONB Indexes

```sql
-- GIN index for JSONB containment queries
CREATE INDEX idx_orders_json_data ON orders USING GIN (data);

-- Expression index for specific JSONB fields
CREATE INDEX idx_orders_status ON orders ((data->>'status'));

-- Functional index for nested JSONB
CREATE INDEX idx_orders_user_email ON orders ((data->'user'->>'email'));
```

### Multi-Column Indexes

```sql
-- Tenant + timestamp for common queries
CREATE INDEX idx_orders_tenant_created
ON orders (tenant_id, created_at DESC);

-- Status + tenant for filtered queries
CREATE INDEX idx_orders_status_tenant
ON orders (status, tenant_id)
WHERE status != 'cancelled';
```

### Partial Indexes

```sql
-- Index only active records
CREATE INDEX idx_orders_active
ON orders (tenant_id, created_at DESC)
WHERE status IN ('pending', 'processing', 'shipped');

-- Index only recent records
CREATE INDEX idx_orders_recent
ON orders (tenant_id, status)
WHERE created_at > NOW() - INTERVAL '30 days';
```

## Query Optimization

### Analyze Query Plans

```sql
EXPLAIN (ANALYZE, BUFFERS)
SELECT data FROM v_orders WHERE tenant_id = '123e4567-...';

-- Look for:
-- - Sequential scans (bad) vs Index scans (good)
-- - High buffer usage
-- - Nested loop joins vs hash joins
```

### Common Optimization Patterns

**Use LATERAL joins for correlated subqueries**:
```sql
CREATE VIEW v_users_with_latest_post AS
SELECT
  u.id,
  jsonb_build_object(
    'id', u.id,
    'name', u.name,
    'latestPost', p.data
  ) as data
FROM users u
LEFT JOIN LATERAL (
  SELECT jsonb_build_object(
    'id', p.id,
    'title', p.title
  ) as data
  FROM posts p
  WHERE p.author_id = u.id
  ORDER BY p.created_at DESC
  LIMIT 1
) p ON true;
```

**Use COALESCE for null handling**:
```sql
SELECT
  jsonb_build_object(
    'items', COALESCE(
      (SELECT jsonb_agg(...) FROM items),
      '[]'::jsonb  -- Default to empty array
    )
  ) as data
FROM orders;
```

**Use DISTINCT ON for latest records**:
```sql
CREATE VIEW v_latest_order_per_user AS
SELECT DISTINCT ON (user_id)
  user_id,
  jsonb_build_object(
    'orderId', id,
    'total', total,
    'createdAt', created_at
  ) as data
FROM orders
ORDER BY user_id, created_at DESC;
```

## Hierarchical Data Patterns

### Recursive CTE for Tree Structures

```sql
-- Category hierarchy
CREATE TABLE categories (
  id UUID PRIMARY KEY,
  parent_id UUID REFERENCES categories(id),
  name VARCHAR(100) NOT NULL,
  slug VARCHAR(100) NOT NULL
);

-- Recursive view for full tree
CREATE VIEW v_category_tree AS
WITH RECURSIVE category_tree AS (
  -- Root categories
  SELECT
    id,
    parent_id,
    name,
    slug,
    0 AS depth,
    ARRAY[id] AS path,
    ARRAY[name] AS breadcrumb
  FROM categories
  WHERE parent_id IS NULL

  UNION ALL

  -- Child categories
  SELECT
    c.id,
    c.parent_id,
    c.name,
    c.slug,
    ct.depth + 1,
    ct.path || c.id,
    ct.breadcrumb || c.name
  FROM categories c
  JOIN category_tree ct ON c.parent_id = ct.id
  WHERE ct.depth < 10  -- Prevent infinite recursion
)
SELECT
  id,
  jsonb_build_object(
    '__typename', 'Category',
    'id', id,
    'name', name,
    'slug', slug,
    'depth', depth,
    'path', path,
    'breadcrumb', breadcrumb,
    'children', (
      SELECT jsonb_agg(jsonb_build_object(
        'id', c.id,
        'name', c.name,
        'slug', c.slug
      ) ORDER BY c.name)
      FROM categories c
      WHERE c.parent_id = category_tree.id
    )
  ) as data
FROM category_tree
ORDER BY path;
```

### Materialized Path Pattern

Using ltree extension for efficient tree queries:

```sql
-- Using ltree extension
CREATE EXTENSION IF NOT EXISTS ltree;

CREATE TABLE categories_ltree (
  id UUID PRIMARY KEY,
  name VARCHAR(100) NOT NULL,
  path ltree NOT NULL,
  UNIQUE(path)
);

-- Index for path operations
CREATE INDEX idx_category_path ON categories_ltree USING gist(path);

-- Insert with path
INSERT INTO categories_ltree (name, path) VALUES
  ('Electronics', 'electronics'),
  ('Computers', 'electronics.computers'),
  ('Laptops', 'electronics.computers.laptops'),
  ('Gaming Laptops', 'electronics.computers.laptops.gaming');

-- Find all descendants
SELECT
  c.id,
  c.name,
  c.path,
  jsonb_build_object(
    'id', c.id,
    'name', c.name,
    'path', c.path::text,
    'depth', nlevel(c.path)
  ) as data
FROM categories_ltree c
WHERE c.path <@ 'electronics.computers'::ltree;  -- All under computers
```

## Polymorphic Associations

### Single Table Inheritance Pattern

Store different entity types in one table:

```sql
-- Polymorphic notifications
CREATE TABLE notifications (
  id UUID PRIMARY KEY,
  user_id UUID NOT NULL,
  type VARCHAR(50) NOT NULL,
  -- Polymorphic reference
  entity_type VARCHAR(50),
  entity_id UUID,
  -- Type-specific data
  data JSONB NOT NULL,
  read_at TIMESTAMP,
  created_at TIMESTAMP DEFAULT NOW()
);

CREATE INDEX idx_user_notifications
ON notifications(user_id, read_at, created_at DESC);

-- Type-specific view with entity resolution
CREATE VIEW v_notifications AS
SELECT
  n.id,
  n.user_id,
  n.read_at,
  jsonb_build_object(
    '__typename', 'Notification',
    'id', n.id,
    'type', n.type,
    'read', n.read_at IS NOT NULL,
    'createdAt', n.created_at,
    -- Polymorphic entity resolution
    'entity', CASE n.entity_type
      WHEN 'Post' THEN (
        SELECT jsonb_build_object(
          '__typename', 'Post',
          'id', p.id,
          'title', p.title
        )
        FROM posts p
        WHERE p.id = n.entity_id
      )
      WHEN 'Comment' THEN (
        SELECT jsonb_build_object(
          '__typename', 'Comment',
          'id', c.id,
          'content', LEFT(c.content, 100)
        )
        FROM comments c
        WHERE c.id = n.entity_id
      )
      ELSE NULL
    END,
    'message', n.data->>'message'
  ) as data
FROM notifications n
ORDER BY n.created_at DESC;
```

### Table Per Type with Union Pattern

Separate tables unified through views:

```sql
-- Different activity types
CREATE TABLE page_views (
  id UUID PRIMARY KEY,
  user_id UUID,
  page_url TEXT NOT NULL,
  referrer TEXT,
  duration_seconds INT,
  created_at TIMESTAMP DEFAULT NOW()
);

CREATE TABLE button_clicks (
  id UUID PRIMARY KEY,
  user_id UUID,
  button_id VARCHAR(100) NOT NULL,
  page_url TEXT NOT NULL,
  created_at TIMESTAMP DEFAULT NOW()
);

CREATE TABLE form_submissions (
  id UUID PRIMARY KEY,
  user_id UUID,
  form_id VARCHAR(100) NOT NULL,
  form_data JSONB NOT NULL,
  created_at TIMESTAMP DEFAULT NOW()
);

-- Unified activity view
CREATE VIEW v_user_activities AS
SELECT
  id,
  user_id,
  activity_type,
  created_at,
  jsonb_build_object(
    '__typename', 'UserActivity',
    'id', id,
    'type', activity_type,
    'details', details,
    'createdAt', created_at
  ) as data
FROM (
  SELECT
    id,
    user_id,
    'page_view' AS activity_type,
    jsonb_build_object(
      'pageUrl', page_url,
      'referrer', referrer,
      'duration', duration_seconds
    ) AS details,
    created_at
  FROM page_views

  UNION ALL

  SELECT
    id,
    user_id,
    'button_click' AS activity_type,
    jsonb_build_object(
      'buttonId', button_id,
      'pageUrl', page_url
    ) AS details,
    created_at
  FROM button_clicks

  UNION ALL

  SELECT
    id,
    user_id,
    'form_submission' AS activity_type,
    jsonb_build_object(
      'formId', form_id,
      'fields', form_data
    ) AS details,
    created_at
  FROM form_submissions
) activities
ORDER BY created_at DESC;
```

## Production Patterns from Real Systems

### Entity Change Log (Audit Trail)

**Purpose**: Centralized audit log for tracking all object-level changes across the system.

**Table Structure**:
```sql
CREATE TABLE core.tb_entity_change_log (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    pk_entity_change_log UUID NOT NULL DEFAULT gen_random_uuid(),

    tenant_id UUID NOT NULL,
    user_id UUID,  -- User who triggered the change

    object_type TEXT NOT NULL,  -- e.g., 'allocation', 'machine', 'location'
    object_id UUID NOT NULL,

    modification_type TEXT NOT NULL CHECK (
        modification_type IN ('INSERT', 'UPDATE', 'DELETE', 'NOOP')
    ),

    change_status TEXT NOT NULL CHECK (
        change_status ~ '^(new|existing|updated|deleted|synced|completed|ok|done|success|failed:[a-z_]+|noop:[a-z_]+|conflict:[a-z_]+|duplicate:[a-z_]+|validation:[a-z_]+|not_found|forbidden|unauthorized|blocked:[a-z_]+)$'
    ),

    object_data JSONB NOT NULL,      -- Before/after snapshots
    extra_metadata JSONB DEFAULT '{}'::jsonb,

    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_entity_log_object ON core.tb_entity_change_log (object_type, object_id);
CREATE INDEX idx_entity_log_tenant ON core.tb_entity_change_log (tenant_id, created_at);
CREATE INDEX idx_entity_log_status ON core.tb_entity_change_log (change_status);
```

**Debezium-Style Object Data Format**:
```json
{
  "before": {
    "id": "123e4567-...",
    "name": "Old Name",
    "status": "pending"
  },
  "after": {
    "id": "123e4567-...",
    "name": "New Name",
    "status": "active"
  },
  "op": "u",
  "source": {
    "connector": "postgresql",
    "table": "tb_orders"
  }
}
```

**Usage in Mutations**:
```python
from fraiseql import type, query, mutation, input, field

@mutation
async def update_order(info, id: UUID, name: str) -> MutationResult:
    db = info.context["db"]

    # Log the mutation
    result = await db.execute(
        """
        INSERT INTO core.tb_entity_change_log
            (tenant_id, user_id, object_type, object_id,
             modification_type, change_status, object_data)
        VALUES
            ($1, $2, 'order', $3, 'UPDATE', 'updated', $4::jsonb)
        RETURNING id
        """,
        info.context["tenant_id"],
        info.context["user_id"],
        id,
        json.dumps({
            "before": {"name": old_name},
            "after": {"name": name}
        })
    )

    return MutationResult(status="updated", id=id)
```

**Benefits**:
- Complete audit trail for compliance
- Debugging production issues (see what changed when)
- Rollback support (reconstruct previous state)
- Analytics on mutation patterns

### Lazy Cache with Version-Based Invalidation

**Purpose**: High-performance GraphQL query caching with automatic invalidation.

**Infrastructure**:
```sql
-- Schema for caching
CREATE SCHEMA IF NOT EXISTS turbo;

-- Unified cache table for all GraphQL queries
CREATE TABLE turbo.tb_graphql_cache (
    tenant_id UUID NOT NULL,
    query_type TEXT NOT NULL,  -- 'orders', 'order_details', etc.
    query_key TEXT NOT NULL,   -- Composite key for the specific query
    response JSONB NOT NULL,
    record_count INT DEFAULT 0,
    cache_version BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW(),
    PRIMARY KEY (tenant_id, query_type, query_key)
);

-- Version tracking per tenant and domain
CREATE TABLE turbo.tb_domain_version (
    tenant_id UUID NOT NULL,
    domain TEXT NOT NULL,  -- 'order', 'machine', 'contract'
    version BIGINT NOT NULL DEFAULT 0,
    last_modified TIMESTAMP DEFAULT NOW(),
    PRIMARY KEY (tenant_id, domain)
);

-- Indexes
CREATE INDEX idx_graphql_cache_lookup
    ON turbo.tb_graphql_cache(tenant_id, query_type, query_key, cache_version);
CREATE INDEX idx_domain_version_lookup
    ON turbo.tb_domain_version(tenant_id, domain, version);
```

**Version Increment Trigger Function**:
```sql
CREATE OR REPLACE FUNCTION turbo.fn_increment_version()
RETURNS TRIGGER AS $$
DECLARE
    v_domain TEXT;
    v_tenant_id UUID;
BEGIN
    -- Extract domain from trigger arguments
    v_domain := TG_ARGV[0];

    -- Get tenant_id from row data
    IF TG_OP = 'DELETE' THEN
        v_tenant_id := OLD.tenant_id;
    ELSIF TG_OP = 'UPDATE' THEN
        v_tenant_id := COALESCE(NEW.tenant_id, OLD.tenant_id);
    ELSE -- INSERT
        v_tenant_id := NEW.tenant_id;
    END IF;

    -- Increment version for the affected tenant and domain
    INSERT INTO turbo.tb_domain_version (tenant_id, domain, version, last_modified)
    VALUES (v_tenant_id, v_domain, 1, NOW())
    ON CONFLICT (tenant_id, domain) DO UPDATE
    SET version = turbo.tb_domain_version.version + 1,
        last_modified = NOW();

    RETURN NULL;
END;
$$ LANGUAGE plpgsql;
```

**Cache Retrieval with Auto-Refresh**:
```sql
CREATE OR REPLACE FUNCTION turbo.fn_get_cached_response(
    p_query_type TEXT,
    p_query_key TEXT,
    p_domain TEXT,
    p_builder_function TEXT,
    p_params JSONB,
    p_tenant_id UUID
)
RETURNS json AS $$
DECLARE
    v_current_version BIGINT;
    v_cached_data RECORD;
    v_fresh_data JSONB;
BEGIN
    -- Get current domain version
    SELECT version INTO v_current_version
    FROM turbo.tb_domain_version
    WHERE tenant_id = p_tenant_id AND domain = p_domain;

    -- Auto-initialize if not found
    IF v_current_version IS NULL THEN
        INSERT INTO turbo.tb_domain_version (tenant_id, domain, version)
        VALUES (p_tenant_id, p_domain, 0)
        ON CONFLICT DO NOTHING;
        v_current_version := 0;
    END IF;

    -- Try cache
    SELECT response, cache_version INTO v_cached_data
    FROM turbo.tb_graphql_cache
    WHERE tenant_id = p_tenant_id
      AND query_type = p_query_type
      AND query_key = p_query_key;

    -- Return if fresh
    IF v_cached_data.response IS NOT NULL
       AND v_cached_data.cache_version >= v_current_version THEN
        RETURN v_cached_data.response::json;
    END IF;

    -- Build fresh data
    EXECUTE format('SELECT %s(%L::jsonb)', p_builder_function, p_params)
    INTO v_fresh_data;

    -- Update cache
    INSERT INTO turbo.tb_graphql_cache
        (tenant_id, query_type, query_key, response, cache_version, updated_at)
    VALUES
        (p_tenant_id, p_query_type, p_query_key, v_fresh_data, v_current_version, NOW())
    ON CONFLICT (tenant_id, query_type, query_key) DO UPDATE SET
        response = EXCLUDED.response,
        cache_version = EXCLUDED.cache_version,
        updated_at = NOW();

    RETURN v_fresh_data::json;
END;
$$ LANGUAGE plpgsql;
```

**Trigger Setup on Materialized Views**:
```sql
-- Attach to any materialized view (tv_*)
CREATE TRIGGER trg_tv_orders_cache_invalidation
AFTER INSERT OR UPDATE OR DELETE ON tv_orders
FOR EACH ROW
EXECUTE FUNCTION turbo.fn_increment_version('order');
```

**Benefits**:
- Sub-millisecond cached response times
- Automatic invalidation (no manual cache clearing)
- Multi-tenant isolation
- Version-based consistency (no stale data)

### Subdomain-Specific Cache Invalidation

**Purpose**: Cascade cache invalidation across related domains.

**Pattern**:
```sql
-- Enhanced trigger with cascade invalidation
CREATE OR REPLACE FUNCTION turbo.fn_tv_table_cache_invalidation()
RETURNS TRIGGER AS $$
DECLARE
    v_tenant_id UUID;
    v_domain TEXT;
BEGIN
    -- Extract domain from table name (e.g., tv_contract -> contract)
    v_domain := regexp_replace(TG_TABLE_NAME, '^tv_', '');

    -- Get tenant_id
    IF TG_OP = 'DELETE' THEN
        v_tenant_id := OLD.tenant_id;
    ELSE
        v_tenant_id := NEW.tenant_id;
    END IF;

    -- Increment primary domain version
    INSERT INTO turbo.tb_domain_version (tenant_id, domain, version)
    VALUES (v_tenant_id, v_domain, 1)
    ON CONFLICT (tenant_id, domain) DO UPDATE
    SET version = turbo.tb_domain_version.version + 1,
        last_modified = NOW();

    -- Handle cascade invalidations for related domains
    IF v_domain = 'contract' THEN
        -- Contract changes affect items and prices
        PERFORM turbo.fn_invalidate_domain(v_tenant_id, 'item');
        PERFORM turbo.fn_invalidate_domain(v_tenant_id, 'price');
    ELSIF v_domain = 'order' THEN
        -- Order changes affect allocation
        PERFORM turbo.fn_invalidate_domain(v_tenant_id, 'allocation');
    END IF;

    RETURN NULL;
END;
$$ LANGUAGE plpgsql;
```

**Helper Function for Domain Invalidation**:
```sql
CREATE OR REPLACE FUNCTION turbo.fn_invalidate_domain(
    p_tenant_id UUID,
    p_domain TEXT
)
RETURNS void AS $$
BEGIN
    INSERT INTO turbo.tb_domain_version (tenant_id, domain, version)
    VALUES (p_tenant_id, p_domain, 1)
    ON CONFLICT (tenant_id, domain) DO UPDATE
    SET version = turbo.tb_domain_version.version + 1,
        last_modified = NOW();
END;
$$ LANGUAGE plpgsql;
```

### Standardized Mutation Response Shape

**Purpose**: Consistent mutation results with before/after snapshots.

**GraphQL Type**:
```python
@fraise_type
class MutationResultBase:
    """Standardized result for all mutations."""
    status: str
    id: UUID | None = None
    updated_fields: list[str] | None = None
    message: str | None = None
    errors: list[dict[str, Any]] | None = None

@fraise_type
class MutationLogResult:
    """Detailed mutation result with change tracking."""
    status: str
    message: str | None = None
    reason: str | None = None
    op: str | None = None  # insert, update, delete
    entity: str | None = None
    extra_metadata: dict[str, Any] | None = None
    payload_before: dict[str, Any] | None = None
    payload_after: dict[str, Any] | None = None
```

**Usage in Resolver**:
```python
from fraiseql import type, query, mutation, input, field

@mutation
async def update_product(
    info,
    id: UUID,
    name: str,
    price: float
) -> MutationLogResult:
    db = info.context["db"]

    # Get current state
    old_product = await db.find_one("v_product", {"id": id})

    # Update
    await db.execute(
        "UPDATE tb_product SET name = $1, price = $2 WHERE id = $3",
        name, price, id
    )

    # Get new state
    new_product = await db.find_one("v_product", {"id": id})

    return MutationLogResult(
        status="updated",
        message=f"Product {name} updated successfully",
        op="update",
        entity="product",
        payload_before=old_product,
        payload_after=new_product,
        extra_metadata={"updated_fields": ["name", "price"]}
    )
```

### Monitoring & Metrics

**Cache Performance Metrics**:
```sql
-- Metrics table
CREATE TABLE turbo.tb_cache_metrics (
    id BIGSERIAL PRIMARY KEY,
    tenant_id UUID NOT NULL,
    query_type TEXT NOT NULL,
    cache_hit BOOLEAN NOT NULL,
    execution_time_ms FLOAT NOT NULL,
    recorded_at TIMESTAMP DEFAULT NOW()
);

CREATE INDEX idx_cache_metrics_analysis
    ON turbo.tb_cache_metrics(query_type, cache_hit, recorded_at);
```

**Cache Hit Rate Query**:
```sql
SELECT
    query_type,
    COUNT(*) FILTER (WHERE cache_hit) AS hits,
    COUNT(*) FILTER (WHERE NOT cache_hit) AS misses,
    ROUND(
        100.0 * COUNT(*) FILTER (WHERE cache_hit) / COUNT(*),
        2
    ) AS hit_rate_pct,
    ROUND(AVG(execution_time_ms)::numeric, 2) AS avg_ms
FROM turbo.tb_cache_metrics
WHERE recorded_at > NOW() - INTERVAL '1 hour'
GROUP BY query_type
ORDER BY COUNT(*) DESC;
```

**Domain Version Status**:
```sql
SELECT
    domain,
    COUNT(DISTINCT tenant_id) as tenant_count,
    MAX(version) as max_version,
    MAX(last_modified) as last_change
FROM turbo.tb_domain_version
GROUP BY domain
ORDER BY max_version DESC;
```

## Best Practices

**View Design**:
- Use JSONB aggregation to prevent N+1 queries
- Return structured data in `data` column
- Include filter columns (id, tenant_id, status) at root level
- Use COALESCE for null handling in aggregations

**Performance**:
- Index foreign keys used in joins
- Create composite indexes for common filter combinations
- Use partial indexes for subset queries
- Analyze query plans regularly

**Multi-Tenancy**:
- Apply tenant filtering at view or application level
- Use Row-Level Security for automatic isolation
- Include tenant_id in all composite indexes

**Caching**:
- Use version-based invalidation (not TTL)
- Invalidate at domain granularity
- Monitor cache hit rates (target >80%)
- Clean up stale cache periodically

**Audit Trail**:
- Log all mutations to entity_change_log
- Store before/after snapshots
- Include user context for compliance
- Use for debugging production issues

**Maintenance**:
- Document view dependencies
- Version views for backward compatibility
- Monitor materialized view freshness
- Keep views focused and composable

**Summary**:
- Use JSONB aggregation to prevent N+1 queries
- Separate write tables from read views
- Apply tenant filtering at view or application level
- Index JSONB fields accessed in WHERE clauses
- Implement lazy caching with version-based invalidation
- Log all mutations for audit trail
- Monitor query plans and cache hit rates regularly
