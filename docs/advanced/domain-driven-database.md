# Domain-Driven Database Design

This guide explains how to implement Domain-Driven Design (DDD) principles directly in the PostgreSQL database layer, aligning perfectly with FraiseQL's philosophy of keeping business logic close to the data.

## DDD Core Concepts

Before diving into implementation, let's define the key DDD concepts and how they map to database structures:

### Domain
The **domain** is the subject area or business problem space your application addresses. In database terms, this is the entire scope of your application's data model and business rules.

### Bounded Context
A **bounded context** is a boundary within which a particular domain model is defined and applicable. Different contexts can have different representations of the same concept. In PostgreSQL, we implement these as schemas.

### Aggregate
An **aggregate** is a cluster of domain objects that are treated as a single unit for data changes. The aggregate ensures consistency of changes within its boundary. In our implementation, an aggregate maps to a set of related tables with a primary table as the root.

### Aggregate Root
The **aggregate root** is the only entry point for accessing objects within an aggregate. All external access to the aggregate must go through the root. This maps to the main table and its associated functions.

### Entity
An **entity** is an object with a unique identity that persists over time. Even if its attributes change, its identity remains constant. Entities are typically stored as rows in tables with UUID primary keys.

### Value Object
A **value object** is an immutable object without identity, defined only by its attributes. Two value objects with the same attributes are considered equal. We implement these as columns, composite types, or separate normalized tables.

### Domain Event
A **domain event** represents something significant that happened in the domain. Events are facts about the past. We capture these using PostgreSQL triggers and event tables.

### Repository
A **repository** provides an abstraction for accessing aggregates, acting like an in-memory collection. In our implementation, views serve as repositories for the query side.

### Invariant
An **invariant** is a business rule that must always be true within an aggregate. PostgreSQL functions and constraints enforce these rules.

### Ubiquitous Language
The **ubiquitous language** is the common vocabulary shared by developers and domain experts. This language should be reflected in table names, function names, and column names.

## Introduction

FraiseQL's architecture naturally aligns with DDD principles by:
- Using normalized tables for the command side (write operations)
- Providing JSONB views for the query side (read operations)
- Leveraging PostgreSQL functions for business logic
- Ensuring consistency through database transactions

This CQRS (Command Query Responsibility Segregation) approach offers several benefits:
1. **Optimal Write Performance**: Normalized tables with proper constraints
2. **Optimal Read Performance**: Denormalized views with JSONB output
3. **Single Source of Truth**: Business rules enforced at the database level
4. **Flexibility**: Different representations for different use cases

## Directory Structure

Organize your database code to reflect your domain model:

```
database/
├── contexts/                 # Bounded contexts (PostgreSQL schemas)
│   ├── ordering/            # Order management context
│   │   ├── _schema.sql      # Schema creation and configuration
│   │   ├── order/           # Order aggregate root
│   │   │   ├── tables/      # Normalized table structure
│   │   │   │   ├── orders.sql
│   │   │   │   ├── order_items.sql
│   │   │   │   └── order_status_history.sql
│   │   │   ├── types.sql    # Value objects and enums
│   │   │   ├── functions/   # Business logic
│   │   │   │   ├── commands/    # State-changing operations
│   │   │   │   │   ├── create_order.sql
│   │   │   │   │   ├── add_order_item.sql
│   │   │   │   │   └── submit_order.sql
│   │   │   │   ├── queries/     # Complex read operations
│   │   │   │   │   ├── calculate_order_totals.sql
│   │   │   │   │   └── get_order_summary.sql
│   │   │   │   ├── validators/  # Invariant checks
│   │   │   │   │   ├── validate_order_items.sql
│   │   │   │   │   └── check_credit_limit.sql
│   │   │   │   └── helpers/     # Internal utilities
│   │   │   ├── views/       # JSONB views for GraphQL
│   │   │   │   ├── v_orders.sql
│   │   │   │   ├── v_order_details.sql
│   │   │   │   └── v_order_summary.sql
│   │   │   ├── triggers/    # Domain event handlers
│   │   │   │   ├── order_audit.sql
│   │   │   │   └── order_status_transition.sql
│   │   │   ├── indexes.sql  # Performance optimization
│   │   │   └── policies.sql # Row-level security
│   │   │
│   │   ├── customer/        # Customer aggregate root
│   │   │   └── ...          # Similar structure
│   │   │
│   │   └── payment/         # Payment aggregate
│   │       └── ...
│   │
│   ├── inventory/           # Inventory bounded context
│   │   ├── _schema.sql
│   │   ├── product/
│   │   └── warehouse/
│   │
│   └── shipping/            # Shipping bounded context
│       ├── _schema.sql
│       ├── shipment/
│       └── carrier/
│
├── shared/                  # Shared kernel
│   ├── types/              # Common value objects
│   │   ├── money.sql
│   │   ├── address.sql
│   │   └── email.sql
│   ├── functions/          # Utility functions
│   └── extensions.sql      # PostgreSQL extensions
│
└── migrations/             # Version-controlled changes
    ├── 001_initial_schema.sql
    ├── 002_add_inventory_context.sql
    └── 003_add_order_events.sql
```

## CQRS Pattern Implementation

### Command Side: Normalized Tables

