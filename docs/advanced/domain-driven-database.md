# Domain-Driven Database Design

FraiseQL implements Domain-Driven Design (DDD) principles at the database level, treating PostgreSQL as not just a data store but as the foundation of your domain model. This approach creates a unified system where business logic, data integrity, and API contracts are all enforced by the database itself.

## Architectural Overview

FraiseQL's DDD implementation centers on three core principles:

1. **Database as Domain Model**: Tables and views represent aggregates and entities
2. **PostgreSQL Functions as Domain Services**: Business operations as database functions
3. **CQRS by Design**: Separate read models (views) from write models (tables)

This architecture enables sub-millisecond response times while maintaining domain integrity, as all operations happen within the database boundary without the overhead of ORM translation or network round-trips.

## CQRS Pattern in PostgreSQL

FraiseQL implements Command Query Responsibility Segregation (CQRS) natively using PostgreSQL's features:

### Command Side (Write Model)

Commands modify state through PostgreSQL functions that accept JSONB parameters. The Python layer defines GraphQL types and passes JSON to the database:

```sql
-- Standard mutation result type
CREATE TYPE mutation_result AS (
    success BOOLEAN,
    message TEXT,
    data JSONB
);

-- Command: Create Order Aggregate
-- Accepts JSONB input for flexibility
CREATE OR REPLACE FUNCTION fn_create_order(input_data JSONB)
RETURNS mutation_result AS $$
DECLARE
    v_order_id UUID;
    v_customer RECORD;
    v_total NUMERIC(10,2);
    v_items JSONB;
BEGIN
    -- Extract and validate input
    v_items := input_data->'items';
    
    -- Validate customer exists and is active
    SELECT * INTO v_customer 
    FROM tb_customers 
    WHERE id = (input_data->>'customer_id')::UUID
        AND status = 'active';
    
    IF NOT FOUND THEN
        RETURN ROW(false, 'Customer not found or inactive', NULL)::mutation_result;
    END IF;
    
    -- Create order aggregate root
    INSERT INTO tb_orders (
        customer_id,
        shipping_address_id,
        billing_address_id,
        status,
        notes,
        order_date
    ) VALUES (
        (input_data->>'customer_id')::UUID,
        (input_data->>'shipping_address_id')::UUID,
        (input_data->>'billing_address_id')::UUID,
        'pending',
        input_data->>'notes',
        NOW()
    ) RETURNING id INTO v_order_id;
    
    -- Create order items (aggregate members)
    INSERT INTO tb_order_items (order_id, product_id, variant_id, quantity, unit_price)
    SELECT 
        v_order_id,
        (item->>'product_id')::UUID,
        (item->>'variant_id')::UUID,
        (item->>'quantity')::INT,
        (item->>'price')::NUMERIC
    FROM jsonb_array_elements(v_items) AS item;
    
    -- Validate all products exist and are active
    IF EXISTS (
        SELECT 1 FROM tb_order_items oi
        LEFT JOIN tb_products p ON p.id = oi.product_id
        WHERE oi.order_id = v_order_id 
            AND (p.id IS NULL OR p.status != 'active')
    ) THEN
        -- Rollback will happen automatically
        RAISE EXCEPTION 'One or more products are not available';
    END IF;
    
    -- Calculate total
    SELECT SUM(quantity * unit_price) INTO v_total
    FROM tb_order_items
    WHERE order_id = v_order_id;
    
    -- Apply coupon if provided
    IF input_data->>'coupon_code' IS NOT NULL THEN
        -- Apply discount logic here
        PERFORM fn_apply_coupon(v_order_id, input_data->>'coupon_code');
    END IF;
    
    -- Update order with total
    UPDATE tb_orders 
    SET total_amount = v_total
    WHERE id = v_order_id;
    
    -- Emit domain event
    PERFORM pg_notify('order_created', 
        jsonb_build_object(
            'order_id', v_order_id,
            'customer_id', (input_data->>'customer_id')::UUID,
            'total', v_total
        )::TEXT
    );
    
    -- Return success with created order
    RETURN ROW(
        true, 
        'Order created successfully',
        jsonb_build_object(
            'id', v_order_id,
            'order_number', 'ORD-' || LPAD(v_order_id::TEXT, 8, '0'),
            'total', v_total,
            'status', 'pending'
        )
    )::mutation_result;
EXCEPTION
    WHEN OTHERS THEN
        RETURN ROW(false, SQLERRM, NULL)::mutation_result;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- Python defines the GraphQL types and passes JSON to PostgreSQL
from fraiseql import mutation, input, success, failure
from uuid import UUID

@input
class OrderItemInput:
    product_id: UUID
    variant_id: UUID | None
    quantity: int
    price: float

@input  
class CreateOrderInput:
    customer_id: UUID
    shipping_address_id: UUID
    billing_address_id: UUID
    items: list[OrderItemInput]
    coupon_code: str | None
    notes: str | None

@success
class CreateOrderSuccess:
    order: Order
    message: str = "Order created successfully"

@failure
class CreateOrderError:
    message: str
    code: str

@mutation
async def create_order(
    input: CreateOrderInput,
    context
) -> CreateOrderSuccess | CreateOrderError:
    """Create a new order."""
    # FraiseQL automatically converts the input to JSON
    # and passes it to the PostgreSQL function
    db = context["db"]
    
    # The input is automatically serialized to JSON
    result = await db.execute_function(
        "fn_create_order",
        input  # FraiseQL handles the conversion to dict/JSON
    )
    
    if result["success"]:
        # Fetch the created order from the view
        order_data = await db.query_one(
            "SELECT data FROM v_order_aggregate WHERE id = $1",
            UUID(result["data"]["id"])
        )
        return CreateOrderSuccess(
            order=Order.from_dict(order_data)
        )
    else:
        return CreateOrderError(
            message=result["message"],
            code="ORDER_CREATION_FAILED"
        )
```