The command side uses traditional normalized tables with foreign keys and constraints:

```sql
-- contexts/ordering/order/tables/orders.sql
CREATE TABLE ordering.orders (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    order_number TEXT UNIQUE NOT NULL,
    customer_id UUID NOT NULL REFERENCES ordering.customers(id),
    status ordering.order_status NOT NULL DEFAULT 'draft',
    shipping_address_id UUID REFERENCES ordering.addresses(id),
    billing_address_id UUID REFERENCES ordering.addresses(id),
    notes TEXT,
    version INTEGER NOT NULL DEFAULT 1,  -- Optimistic locking
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by UUID NOT NULL,
    updated_by UUID NOT NULL
);

-- contexts/ordering/order/tables/order_items.sql
CREATE TABLE ordering.order_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    order_id UUID NOT NULL REFERENCES ordering.orders(id) ON DELETE CASCADE,
    product_id UUID NOT NULL,
    product_name TEXT NOT NULL,  -- Denormalized for historical accuracy
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    unit_price NUMERIC(10, 2) NOT NULL CHECK (unit_price >= 0),
    discount_amount NUMERIC(10, 2) NOT NULL DEFAULT 0 CHECK (discount_amount >= 0),
    tax_amount NUMERIC(10, 2) NOT NULL DEFAULT 0 CHECK (tax_amount >= 0),
    line_total NUMERIC(10, 2) GENERATED ALWAYS AS
        ((quantity * unit_price) - discount_amount + tax_amount) STORED,
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- contexts/ordering/order/tables/order_status_history.sql
CREATE TABLE ordering.order_status_history (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    order_id UUID NOT NULL REFERENCES ordering.orders(id) ON DELETE CASCADE,
    from_status ordering.order_status,
    to_status ordering.order_status NOT NULL,
    reason TEXT,
    metadata JSONB,
    transitioned_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    transitioned_by UUID NOT NULL
);

-- Indexes for performance
CREATE INDEX idx_orders_customer ON ordering.orders(customer_id);
CREATE INDEX idx_orders_status ON ordering.orders(status);
CREATE INDEX idx_orders_created ON ordering.orders(created_at DESC);
CREATE INDEX idx_order_items_order ON ordering.order_items(order_id);
CREATE INDEX idx_order_items_product ON ordering.order_items(product_id);
```

### Query Side: JSONB Views

The query side presents denormalized views with JSONB output for FraiseQL:

```sql
-- contexts/ordering/order/views/v_orders.sql
CREATE OR REPLACE VIEW ordering.v_orders AS
WITH order_totals AS (
    SELECT
        o.id,
        COUNT(oi.id) as item_count,
        SUM(oi.quantity) as total_quantity,
        SUM(oi.line_total) as subtotal,
        SUM(oi.tax_amount) as tax_total,
        SUM(oi.discount_amount) as discount_total,
        SUM(oi.line_total) as grand_total
    FROM ordering.orders o
    LEFT JOIN ordering.order_items oi ON oi.order_id = o.id
    GROUP BY o.id
)
SELECT
    o.id,
    jsonb_build_object(
        'id', o.id,
        'order_number', o.order_number,
        'customer_id', o.customer_id,
        'status', o.status,
        'notes', o.notes,
        'totals', jsonb_build_object(
            'subtotal', COALESCE(ot.subtotal, 0),
            'tax_total', COALESCE(ot.tax_total, 0),
            'discount_total', COALESCE(ot.discount_total, 0),
            'grand_total', COALESCE(ot.grand_total, 0),
            'currency', 'USD'
        ),
        'item_count', COALESCE(ot.item_count, 0),
        'created_at', o.created_at,
        'updated_at', o.updated_at
    ) as data
FROM ordering.orders o
LEFT JOIN order_totals ot ON ot.id = o.id;

-- contexts/ordering/order/views/v_order_details.sql
CREATE OR REPLACE VIEW ordering.v_order_details AS
SELECT
    o.id,
    jsonb_build_object(
        'id', o.id,
        'order_number', o.order_number,
        'customer', jsonb_build_object(
            'id', c.id,
            'name', c.name,
            'email', c.email
        ),
        'status', o.status,
        'items', COALESCE(
            jsonb_agg(
                jsonb_build_object(
                    'id', oi.id,
                    'product_id', oi.product_id,
                    'product_name', oi.product_name,
                    'quantity', oi.quantity,
                    'unit_price', oi.unit_price,
                    'discount_amount', oi.discount_amount,
                    'tax_amount', oi.tax_amount,
                    'line_total', oi.line_total
                ) ORDER BY oi.created_at
            ) FILTER (WHERE oi.id IS NOT NULL),
            '[]'::jsonb
        ),
        'shipping_address', CASE
            WHEN sa.id IS NOT NULL THEN jsonb_build_object(
                'street_line_1', sa.street_line_1,
                'street_line_2', sa.street_line_2,
                'city', sa.city,
                'state_province', sa.state_province,
                'postal_code', sa.postal_code,
                'country_code', sa.country_code
            )
            ELSE NULL
        END,
        'billing_address', CASE
            WHEN ba.id IS NOT NULL THEN jsonb_build_object(
                'street_line_1', ba.street_line_1,
                'street_line_2', ba.street_line_2,
                'city', ba.city,
                'state_province', ba.state_province,
                'postal_code', ba.postal_code,
                'country_code', ba.country_code
            )
            ELSE NULL
        END,
        'status_history', (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'from_status', h.from_status,
                    'to_status', h.to_status,
                    'reason', h.reason,
                    'transitioned_at', h.transitioned_at
                ) ORDER BY h.transitioned_at
            )
            FROM ordering.order_status_history h
            WHERE h.order_id = o.id
        ),
        'notes', o.notes,
        'created_at', o.created_at,
        'updated_at', o.updated_at
    ) as data
FROM ordering.orders o
LEFT JOIN ordering.customers c ON c.id = o.customer_id
LEFT JOIN ordering.order_items oi ON oi.order_id = o.id
LEFT JOIN ordering.addresses sa ON sa.id = o.shipping_address_id
LEFT JOIN ordering.addresses ba ON ba.id = o.billing_address_id
GROUP BY o.id, c.id, c.name, c.email, sa.id, sa.street_line_1, sa.street_line_2,
         sa.city, sa.state_province, sa.postal_code, sa.country_code,
         ba.id, ba.street_line_1, ba.street_line_2, ba.city, ba.state_province,
         ba.postal_code, ba.country_code;

-- Grant access for FraiseQL
GRANT SELECT ON ordering.v_orders TO fraiseql_role;
GRANT SELECT ON ordering.v_order_details TO fraiseql_role;
```

## Core Implementation Patterns

### Command Functions (Write Operations)

Commands work with normalized tables and enforce business rules:

```sql
-- contexts/ordering/order/functions/commands/create_order.sql
CREATE OR REPLACE FUNCTION ordering.create_order(
    input_data JSONB
) RETURNS JSONB AS $$
DECLARE
    v_order_id UUID;
    v_order_number TEXT;
    v_customer_id UUID;
    result JSONB;
BEGIN
    -- Extract and validate customer
    v_customer_id := (input_data->>'customer_id')::UUID;
    IF NOT EXISTS (SELECT 1 FROM ordering.customers WHERE id = v_customer_id) THEN
        RETURN jsonb_build_object(
            'success', false,
            'error', 'Customer not found',
            'code', 'CUSTOMER_NOT_FOUND'
        );
    END IF;

    -- Generate order number
    v_order_number := 'ORD-' || to_char(NOW(), 'YYYYMMDD-') ||
                     lpad(nextval('ordering.order_number_seq')::text, 6, '0');

    -- Create order in normalized table
    INSERT INTO ordering.orders (
        order_number,
        customer_id,
        notes,
        created_by,
        updated_by
    ) VALUES (
        v_order_number,
        v_customer_id,
        input_data->>'notes',
        current_setting('app.user_id')::UUID,
        current_setting('app.user_id')::UUID
    )
    RETURNING id INTO v_order_id;

    -- Add items if provided
    IF input_data ? 'items' AND jsonb_array_length(input_data->'items') > 0 THEN
        INSERT INTO ordering.order_items (
            order_id,
            product_id,
            product_name,
            quantity,
            unit_price,
            discount_amount,
            tax_amount
        )
        SELECT
            v_order_id,
            (item->>'product_id')::UUID,
            item->>'product_name',
            (item->>'quantity')::INTEGER,
            (item->>'unit_price')::NUMERIC,
            COALESCE((item->>'discount_amount')::NUMERIC, 0),
            COALESCE((item->>'tax_amount')::NUMERIC, 0)
        FROM jsonb_array_elements(input_data->'items') AS item;
    END IF;

    -- Record status history
    INSERT INTO ordering.order_status_history (
        order_id,
        from_status,
        to_status,
        reason,
        transitioned_by
    ) VALUES (
        v_order_id,
        NULL,
        'draft',
        'Order created',
        current_setting('app.user_id')::UUID
    );

    -- Return the order using the JSONB view
    SELECT jsonb_build_object(
        'success', true,
        'order', data
    ) INTO result
    FROM ordering.v_order_details
    WHERE (data->>'id')::UUID = v_order_id;

    RETURN result;
END;
$$ LANGUAGE plpgsql;

-- contexts/ordering/order/functions/commands/add_order_item.sql
CREATE OR REPLACE FUNCTION ordering.add_order_item(
    p_order_id UUID,
    p_product_id UUID,
    p_quantity INTEGER,
    p_unit_price NUMERIC,
    p_discount_amount NUMERIC DEFAULT 0,
    p_tax_amount NUMERIC DEFAULT 0
) RETURNS JSONB AS $$
DECLARE
    v_order_status ordering.order_status;
    v_product_name TEXT;
    v_item_id UUID;
    result JSONB;
BEGIN
    -- Lock order and check status
    SELECT status INTO v_order_status
    FROM ordering.orders
    WHERE id = p_order_id
    FOR UPDATE;

    IF NOT FOUND THEN
        RETURN jsonb_build_object(
            'success', false,
            'error', 'Order not found',
            'code', 'ORDER_NOT_FOUND'
        );
    END IF;

    IF v_order_status NOT IN ('draft', 'submitted') THEN
        RETURN jsonb_build_object(
            'success', false,
            'error', 'Cannot modify order in status: ' || v_order_status,
            'code', 'INVALID_ORDER_STATUS'
        );
    END IF;

    -- Get product details
    SELECT name INTO v_product_name
    FROM inventory.products
    WHERE id = p_product_id;

    IF NOT FOUND THEN
        RETURN jsonb_build_object(
            'success', false,
            'error', 'Product not found',
            'code', 'PRODUCT_NOT_FOUND'
        );
    END IF;

    -- Add item to order
    INSERT INTO ordering.order_items (
        order_id,
        product_id,
        product_name,
        quantity,
        unit_price,
        discount_amount,
        tax_amount
    ) VALUES (
        p_order_id,
        p_product_id,
        v_product_name,
        p_quantity,
        p_unit_price,
        p_discount_amount,
        p_tax_amount
    )
    RETURNING id INTO v_item_id;

    -- Update order timestamp
    UPDATE ordering.orders
    SET
        updated_at = NOW(),
        updated_by = current_setting('app.user_id')::UUID,
        version = version + 1
    WHERE id = p_order_id;

    -- Return updated order
    SELECT jsonb_build_object(
        'success', true,
        'item_id', v_item_id,
        'order', data
    ) INTO result
    FROM ordering.v_order_details
    WHERE (data->>'id')::UUID = p_order_id;

    RETURN result;
END;
$$ LANGUAGE plpgsql;
```