The architecture benefits:
1. **Type safety** in Python with GraphQL schema generation
2. **Flexible JSONB** parameters in PostgreSQL for easy evolution
3. **Thin Python layer** that mainly handles type conversion
4. **Business logic** consolidated in PostgreSQL functions
5. **Transaction safety** guaranteed by database

### Query Side (Read Model)

Queries use materialized views that present denormalized, read-optimized data:

```sql
-- Query: Order Aggregate View
CREATE OR REPLACE VIEW v_order_aggregate AS
WITH order_items_agg AS (
    -- Pre-aggregate items for performance
    SELECT 
        order_id,
        jsonb_agg(
            jsonb_build_object(
                'id', oi.id,
                'product', p.data,
                'quantity', oi.quantity,
                'price', oi.unit_price,
                'subtotal', oi.quantity * oi.unit_price
            ) ORDER BY oi.created_at
        ) AS items,
        SUM(oi.quantity * oi.unit_price) AS total,
        COUNT(*) AS item_count
    FROM tb_order_items oi
    JOIN v_products p ON p.id = oi.product_id
    GROUP BY order_id
)
SELECT 
    o.id,
    o.customer_id,
    o.status,
    o.created_at AS order_date,
    jsonb_build_object(
        '__typename', 'OrderAggregate',
        'id', o.id,
        'order_number', 'ORD-' || LPAD(o.id::TEXT, 8, '0'),
        'status', o.status,
        'customer', c.data,
        'items', COALESCE(oia.items, '[]'::jsonb),
        'total', COALESCE(oia.total, 0),
        'item_count', COALESCE(oia.item_count, 0),
        'shipping_address', sa.data,
        'billing_address', ba.data,
        'created_at', o.created_at,
        'updated_at', o.updated_at
    ) AS data
FROM tb_orders o
LEFT JOIN v_customers c ON c.id = o.customer_id
LEFT JOIN order_items_agg oia ON oia.order_id = o.id
LEFT JOIN v_addresses sa ON sa.id = o.shipping_address_id
LEFT JOIN v_addresses ba ON ba.id = o.billing_address_id;

-- Index for fast aggregate queries
CREATE INDEX idx_order_aggregate_lookup 
ON tb_orders(id, customer_id, status, created_at);
```

### Event Sourcing Readiness

While FraiseQL doesn't require event sourcing, its architecture naturally supports it:

```sql
-- Event store table
CREATE TABLE tb_domain_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    aggregate_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL,
    event_type TEXT NOT NULL,
    event_data JSONB NOT NULL,
    event_version INT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    created_by UUID
);

-- Event projection trigger
CREATE OR REPLACE FUNCTION fn_project_order_events()
RETURNS TRIGGER AS $$
BEGIN
    -- Project events to read model
    IF NEW.event_type = 'OrderCreated' THEN
        INSERT INTO tb_orders 
        SELECT * FROM jsonb_populate_record(NULL::tb_orders, NEW.event_data);
    ELSIF NEW.event_type = 'OrderItemAdded' THEN
        INSERT INTO tb_order_items
        SELECT * FROM jsonb_populate_record(NULL::tb_order_items, NEW.event_data);
    ELSIF NEW.event_type = 'OrderShipped' THEN
        UPDATE tb_orders 
        SET status = 'shipped',
            shipped_at = (NEW.event_data->>'shipped_at')::TIMESTAMPTZ
        WHERE id = NEW.aggregate_id;
    END IF;
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER project_order_events
AFTER INSERT ON tb_domain_events
FOR EACH ROW
WHEN (NEW.aggregate_type = 'Order')
EXECUTE FUNCTION fn_project_order_events();
```

## Table/View Separation Architecture

FraiseQL enforces a strict separation between storage (tables) and API contracts (views):

### The tb_/v_/tv_ Pattern

```sql
-- tb_* = Base Tables (Storage Layer)
-- These are your domain entities, never exposed directly
CREATE TABLE tb_products (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sku VARCHAR(50) NOT NULL UNIQUE,
    name VARCHAR(200) NOT NULL,
    description TEXT,
    base_price NUMERIC(10,2) NOT NULL,
    category_id UUID REFERENCES tb_categories(id),
    manufacturer_id UUID REFERENCES tb_manufacturers(id),
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- v_* = Views (API Layer)
-- These define your GraphQL API contract
CREATE OR REPLACE VIEW v_products AS
SELECT
    p.id,
    p.category_id,  -- For filtering
    p.base_price,   -- For sorting
    p.created_at,   -- For sorting
    jsonb_build_object(
        '__typename', 'Product',
        'id', p.id,
        'sku', p.sku,
        'name', p.name,
        'description', p.description,
        'price', p.base_price,
        'category', c.data,
        'manufacturer', m.data,
        'inventory', (
            SELECT jsonb_build_object(
                'in_stock', SUM(i.quantity),
                'reserved', SUM(i.reserved),
                'available', SUM(i.quantity - i.reserved)
            )
            FROM tb_inventory i
            WHERE i.product_id = p.id
        ),
        'created_at', p.created_at
    ) AS data
FROM tb_products p
LEFT JOIN v_categories c ON c.id = p.category_id
LEFT JOIN v_manufacturers m ON m.id = p.manufacturer_id;

-- tv_* = Table Views (Materialized/Cached Layer)
-- For expensive aggregations that need caching
CREATE TABLE tv_product_analytics AS
SELECT
    p.id AS product_id,
    p.sku,
    COUNT(DISTINCT o.customer_id) AS unique_buyers,
    SUM(oi.quantity) AS total_sold,
    AVG(r.rating) AS avg_rating,
    COUNT(r.id) AS review_count,
    SUM(oi.quantity * oi.unit_price) AS total_revenue,
    jsonb_build_object(
        '__typename', 'ProductAnalytics',
        'product_id', p.id,
        'sku', p.sku,
        'metrics', jsonb_build_object(
            'unique_buyers', COUNT(DISTINCT o.customer_id),
            'total_sold', SUM(oi.quantity),
            'avg_rating', ROUND(AVG(r.rating), 2),
            'review_count', COUNT(r.id),
            'total_revenue', SUM(oi.quantity * oi.unit_price)
        ),
        'trend', jsonb_build_object(
            'daily_sales', (
                SELECT jsonb_agg(
                    jsonb_build_object(
                        'date', date,
                        'quantity', quantity,
                        'revenue', revenue
                    ) ORDER BY date DESC
                )
                FROM (
                    SELECT 
                        DATE(o2.created_at) AS date,
                        SUM(oi2.quantity) AS quantity,
                        SUM(oi2.quantity * oi2.unit_price) AS revenue
                    FROM tb_order_items oi2
                    JOIN tb_orders o2 ON o2.id = oi2.order_id
                    WHERE oi2.product_id = p.id
                        AND o2.created_at >= CURRENT_DATE - INTERVAL '30 days'
                    GROUP BY DATE(o2.created_at)
                ) daily
            )
        ),
        'last_updated', NOW()
    ) AS data
FROM tb_products p
LEFT JOIN tb_order_items oi ON oi.product_id = p.id
LEFT JOIN tb_orders o ON o.id = oi.order_id
LEFT JOIN tb_reviews r ON r.product_id = p.id
GROUP BY p.id, p.sku;

-- Refresh strategy for table views
CREATE OR REPLACE FUNCTION fn_refresh_product_analytics()
RETURNS void AS $$
BEGIN
    -- Use UPSERT pattern for incremental updates
    INSERT INTO tv_product_analytics
    SELECT * FROM generate_product_analytics()
    ON CONFLICT (product_id) 
    DO UPDATE SET
        unique_buyers = EXCLUDED.unique_buyers,
        total_sold = EXCLUDED.total_sold,
        avg_rating = EXCLUDED.avg_rating,
        review_count = EXCLUDED.review_count,
        total_revenue = EXCLUDED.total_revenue,
        data = EXCLUDED.data;
END;
$$ LANGUAGE plpgsql;
```

## Aggregate Boundaries

Aggregates in FraiseQL are defined by views that compose related entities:

### Defining Aggregate Roots

```sql
-- Customer Aggregate Root
CREATE TABLE tb_customers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email VARCHAR(255) NOT NULL UNIQUE,
    status VARCHAR(50) DEFAULT 'active',
    credit_limit NUMERIC(10,2) DEFAULT 0,
    metadata JSONB DEFAULT '{}'
);

-- Aggregate Members
CREATE TABLE tb_customer_addresses (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    customer_id UUID NOT NULL REFERENCES tb_customers(id),
    address_type VARCHAR(50) NOT NULL,
    street_address TEXT NOT NULL,
    city VARCHAR(100) NOT NULL,
    postal_code VARCHAR(20),
    country VARCHAR(2) NOT NULL,
    is_default BOOLEAN DEFAULT false,
    -- Ensure aggregate consistency
    CONSTRAINT one_default_per_type UNIQUE (customer_id, address_type, is_default)
        DEFERRABLE INITIALLY DEFERRED
);

CREATE TABLE tb_customer_payment_methods (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    customer_id UUID NOT NULL REFERENCES tb_customers(id),
    payment_type VARCHAR(50) NOT NULL,
    provider VARCHAR(50) NOT NULL,
    token TEXT NOT NULL, -- Encrypted payment token
    is_default BOOLEAN DEFAULT false,
    expires_at DATE,
    -- Ensure aggregate consistency
    CONSTRAINT one_default_payment UNIQUE (customer_id, is_default)
        DEFERRABLE INITIALLY DEFERRED
);

-- Aggregate View
CREATE OR REPLACE VIEW v_customer_aggregate AS
SELECT
    c.id,
    c.email,  -- For filtering
    c.status, -- For filtering
    jsonb_build_object(
        '__typename', 'CustomerAggregate',
        'id', c.id,
        'email', c.email,
        'status', c.status,
        'credit_limit', c.credit_limit,
        'addresses', (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'id', a.id,
                    'type', a.address_type,
                    'street_address', a.street_address,
                    'city', a.city,
                    'postal_code', a.postal_code,
                    'country', a.country,
                    'is_default', a.is_default
                ) ORDER BY a.is_default DESC, a.created_at DESC
            )
            FROM tb_customer_addresses a
            WHERE a.customer_id = c.id
        ),
        'payment_methods', (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'id', pm.id,
                    'type', pm.payment_type,
                    'provider', pm.provider,
                    'last_four', RIGHT(pm.token, 4),
                    'is_default', pm.is_default,
                    'expires_at', pm.expires_at
                ) ORDER BY pm.is_default DESC, pm.created_at DESC
            )
            FROM tb_customer_payment_methods pm
            WHERE pm.customer_id = c.id
                AND (pm.expires_at IS NULL OR pm.expires_at > CURRENT_DATE)
        ),
        'statistics', (
            SELECT jsonb_build_object(
                'total_orders', COUNT(*),
                'total_spent', SUM(total_amount),
                'avg_order_value', AVG(total_amount),
                'last_order_date', MAX(created_at)
            )
            FROM tb_orders
            WHERE customer_id = c.id
                AND status != 'cancelled'
        )
    ) AS data
FROM tb_customers c;
```