### Business Rules and Invariants

Enforce invariants through functions and constraints:

```sql
-- contexts/ordering/order/functions/validators/validate_order_limits.sql
CREATE OR REPLACE FUNCTION ordering.validate_order_limits(
    p_order_id UUID
) RETURNS BOOLEAN AS $$
DECLARE
    v_customer_id UUID;
    v_order_total NUMERIC;
    v_credit_limit NUMERIC;
    v_outstanding_balance NUMERIC;
BEGIN
    -- Get order details
    SELECT
        o.customer_id,
        SUM(oi.line_total)
    INTO v_customer_id, v_order_total
    FROM ordering.orders o
    LEFT JOIN ordering.order_items oi ON oi.order_id = o.id
    WHERE o.id = p_order_id
    GROUP BY o.customer_id;

    -- Get customer credit limit
    SELECT
        credit_limit,
        outstanding_balance
    INTO v_credit_limit, v_outstanding_balance
    FROM ordering.customers
    WHERE id = v_customer_id;

    -- Check credit limit
    IF v_outstanding_balance + v_order_total > v_credit_limit THEN
        RAISE EXCEPTION 'Order exceeds customer credit limit'
            USING ERRCODE = 'check_violation',
                  DETAIL = 'Credit limit: ' || v_credit_limit ||
                          ', Outstanding: ' || v_outstanding_balance ||
                          ', Order total: ' || v_order_total;
    END IF;

    -- Check minimum order amount
    IF v_order_total < 10.00 THEN
        RAISE EXCEPTION 'Order total must be at least $10.00'
            USING ERRCODE = 'check_violation';
    END IF;

    RETURN TRUE;
END;
$$ LANGUAGE plpgsql;

-- contexts/ordering/order/functions/commands/submit_order.sql
CREATE OR REPLACE FUNCTION ordering.submit_order(
    p_order_id UUID
) RETURNS JSONB AS $$
DECLARE
    v_current_status ordering.order_status;
    v_validation_result BOOLEAN;
    result JSONB;
BEGIN
    -- Lock order
    SELECT status INTO v_current_status
    FROM ordering.orders
    WHERE id = p_order_id
    FOR UPDATE;

    IF NOT FOUND THEN
        RETURN jsonb_build_object(
            'success', false,
            'error', 'Order not found',
            'code', 'ORDER_NOT_FOUND'
        );
    END IF;

    IF v_current_status != 'draft' THEN
        RETURN jsonb_build_object(
            'success', false,
            'error', 'Order must be in draft status to submit',
            'code', 'INVALID_STATUS_TRANSITION'
        );
    END IF;

    -- Validate business rules
    BEGIN
        v_validation_result := ordering.validate_order_limits(p_order_id);
    EXCEPTION
        WHEN check_violation THEN
            RETURN jsonb_build_object(
                'success', false,
                'error', SQLERRM,
                'code', 'VALIDATION_FAILED'
            );
    END;

    -- Check inventory availability (cross-context call)
    result := inventory.check_order_availability(p_order_id);
    IF NOT (result->>'available')::BOOLEAN THEN
        RETURN jsonb_build_object(
            'success', false,
            'error', 'Insufficient inventory',
            'code', 'INSUFFICIENT_INVENTORY',
            'unavailable_items', result->'unavailable_items'
        );
    END IF;

    -- Update order status
    UPDATE ordering.orders
    SET
        status = 'submitted',
        updated_at = NOW(),
        updated_by = current_setting('app.user_id')::UUID,
        version = version + 1
    WHERE id = p_order_id;

    -- Record status transition
    INSERT INTO ordering.order_status_history (
        order_id,
        from_status,
        to_status,
        reason,
        transitioned_by
    ) VALUES (
        p_order_id,
        'draft',
        'submitted',
        'Order submitted for processing',
        current_setting('app.user_id')::UUID
    );

    -- Reserve inventory
    PERFORM inventory.reserve_order_items(p_order_id);

    -- Return updated order
    SELECT jsonb_build_object(
        'success', true,
        'order', data
    ) INTO result
    FROM ordering.v_order_details
    WHERE (data->>'id')::UUID = p_order_id;

    RETURN result;
END;
$$ LANGUAGE plpgsql;
```