### Aggregate Consistency Rules

```sql
-- Ensure aggregate invariants
CREATE OR REPLACE FUNCTION fn_validate_customer_aggregate()
RETURNS TRIGGER AS $$
BEGIN
    -- Rule: Only one default address per type
    IF NEW.is_default = true THEN
        UPDATE tb_customer_addresses
        SET is_default = false
        WHERE customer_id = NEW.customer_id
            AND address_type = NEW.address_type
            AND id != NEW.id;
    END IF;
    
    -- Rule: Customer must have at least one address
    IF TG_OP = 'DELETE' THEN
        IF NOT EXISTS (
            SELECT 1 FROM tb_customer_addresses
            WHERE customer_id = OLD.customer_id
                AND id != OLD.id
        ) THEN
            RAISE EXCEPTION 'Customer must have at least one address';
        END IF;
    END IF;
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER validate_customer_aggregate
BEFORE INSERT OR UPDATE OR DELETE ON tb_customer_addresses
FOR EACH ROW EXECUTE FUNCTION fn_validate_customer_aggregate();
```

## Bounded Contexts

Use PostgreSQL schemas to implement bounded contexts with clear boundaries:

### Schema-Based Context Separation

```sql
-- Sales Context
CREATE SCHEMA sales;

CREATE TABLE sales.tb_orders (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    customer_id UUID NOT NULL,
    total_amount NUMERIC(10,2) NOT NULL,
    status VARCHAR(50) NOT NULL
);

CREATE OR REPLACE VIEW sales.v_orders AS
SELECT
    id,
    customer_id,
    status,
    jsonb_build_object(
        '__typename', 'SalesOrder',
        'id', id,
        'customer_id', customer_id,
        'total_amount', total_amount,
        'status', status
    ) AS data
FROM sales.tb_orders;

-- Inventory Context
CREATE SCHEMA inventory;

CREATE TABLE inventory.tb_stock_levels (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    product_id UUID NOT NULL,
    warehouse_id UUID NOT NULL,
    quantity INT NOT NULL DEFAULT 0,
    reserved INT NOT NULL DEFAULT 0
);

CREATE OR REPLACE VIEW inventory.v_stock_availability AS
SELECT
    product_id,
    jsonb_build_object(
        '__typename', 'StockAvailability',
        'product_id', product_id,
        'total_quantity', SUM(quantity),
        'total_reserved', SUM(reserved),
        'available', SUM(quantity - reserved),
        'warehouses', jsonb_agg(
            jsonb_build_object(
                'warehouse_id', warehouse_id,
                'quantity', quantity,
                'reserved', reserved,
                'available', quantity - reserved
            )
        )
    ) AS data
FROM inventory.tb_stock_levels
GROUP BY product_id;

-- Context Integration View (Anti-Corruption Layer)
CREATE OR REPLACE VIEW public.v_order_fulfillment AS
SELECT
    o.id AS order_id,
    jsonb_build_object(
        '__typename', 'OrderFulfillment',
        'order', (
            SELECT data FROM sales.v_orders 
            WHERE id = o.id
        ),
        'inventory_status', (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'product_id', oi.product_id,
                    'requested', oi.quantity,
                    'available', (sa.data->>'available')::INT
                )
            )
            FROM sales.tb_order_items oi
            LEFT JOIN inventory.v_stock_availability sa 
                ON sa.product_id = oi.product_id
            WHERE oi.order_id = o.id
        )
    ) AS data
FROM sales.tb_orders o;
```

### Cross-Context Communication

```sql
-- Domain Events for Cross-Context Communication
CREATE TABLE public.tb_domain_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_context TEXT NOT NULL,
    target_context TEXT NOT NULL,
    event_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    event_data JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    processed_at TIMESTAMPTZ
);

-- Publish event from Sales context
CREATE OR REPLACE FUNCTION sales.fn_publish_order_placed()
RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO public.tb_domain_events (
        source_context,
        target_context,
        event_type,
        aggregate_id,
        event_data
    ) VALUES (
        'sales',
        'inventory',
        'OrderPlaced',
        NEW.id,
        jsonb_build_object(
            'order_id', NEW.id,
            'items', (
                SELECT jsonb_agg(
                    jsonb_build_object(
                        'product_id', product_id,
                        'quantity', quantity
                    )
                )
                FROM sales.tb_order_items
                WHERE order_id = NEW.id
            )
        )
    );
    
    -- Also notify via PostgreSQL LISTEN/NOTIFY
    PERFORM pg_notify(
        'domain_event',
        jsonb_build_object(
            'context', 'sales',
            'type', 'OrderPlaced',
            'id', NEW.id
        )::TEXT
    );
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Process event in Inventory context
CREATE OR REPLACE FUNCTION inventory.fn_process_order_placed()
RETURNS TRIGGER AS $$
BEGIN
    -- Reserve inventory for order
    IF NEW.event_type = 'OrderPlaced' AND NEW.target_context = 'inventory' THEN
        UPDATE inventory.tb_stock_levels sl
        SET reserved = reserved + items.quantity
        FROM (
            SELECT 
                (item->>'product_id')::UUID AS product_id,
                (item->>'quantity')::INT AS quantity
            FROM jsonb_array_elements(NEW.event_data->'items') AS item
        ) items
        WHERE sl.product_id = items.product_id;
        
        -- Mark event as processed
        UPDATE public.tb_domain_events
        SET processed_at = NOW()
        WHERE id = NEW.id;
    END IF;
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
```

## Domain Events with NOTIFY/LISTEN

PostgreSQL's NOTIFY/LISTEN provides real-time domain event propagation:

### Event Publishing

```sql
-- Generic domain event publisher
CREATE OR REPLACE FUNCTION fn_publish_domain_event(
    p_aggregate_type TEXT,
    p_aggregate_id UUID,
    p_event_type TEXT,
    p_event_data JSONB
) RETURNS void AS $$
BEGIN
    -- Store event for durability
    INSERT INTO tb_domain_events (
        aggregate_type,
        aggregate_id,
        event_type,
        event_data,
        event_version,
        created_at
    ) VALUES (
        p_aggregate_type,
        p_aggregate_id,
        p_event_type,
        p_event_data,
        COALESCE(
            (SELECT MAX(event_version) + 1 
             FROM tb_domain_events 
             WHERE aggregate_id = p_aggregate_id),
            1
        ),
        NOW()
    );
    
    -- Notify listeners
    PERFORM pg_notify(
        'domain_event_' || p_aggregate_type,
        jsonb_build_object(
            'aggregate_type', p_aggregate_type,
            'aggregate_id', p_aggregate_id,
            'event_type', p_event_type,
            'event_data', p_event_data,
            'timestamp', NOW()
        )::TEXT
    );
END;
$$ LANGUAGE plpgsql;

-- Automatic event publishing on state changes
CREATE OR REPLACE FUNCTION fn_auto_publish_events()
RETURNS TRIGGER AS $$
DECLARE
    v_event_type TEXT;
    v_old_data JSONB;
    v_new_data JSONB;
BEGIN
    -- Determine event type
    CASE TG_OP
        WHEN 'INSERT' THEN 
            v_event_type := TG_TABLE_NAME || 'Created';
            v_new_data := to_jsonb(NEW);
        WHEN 'UPDATE' THEN 
            v_event_type := TG_TABLE_NAME || 'Updated';
            v_old_data := to_jsonb(OLD);
            v_new_data := to_jsonb(NEW);
        WHEN 'DELETE' THEN 
            v_event_type := TG_TABLE_NAME || 'Deleted';
            v_old_data := to_jsonb(OLD);
    END CASE;
    
    -- Publish event
    PERFORM fn_publish_domain_event(
        TG_TABLE_NAME,
        COALESCE(NEW.id, OLD.id),
        v_event_type,
        jsonb_build_object(
            'old', v_old_data,
            'new', v_new_data,
            'changed_fields', (
                SELECT jsonb_object_agg(key, value)
                FROM jsonb_each(v_new_data)
                WHERE v_old_data IS NULL 
                    OR v_old_data->>key IS DISTINCT FROM value::TEXT
            )
        )
    );
    
    RETURN COALESCE(NEW, OLD);
END;
$$ LANGUAGE plpgsql;

-- Apply to domain entities
CREATE TRIGGER publish_order_events
AFTER INSERT OR UPDATE OR DELETE ON tb_orders
FOR EACH ROW EXECUTE FUNCTION fn_auto_publish_events();
```

### Event Subscription (Python)

```python
import asyncio
import json
import asyncpg
from typing import Callable, Any

class DomainEventListener:
    """Subscribe to domain events from PostgreSQL."""
    
    def __init__(self, connection_url: str):
        self.connection_url = connection_url
        self.handlers: dict[str, list[Callable]] = {}
    
    def on(self, event_type: str, handler: Callable):
        """Register event handler."""
        if event_type not in self.handlers:
            self.handlers[event_type] = []
        self.handlers[event_type].append(handler)
    
    async def listen(self, aggregate_types: list[str]):
        """Start listening for domain events."""
        conn = await asyncpg.connect(self.connection_url)
        
        # Subscribe to channels
        for aggregate_type in aggregate_types:
            await conn.add_listener(
                f'domain_event_{aggregate_type}',
                self._handle_notification
            )
        
        print(f"Listening for events on: {aggregate_types}")
        
        # Keep connection alive
        try:
            await asyncio.Future()  # Run forever
        finally:
            await conn.close()
    
    async def _handle_notification(
        self, 
        connection, 
        pid, 
        channel, 
        payload
    ):
        """Process incoming domain event."""
        event = json.loads(payload)
        event_type = event['event_type']
        
        # Call registered handlers
        for handler in self.handlers.get(event_type, []):
            try:
                await handler(event)
            except Exception as e:
                print(f"Handler error for {event_type}: {e}")

# Usage example
listener = DomainEventListener("postgresql://...")

@listener.on("OrderCreated")
async def handle_order_created(event):
    print(f"New order: {event['aggregate_id']}")
    # Send email, update search index, etc.

@listener.on("OrderShipped")
async def handle_order_shipped(event):
    print(f"Order shipped: {event['aggregate_id']}")
    # Notify customer, update tracking, etc.

# Start listening
asyncio.run(listener.listen(["orders", "customers"]))
```

## Repository Abstraction

FraiseQL's repository pattern provides a clean abstraction over the CQRS implementation. The Python layer handles type conversion while PostgreSQL manages business logic:

### Repository Pattern

```python
from fraiseql import CQRSRepository
from typing import Any
from uuid import UUID

class OrderRepository(CQRSRepository):
    """Domain repository for Order aggregate."""
    
    async def create_order(self, order_data: dict) -> dict:
        """Create a new order - passes JSON to PostgreSQL function."""
        result = await self.execute_function('fn_create_order', order_data)
        
        if result["success"]:
            # Fetch complete order from view
            order = await self.get_by_id('v_order_aggregate', result["data"]["id"])
            return {"success": True, "order": order}
        else:
            return {"success": False, "error": result["message"]}
    
    async def ship_order(self, order_id: UUID, tracking_number: str) -> dict:
        """Ship an order - business logic in PostgreSQL."""
        return await self.execute_function('fn_ship_order', {
            "order_id": str(order_id),
            "tracking_number": tracking_number
        })
    
    async def get_order(self, order_id: UUID) -> dict | None:
        """Get order aggregate from read model."""
        return await self.get_by_id('v_order_aggregate', order_id)
    
    async def find_customer_orders(
        self, 
        customer_id: UUID,
        status: str | None = None,
        limit: int = 10
    ) -> list[dict]:
        """Query orders for a customer."""
        filters = {'customer_id': {'eq': str(customer_id)}}
        if status:
            filters['status'] = {'eq': status}
        
        return await self.query(
            'v_order_aggregate',
            filters=filters,
            order_by='created_at DESC',
            limit=limit
        )
```

### Service Layer (When Needed)

For complex orchestration across multiple aggregates:

```python
from dataclasses import dataclass
from uuid import UUID

@dataclass
class OrderService:
    """Domain service for complex order operations."""
    
    order_repo: OrderRepository
    inventory_repo: InventoryRepository
    payment_repo: PaymentRepository
    
    async def fulfill_order(
        self,
        order_id: UUID,
        warehouse_id: UUID
    ) -> dict:
        """Orchestrate order fulfillment across bounded contexts."""
        
        # Get order details
        order = await self.order_repo.get_order(order_id)
        if not order:
            return {"success": False, "error": "Order not found"}
        
        # Check inventory across warehouses
        for item in order["data"]["items"]:
            available = await self.inventory_repo.check_availability(
                product_id=item["product"]["id"],
                warehouse_id=warehouse_id,
                quantity=item["quantity"]
            )
            if not available:
                return {
                    "success": False, 
                    "error": f"Insufficient inventory for {item['product']['name']}"
                }
        
        # Reserve inventory (transaction in PostgreSQL)
        reservation = await self.inventory_repo.reserve_items(
            order_id=order_id,
            warehouse_id=warehouse_id,
            items=[{
                "product_id": item["product"]["id"],
                "quantity": item["quantity"]
            } for item in order["data"]["items"]]
        )
        
        if reservation["success"]:
            # Update order status
            return await self.order_repo.execute_function(
                "fn_mark_order_fulfilling",
                {
                    "order_id": str(order_id),
                    "warehouse_id": str(warehouse_id),
                    "reservation_id": reservation["reservation_id"]
                }
            )
        else:
            return reservation
```

## Performance Optimization Strategies

### Aggregate Materialization

For complex aggregates, use materialized views or table views:

```sql
-- Materialized view for complex aggregates
CREATE MATERIALIZED VIEW mv_customer_360 AS
WITH customer_orders AS (
    SELECT 
        customer_id,
        COUNT(*) AS order_count,
        SUM(total_amount) AS lifetime_value,
        MAX(created_at) AS last_order_date,
        AVG(total_amount) AS avg_order_value
    FROM tb_orders
    WHERE status != 'cancelled'
    GROUP BY customer_id
),
customer_products AS (
    SELECT 
        o.customer_id,
        jsonb_agg(DISTINCT p.category_id) AS categories_purchased,
        COUNT(DISTINCT oi.product_id) AS unique_products
    FROM tb_orders o
    JOIN tb_order_items oi ON oi.order_id = o.id
    JOIN tb_products p ON p.id = oi.product_id
    GROUP BY o.customer_id
)
SELECT
    c.id,
    jsonb_build_object(
        '__typename', 'Customer360',
        'id', c.id,
        'profile', jsonb_build_object(
            'email', c.email,
            'name', c.name,
            'created_at', c.created_at
        ),
        'metrics', jsonb_build_object(
            'lifetime_value', COALESCE(co.lifetime_value, 0),
            'order_count', COALESCE(co.order_count, 0),
            'avg_order_value', COALESCE(co.avg_order_value, 0),
            'last_order_date', co.last_order_date,
            'unique_products', COALESCE(cp.unique_products, 0)
        ),
        'segments', (
            CASE 
                WHEN co.lifetime_value > 1000 THEN ARRAY['vip']
                WHEN co.order_count > 5 THEN ARRAY['loyal']
                WHEN co.last_order_date > CURRENT_DATE - INTERVAL '30 days' THEN ARRAY['active']
                ELSE ARRAY['dormant']
            END
        )
    ) AS data
FROM tb_customers c
LEFT JOIN customer_orders co ON co.customer_id = c.id
LEFT JOIN customer_products cp ON cp.customer_id = c.id;

-- Refresh strategy
CREATE INDEX idx_mv_customer_360 ON mv_customer_360(id);
REFRESH MATERIALIZED VIEW CONCURRENTLY mv_customer_360;
```

### Projection Optimization

Pre-compute expensive projections:

```sql
-- Denormalized projection for read performance
CREATE TABLE tv_order_search_projection (
    order_id UUID PRIMARY KEY,
    customer_id UUID NOT NULL,
    customer_email VARCHAR(255) NOT NULL,
    customer_name VARCHAR(200),
    order_number VARCHAR(50) NOT NULL,
    status VARCHAR(50) NOT NULL,
    total_amount NUMERIC(10,2) NOT NULL,
    item_count INT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    -- Denormalized search fields
    product_names TEXT[],
    product_skus TEXT[],
    search_vector tsvector,
    -- Pre-computed JSON for API
    data JSONB NOT NULL
);

-- Update projection on changes
CREATE OR REPLACE FUNCTION fn_update_order_projection()
RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO tv_order_search_projection
    SELECT
        o.id,
        o.customer_id,
        c.email,
        c.name,
        'ORD-' || LPAD(o.id::TEXT, 8, '0'),
        o.status,
        o.total_amount,
        (SELECT COUNT(*) FROM tb_order_items WHERE order_id = o.id),
        o.created_at,
        ARRAY(
            SELECT p.name 
            FROM tb_order_items oi 
            JOIN tb_products p ON p.id = oi.product_id 
            WHERE oi.order_id = o.id
        ),
        ARRAY(
            SELECT p.sku 
            FROM tb_order_items oi 
            JOIN tb_products p ON p.id = oi.product_id 
            WHERE oi.order_id = o.id
        ),
        to_tsvector('english', 
            c.name || ' ' || c.email || ' ' || 
            o.status || ' ' ||
            (SELECT string_agg(p.name || ' ' || p.sku, ' ')
             FROM tb_order_items oi 
             JOIN tb_products p ON p.id = oi.product_id 
             WHERE oi.order_id = o.id)
        ),
        (SELECT data FROM v_order_aggregate WHERE id = o.id)
    FROM tb_orders o
    JOIN tb_customers c ON c.id = o.customer_id
    WHERE o.id = COALESCE(NEW.id, NEW.order_id)
    ON CONFLICT (order_id) DO UPDATE SET
        status = EXCLUDED.status,
        data = EXCLUDED.data,
        search_vector = EXCLUDED.search_vector;
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Fast search using projection
CREATE INDEX idx_order_search_vector 
ON tv_order_search_projection 
USING gin(search_vector);

CREATE INDEX idx_order_search_status_date 
ON tv_order_search_projection(status, created_at DESC);
```

## Evolution Strategy

### Schema Versioning

Manage schema evolution without breaking existing clients:

```sql
-- Version 1: Original schema
CREATE OR REPLACE VIEW v_products_v1 AS
SELECT
    id,
    jsonb_build_object(
        '__typename', 'Product',
        'id', id,
        'name', name,
        'price', price
    ) AS data
FROM tb_products;

-- Version 2: Add new fields, maintain backward compatibility
CREATE OR REPLACE VIEW v_products_v2 AS
SELECT
    id,
    jsonb_build_object(
        '__typename', 'Product',
        'id', id,
        'name', name,
        'price', price,
        -- New fields with defaults for backward compatibility
        'category', COALESCE(
            (SELECT data FROM v_categories WHERE id = p.category_id),
            jsonb_build_object('id', null, 'name', 'Uncategorized')
        ),
        'inventory', COALESCE(
            (SELECT data FROM v_inventory WHERE product_id = p.id),
            jsonb_build_object('in_stock', 0, 'available', false)
        )
    ) AS data
FROM tb_products p;

-- Transition strategy: Keep both versions
CREATE OR REPLACE VIEW v_products AS
SELECT * FROM v_products_v2;

-- Deprecation notice
COMMENT ON VIEW v_products_v1 IS 
'DEPRECATED: Use v_products (v2). Will be removed in next major version.';
```

### Refactoring Aggregates

Safely refactor aggregate boundaries:

```sql
-- Step 1: Create new aggregate structure
CREATE TABLE tb_order_fulfillment (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    order_id UUID REFERENCES tb_orders(id),
    warehouse_id UUID,
    status VARCHAR(50),
    shipped_at TIMESTAMPTZ
);

-- Step 2: Migrate data
INSERT INTO tb_order_fulfillment (order_id, status, shipped_at)
SELECT id, shipping_status, shipped_at
FROM tb_orders
WHERE shipping_status IS NOT NULL;

-- Step 3: Create new view
CREATE OR REPLACE VIEW v_order_with_fulfillment AS
SELECT
    o.id,
    jsonb_build_object(
        '__typename', 'Order',
        'id', o.id,
        'status', o.status,
        'fulfillment', (
            SELECT data FROM v_order_fulfillment 
            WHERE order_id = o.id
        )
    ) AS data
FROM tb_orders o;

-- Step 4: Update old view to use new structure
CREATE OR REPLACE VIEW v_orders AS
SELECT * FROM v_order_with_fulfillment;
```

## Real-World Examples

### E-Commerce Domain Model