### Domain Events

Capture and handle domain events:

```sql
-- contexts/ordering/order/tables/order_events.sql
CREATE TABLE ordering.order_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    aggregate_id UUID NOT NULL REFERENCES ordering.orders(id),
    event_type TEXT NOT NULL,
    event_data JSONB NOT NULL,
    event_version INTEGER NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    user_id UUID,
    correlation_id UUID,
    processed BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE INDEX idx_order_events_aggregate ON ordering.order_events(aggregate_id, occurred_at);
CREATE INDEX idx_order_events_unprocessed ON ordering.order_events(processed) WHERE NOT processed;

-- contexts/ordering/order/triggers/order_audit.sql
CREATE OR REPLACE FUNCTION ordering.audit_order_changes()
RETURNS TRIGGER AS $$
DECLARE
    v_event_type TEXT;
    v_event_data JSONB;
BEGIN
    -- Determine event type
    IF TG_OP = 'INSERT' THEN
        v_event_type := 'OrderCreated';
        v_event_data := jsonb_build_object(
            'order_id', NEW.id,
            'order_number', NEW.order_number,
            'customer_id', NEW.customer_id
        );
    ELSIF TG_OP = 'UPDATE' THEN
        IF OLD.status != NEW.status THEN
            v_event_type := 'OrderStatusChanged';
            v_event_data := jsonb_build_object(
                'order_id', NEW.id,
                'old_status', OLD.status,
                'new_status', NEW.status
            );
        ELSE
            v_event_type := 'OrderUpdated';
            v_event_data := jsonb_build_object(
                'order_id', NEW.id,
                'changes', jsonb_build_object(
                    'version', NEW.version,
                    'updated_by', NEW.updated_by
                )
            );
        END IF;
    END IF;

    -- Record event
    INSERT INTO ordering.order_events (
        aggregate_id,
        event_type,
        event_data,
        event_version,
        user_id,
        correlation_id
    ) VALUES (
        NEW.id,
        v_event_type,
        v_event_data,
        NEW.version,
        current_setting('app.user_id', true)::UUID,
        current_setting('app.correlation_id', true)::UUID
    );

    -- Notify listeners
    PERFORM pg_notify('order_events', json_build_object(
        'event_type', v_event_type,
        'aggregate_id', NEW.id,
        'event_data', v_event_data
    )::text);

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER audit_order_changes
AFTER INSERT OR UPDATE ON ordering.orders
FOR EACH ROW
EXECUTE FUNCTION ordering.audit_order_changes();
```

### Query Functions (Read Operations)

Optimized queries using the JSONB views:

```sql
-- contexts/ordering/order/functions/queries/find_orders_by_customer.sql
CREATE OR REPLACE FUNCTION ordering.find_orders_by_customer(
    p_customer_id UUID,
    p_status ordering.order_status DEFAULT NULL,
    p_limit INTEGER DEFAULT 20,
    p_offset INTEGER DEFAULT 0
) RETURNS JSONB AS $$
DECLARE
    result JSONB;
BEGIN
    WITH filtered_orders AS (
        SELECT data
        FROM ordering.v_order_details
        WHERE (data->>'customer_id')::UUID = p_customer_id
        AND (p_status IS NULL OR (data->>'status')::ordering.order_status = p_status)
        ORDER BY (data->>'created_at')::TIMESTAMPTZ DESC
        LIMIT p_limit
        OFFSET p_offset
    ),
    order_count AS (
        SELECT COUNT(*) as total
        FROM ordering.orders
        WHERE customer_id = p_customer_id
        AND (p_status IS NULL OR status = p_status)
    )
    SELECT jsonb_build_object(
        'orders', COALESCE(jsonb_agg(data), '[]'::jsonb),
        'total_count', (SELECT total FROM order_count),
        'has_more', (SELECT total FROM order_count) > (p_offset + p_limit)
    ) INTO result
    FROM filtered_orders;

    RETURN result;
END;
$$ LANGUAGE plpgsql STABLE;

-- contexts/ordering/order/functions/queries/get_order_analytics.sql
CREATE OR REPLACE FUNCTION ordering.get_order_analytics(
    p_start_date DATE,
    p_end_date DATE
) RETURNS JSONB AS $$
DECLARE
    result JSONB;
BEGIN
    WITH daily_stats AS (
        SELECT
            DATE(o.created_at) as order_date,
            COUNT(*) as order_count,
            COUNT(DISTINCT o.customer_id) as unique_customers,
            SUM(oi.line_total) as revenue,
            AVG(oi.line_total) as avg_order_value
        FROM ordering.orders o
        LEFT JOIN ordering.order_items oi ON oi.order_id = o.id
        WHERE DATE(o.created_at) BETWEEN p_start_date AND p_end_date
        GROUP BY DATE(o.created_at)
    ),
    product_stats AS (
        SELECT
            oi.product_id,
            oi.product_name,
            SUM(oi.quantity) as units_sold,
            SUM(oi.line_total) as revenue
        FROM ordering.orders o
        JOIN ordering.order_items oi ON oi.order_id = o.id
        WHERE DATE(o.created_at) BETWEEN p_start_date AND p_end_date
        GROUP BY oi.product_id, oi.product_name
        ORDER BY revenue DESC
        LIMIT 10
    )
    SELECT jsonb_build_object(
        'period', jsonb_build_object(
            'start_date', p_start_date,
            'end_date', p_end_date
        ),
        'summary', jsonb_build_object(
            'total_orders', (SELECT SUM(order_count) FROM daily_stats),
            'total_revenue', (SELECT SUM(revenue) FROM daily_stats),
            'unique_customers', (SELECT COUNT(DISTINCT customer_id)
                                FROM ordering.orders
                                WHERE DATE(created_at) BETWEEN p_start_date AND p_end_date),
            'avg_order_value', (SELECT AVG(revenue) FROM daily_stats)
        ),
        'daily_breakdown', (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'date', order_date,
                    'orders', order_count,
                    'revenue', revenue,
                    'avg_order_value', avg_order_value
                ) ORDER BY order_date
            ) FROM daily_stats
        ),
        'top_products', (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'product_id', product_id,
                    'product_name', product_name,
                    'units_sold', units_sold,
                    'revenue', revenue
                )
            ) FROM product_stats
        )
    ) INTO result;

    RETURN result;
END;
$$ LANGUAGE plpgsql STABLE;
```

## Best Practices

### 1. Naming Conventions

- **Schemas**: Use bounded context names (ordering, inventory, shipping)
- **Tables**: Use plural nouns (orders, customers, products)
- **Functions**: Use verb_noun pattern (create_order, find_customer, validate_address)
- **Views**: Prefix with v_ (v_orders, v_order_summary)
- **Types**: Use singular nouns (address, money_amount)

### 2. Transaction Boundaries

Always handle transactions at the PostgreSQL function level:

```sql
-- Good: Single function handles the entire transaction
CREATE FUNCTION ordering.complete_order_checkout(p_order_id UUID) RETURNS JSONB AS $$
BEGIN
    -- All operations within this function are atomic
    PERFORM ordering.validate_order_limits(p_order_id);
    PERFORM ordering.submit_order(p_order_id);
    PERFORM payment.process_order_payment(p_order_id);
    PERFORM shipping.create_shipment(p_order_id);
    -- If any step fails, entire transaction rolls back

    RETURN jsonb_build_object('success', true);
EXCEPTION
    WHEN OTHERS THEN
        RETURN jsonb_build_object(
            'success', false,
            'error', SQLERRM,
            'code', SQLSTATE
        );
END;
$$ LANGUAGE plpgsql;
```

### 3. View Design Principles

Design views for specific GraphQL query patterns:

```sql
-- Materialized view for expensive aggregations
CREATE MATERIALIZED VIEW ordering.mv_customer_lifetime_value AS
SELECT
    c.id as customer_id,
    jsonb_build_object(
        'customer_id', c.id,
        'total_orders', COUNT(DISTINCT o.id),
        'total_spent', COALESCE(SUM(oi.line_total), 0),
        'avg_order_value', COALESCE(AVG(order_totals.total), 0),
        'first_order_date', MIN(o.created_at),
        'last_order_date', MAX(o.created_at),
        'days_since_last_order', EXTRACT(DAY FROM NOW() - MAX(o.created_at)),
        'favorite_products', (
            SELECT jsonb_agg(jsonb_build_object(
                'product_id', product_id,
                'product_name', product_name,
                'order_count', order_count
            ) ORDER BY order_count DESC)
            FROM (
                SELECT
                    oi2.product_id,
                    oi2.product_name,
                    COUNT(*) as order_count
                FROM ordering.order_items oi2
                JOIN ordering.orders o2 ON o2.id = oi2.order_id
                WHERE o2.customer_id = c.id
                GROUP BY oi2.product_id, oi2.product_name
                ORDER BY COUNT(*) DESC
                LIMIT 5
            ) top_products
        )
    ) as data
FROM ordering.customers c
LEFT JOIN ordering.orders o ON o.customer_id = c.id
LEFT JOIN ordering.order_items oi ON oi.order_id = o.id
LEFT JOIN LATERAL (
    SELECT o.id, SUM(oi.line_total) as total
    FROM ordering.order_items oi
    WHERE oi.order_id = o.id
    GROUP BY o.id
) order_totals ON TRUE
GROUP BY c.id;

CREATE UNIQUE INDEX ON ordering.mv_customer_lifetime_value(customer_id);

-- Refresh strategy
CREATE OR REPLACE FUNCTION ordering.refresh_customer_metrics()
RETURNS void AS $$
BEGIN
    REFRESH MATERIALIZED VIEW CONCURRENTLY ordering.mv_customer_lifetime_value;
END;
$$ LANGUAGE plpgsql;
```

### 4. Testing Strategies

Create comprehensive tests for your domain logic:

```sql
-- contexts/ordering/order/tests/test_order_submission.sql
BEGIN;
    -- Setup test data
    INSERT INTO ordering.customers (id, name, email, credit_limit) VALUES
    ('11111111-1111-1111-1111-111111111111'::UUID, 'Test Customer', 'test@example.com', 1000.00);

    INSERT INTO inventory.products (id, name, sku, price) VALUES
    ('22222222-2222-2222-2222-222222222222'::UUID, 'Test Product', 'TEST-001', 50.00);

    -- Create test order
    DO $$
    DECLARE
        v_result JSONB;
        v_order_id UUID;
    BEGIN
        -- Create order
        v_result := ordering.create_order(jsonb_build_object(
            'customer_id', '11111111-1111-1111-1111-111111111111'
        ));

        ASSERT (v_result->>'success')::BOOLEAN = true, 'Order creation should succeed';
        v_order_id := (v_result->'order'->>'id')::UUID;

        -- Add items
        v_result := ordering.add_order_item(
            v_order_id,
            '22222222-2222-2222-2222-222222222222'::UUID,
            2,
            50.00
        );

        ASSERT (v_result->>'success')::BOOLEAN = true, 'Adding item should succeed';

        -- Submit order
        v_result := ordering.submit_order(v_order_id);

        ASSERT (v_result->>'success')::BOOLEAN = true, 'Order submission should succeed';
        ASSERT (v_result->'order'->>'status') = 'submitted', 'Order status should be submitted';

        -- Verify order state
        PERFORM 1 FROM ordering.orders
        WHERE id = v_order_id
        AND status = 'submitted';

        ASSERT FOUND, 'Order should be in submitted status';

        -- Verify inventory reservation
        PERFORM 1 FROM inventory.reservations
        WHERE order_id = v_order_id;

        ASSERT FOUND, 'Inventory should be reserved';
    END $$;

ROLLBACK;
```

## Integration with FraiseQL

Your DDD database structure integrates seamlessly with FraiseQL:

1. **Views as GraphQL Types**: Each JSONB view maps directly to a FraiseQL type
2. **Functions as Mutations**: Command functions become GraphQL mutations
3. **Query Functions**: Become GraphQL queries
4. **Event Streams**: Can trigger GraphQL subscriptions

Example FraiseQL integration:

```python
# Python FraiseQL types matching your JSONB views
@fraiseql.type
class Order:
    id: UUID
    order_number: str
    customer_id: UUID
    status: OrderStatus
    items: List[OrderItem]
    totals: OrderTotals
    shipping_address: Optional[Address]
    billing_address: Optional[Address]
    created_at: datetime
    updated_at: datetime

@fraiseql.type
class OrderItem:
    id: UUID
    product_id: UUID
    product_name: str
    quantity: int
    unit_price: Decimal
    line_total: Decimal

# Mutations calling your PostgreSQL functions
async def create_order(
    info,
    input: CreateOrderInput
) -> CreateOrderSuccess | CreateOrderError:
    repo = CQRSRepository(info.context["db"])

    # Call the PostgreSQL function
    result = await repo.call_function(
        "ordering.create_order",
        json.dumps(input.dict())
    )

    if result["success"]:
        return CreateOrderSuccess(
            order=Order.from_dict(result["order"])
        )
    else:
        return CreateOrderError(
            message=result["error"],
            code=result.get("code", "UNKNOWN_ERROR")
        )

# Query using the JSONB view
@fraiseql.type
class Query:
    @fraise_field
    async def orders(
        self,
        info,
        first: Optional[int] = 20,
        after: Optional[str] = None,
        status: Optional[OrderStatus] = None
    ) -> Connection[Order]:
        repo = CQRSRepository(info.context["db"])

        # Query the v_orders view directly
        filters = {}
        if status:
            filters["status"] = status.value

        result = await repo.paginate(
            "ordering.v_orders",
            first=first,
            after=after,
            filters=filters,
            order_by="created_at DESC"
        )

        return Connection[Order].from_dict(result)
```

## Addressing the DDD Paradox: Domain Logic in the Database

### The Traditional DDD Perspective

This approach may seem counterintuitive to traditional DDD practitioners. Classic DDD literature, particularly Eric Evans' "Domain-Driven Design" and subsequent interpretations, strongly advocates for:

1. **Persistence Ignorance**: Domain objects should not know about databases
2. **Separation of Concerns**: Business logic should be independent of storage mechanisms
3. **Hexagonal Architecture**: The domain core should be isolated from infrastructure
4. **Repository Pattern**: Abstract away all database concerns

The traditional view suggests that coupling domain logic to PostgreSQL violates these principles. So why does this database-centric approach actually align with DDD philosophy?

### Why Database-Centric DDD Makes Sense

#### 1. The Database IS the Domain Model

In this architecture, we're not coupling domain logic to a persistence mechanism—we're recognizing that **the database itself is a first-class domain modeling tool**. PostgreSQL provides:

- **Types**: Composite types, enums, and domains express value objects
- **Constraints**: Check constraints and foreign keys enforce invariants
- **Functions**: Stored procedures encapsulate business rules
- **Transactions**: ACID guarantees ensure aggregate consistency

These aren't just persistence features—they're domain modeling primitives.

#### 2. Ubiquitous Language in SQL

Traditional DDD struggles with the impedance mismatch between object-oriented languages and relational databases. Our approach eliminates this by using SQL as part of the ubiquitous language:

```sql
-- This IS domain language, not just data access
CREATE FUNCTION ordering.submit_order(p_order_id UUID) RETURNS JSONB AS $$
BEGIN
    -- Business rule: Orders can only be submitted from draft status
    -- Invariant: Credit limit must not be exceeded
    -- Process: Reserve inventory, transition status, emit events
END;
$$ LANGUAGE plpgsql;
```

The function name, parameters, and internal logic all use domain terms directly.

#### 3. True Aggregate Consistency

Traditional DDD implementations often struggle with consistency:
- ORMs make it easy to violate aggregate boundaries
- Distributed systems complicate transaction management
- Eventually consistent systems sacrifice invariants

Database-centric DDD provides **true aggregate consistency**:
```sql
-- This function IS the aggregate root
-- No way to bypass business rules
CREATE FUNCTION ordering.add_order_item(...) RETURNS JSONB AS $$
BEGIN
    -- All invariants enforced atomically
    -- No possibility of partial updates
    -- No race conditions
END;
$$ LANGUAGE plpgsql;
```

#### 4. Commands and Queries, Not Objects

This approach aligns with a functional interpretation of DDD:
- **Commands**: Functions that change state
- **Queries**: Functions that read state
- **Events**: Triggers that record what happened

This is arguably closer to the original DDD vision than object-oriented implementations:
- Focus on behavior, not data structures
- Explicit boundaries and transitions
- Clear separation of concerns

### Where This Approach Shines

#### 1. Complex Invariants
When your domain has complex business rules that span multiple entities:
```sql
-- Enforcing invariants across multiple tables atomically
-- Try doing this with eventual consistency!
CREATE FUNCTION validate_order_within_credit_limit(...)
```

#### 2. Event Sourcing and Audit Requirements
Built-in support for immutable event logs:
```sql
-- Events are first-class citizens, not an afterthought
CREATE TRIGGER capture_domain_events...
```

#### 3. Reporting and Analytics
No need for separate OLAP systems for many use cases:
```sql
-- Rich domain queries without data duplication
CREATE MATERIALIZED VIEW customer_lifetime_value...
```

#### 4. Performance-Critical Domains
When milliseconds matter:
- No network round trips for complex operations
- No ORM overhead
- Direct SQL execution with query planning

### Where Traditional DDD Might Be Better

To be fair, this approach has limitations:

1. **Multiple Database Vendors**: If you need to support MySQL, MongoDB, etc.
2. **Microservices**: When teams need independent deployment cycles
3. **Complex UI Logic**: When significant behavior lives in the frontend
4. **Machine Learning**: When domain logic includes trained models

### The Philosophical Alignment

Despite appearances, this approach deeply aligns with DDD principles:

#### Bounded Contexts ✓
PostgreSQL schemas provide true isolation:
```sql
-- Clear boundaries between contexts
CREATE SCHEMA ordering;
CREATE SCHEMA inventory;
CREATE SCHEMA shipping;
```

#### Aggregate Roots ✓
Functions serve as the only entry points:
```sql
-- Can't bypass the aggregate root
-- No way to directly modify order_items
CREATE FUNCTION ordering.add_order_item(p_order_id UUID, ...)
```

#### Domain Events ✓
First-class support through triggers and NOTIFY:
```sql
-- Events are integral, not bolted on
CREATE TRIGGER emit_order_submitted
AFTER UPDATE ON orders...
```

#### Ubiquitous Language ✓
SQL becomes part of the domain language:
```sql
-- This reads like a domain specification
CREATE TYPE order_status AS ENUM (
    'draft', 'submitted', 'confirmed', 'shipped', 'delivered'
);
```

### A New Perspective on DDD

Perhaps it's time to reconsider what "persistence ignorance" means. Instead of ignoring the database, we're:

1. **Embracing the database** as a domain modeling tool
2. **Using SQL** as part of our ubiquitous language
3. **Leveraging database features** to enforce invariants
4. **Treating functions** as our aggregate roots

This isn't a violation of DDD—it's an evolution that recognizes the database as a powerful domain modeling platform.

### The FraiseQL Synthesis

FraiseQL bridges both worlds by:
- **Separating reads and writes** (CQRS pattern)
- **Providing GraphQL abstractions** (technology independence for clients)
- **Keeping domain logic centralized** (in the database)
- **Supporting event-driven architectures** (through PostgreSQL NOTIFY)

This creates a system that is both:
- **Philosophically pure**: Clear boundaries, consistent aggregates, domain events
- **Pragmatically powerful**: High performance, simple deployment, fewer moving parts

## Conclusion

This domain-driven database design approach perfectly aligns with FraiseQL's philosophy:

- **Write operations** use normalized tables with proper constraints
- **Read operations** use JSONB views optimized for GraphQL
- **Business logic** lives in PostgreSQL functions where it can be properly tested
- **Consistency** is guaranteed by database transactions
- **Performance** is optimized through proper indexing and materialized views

By separating command and query responsibilities while keeping all logic in the database, you create a maintainable, scalable system that clearly expresses your business domain.

Rather than seeing this as a violation of DDD principles, we should recognize it as an evolution—one that acknowledges the database not as a mere persistence mechanism, but as a sophisticated platform for implementing domain-driven designs. The key insight is that **the best place to enforce domain rules is where the data lives**, and modern PostgreSQL provides all the tools necessary to do this elegantly.