```sql
-- Product Catalog Bounded Context
CREATE SCHEMA catalog;

-- Category Aggregate
CREATE TABLE catalog.tb_categories (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    parent_id UUID REFERENCES catalog.tb_categories(id),
    name VARCHAR(100) NOT NULL,
    slug VARCHAR(100) NOT NULL UNIQUE,
    path ltree, -- Hierarchical path
    metadata JSONB DEFAULT '{}'
);

-- Product Aggregate Root
CREATE TABLE catalog.tb_products (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sku VARCHAR(50) NOT NULL UNIQUE,
    name VARCHAR(200) NOT NULL,
    category_id UUID REFERENCES catalog.tb_categories(id),
    base_price NUMERIC(10,2) NOT NULL,
    status VARCHAR(50) DEFAULT 'draft'
);

-- Product Variants (Aggregate Member)
CREATE TABLE catalog.tb_product_variants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    product_id UUID NOT NULL REFERENCES catalog.tb_products(id),
    sku_suffix VARCHAR(20) NOT NULL,
    attributes JSONB NOT NULL, -- {color: "red", size: "XL"}
    price_adjustment NUMERIC(10,2) DEFAULT 0,
    UNIQUE(product_id, sku_suffix)
);

-- Inventory Bounded Context
CREATE SCHEMA inventory;

-- Stock Aggregate
CREATE TABLE inventory.tb_stock (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    variant_id UUID NOT NULL,
    warehouse_id UUID NOT NULL,
    quantity INT NOT NULL DEFAULT 0,
    reserved INT NOT NULL DEFAULT 0,
    reorder_point INT DEFAULT 10,
    reorder_quantity INT DEFAULT 100,
    UNIQUE(variant_id, warehouse_id)
);

-- Cross-context Integration View
CREATE OR REPLACE VIEW public.v_product_availability AS
SELECT
    p.id AS product_id,
    jsonb_build_object(
        '__typename', 'ProductAvailability',
        'product', (
            SELECT jsonb_build_object(
                'id', p.id,
                'sku', p.sku,
                'name', p.name,
                'price', p.base_price,
                'status', p.status
            )
            FROM catalog.tb_products p
            WHERE p.id = p.id
        ),
        'variants', (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'id', v.id,
                    'sku', p.sku || '-' || v.sku_suffix,
                    'attributes', v.attributes,
                    'price', p.base_price + v.price_adjustment,
                    'stock', (
                        SELECT jsonb_build_object(
                            'total', SUM(s.quantity),
                            'available', SUM(s.quantity - s.reserved),
                            'warehouses', COUNT(DISTINCT s.warehouse_id)
                        )
                        FROM inventory.tb_stock s
                        WHERE s.variant_id = v.id
                    )
                )
            )
            FROM catalog.tb_product_variants v
            WHERE v.product_id = p.id
        )
    ) AS data
FROM catalog.tb_products p;
```

### SaaS Multi-Tenant Model

```sql
-- Tenant Isolation using Row-Level Security
CREATE TABLE public.tb_tenants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(50) NOT NULL UNIQUE,
    name VARCHAR(200) NOT NULL,
    plan VARCHAR(50) DEFAULT 'free',
    settings JSONB DEFAULT '{}'
);

-- Enable RLS
ALTER TABLE tb_tenants ENABLE ROW LEVEL SECURITY;

-- Base table with tenant isolation
CREATE TABLE public.tb_projects (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tb_tenants(id),
    name VARCHAR(200) NOT NULL,
    status VARCHAR(50) DEFAULT 'active',
    metadata JSONB DEFAULT '{}'
);

ALTER TABLE tb_projects ENABLE ROW LEVEL SECURITY;

-- RLS Policy
CREATE POLICY tenant_isolation ON tb_projects
    FOR ALL
    USING (tenant_id = current_setting('app.tenant_id')::UUID);

-- Tenant-aware view
CREATE OR REPLACE VIEW v_projects AS
SELECT
    p.id,
    p.status,
    jsonb_build_object(
        '__typename', 'Project',
        'id', p.id,
        'name', p.name,
        'status', p.status,
        'tenant', (
            SELECT jsonb_build_object(
                'id', t.id,
                'name', t.name,
                'plan', t.plan
            )
            FROM tb_tenants t
            WHERE t.id = p.tenant_id
        ),
        'team_members', (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'id', tm.user_id,
                    'role', tm.role,
                    'joined_at', tm.created_at
                )
            )
            FROM tb_project_members tm
            WHERE tm.project_id = p.id
        )
    ) AS data
FROM tb_projects p
WHERE p.tenant_id = current_setting('app.tenant_id')::UUID;

-- Tenant-aware repository
class TenantAwareRepository(CQRSRepository):
    async def with_tenant(self, tenant_id: UUID):
        """Set tenant context for queries."""
        await self.connection.execute(
            "SET LOCAL app.tenant_id = %s",
            str(tenant_id)
        )
        return self
```

## Best Practices

### 1. Aggregate Design Guidelines

- **Keep aggregates small**: Focus on consistency boundaries
- **Reference by ID**: Don't embed large aggregates
- **Use domain events**: Coordinate between aggregates
- **Version your aggregates**: Track changes over time

### 2. View Composition Patterns

- **Layer your views**: Build complex views from simpler ones
- **Materialize expensive aggregations**: Use tv_ tables for performance
- **Include filter columns**: Enable efficient WHERE clauses
- **Always include __typename**: Support GraphQL type resolution

### 3. Performance Considerations

- **Index strategically**: Focus on filter and join columns
- **Use JSONB indexes**: GIN indexes for JSONB queries
- **Batch operations**: Reduce round trips
- **Monitor slow queries**: Use EXPLAIN ANALYZE

### 4. Evolution Strategies

- **Version your schemas**: Maintain backward compatibility
- **Use view abstraction**: Hide schema changes
- **Document deprecations**: Clear migration paths
- **Test migrations**: Ensure data integrity

## Next Steps

- Explore [Database API Design Patterns](./database-api-patterns.md) for advanced API patterns
- Learn about [LLM-Native Architecture](./llm-native-architecture.md) for AI integration
- See practical examples in the [Blog API Tutorial](../tutorials/blog-api.md)