# Domain-Driven Design Database Layer Guide for FraiseQL - Outline

## 1. Introduction
- Overview of DDD principles in database design
- How FraiseQL's JSONB architecture aligns with DDD concepts
- Benefits of DDD-organized database structures

## 2. Core Concepts Mapping

### 2.1 DDD to PostgreSQL Mapping
- **Aggregates** → Tables with JSONB data columns
- **Entities** → Rows with unique identifiers
- **Value Objects** → JSONB nested objects or PostgreSQL composite types
- **Domain Events** → Event tables with triggers
- **Business Rules** → PostgreSQL functions and constraints
- **Bounded Contexts** → PostgreSQL schemas
- **Repositories** → Views and table-returning functions

### 2.2 FraiseQL-Specific Considerations
- JSONB data column as aggregate root storage
- Type safety through Python decorators
- GraphQL schema generation implications

## 3. Directory Structure Organization

### 3.1 Top-Level Structure
```
db/
├── contexts/           # Bounded contexts
│   ├── catalog/       # Example: Product catalog context
│   ├── ordering/      # Example: Order management context
│   └── identity/      # Example: User identity context
├── shared/            # Shared value objects and types
├── migrations/        # Sequential migration files
└── seeds/            # Test and development data
```

### 3.2 Entity/Aggregate Directory Structure
```
contexts/catalog/
├── product/           # Product aggregate
│   ├── schema.sql    # Table definition (current state)
│   ├── types.sql     # Custom types and enums
│   ├── functions/    # Business logic functions
│   ├── views/        # Query views (repository pattern)
│   ├── triggers/     # Domain event triggers
│   ├── indexes.sql   # Performance indexes
│   └── policies.sql  # Row-level security
├── category/         # Category aggregate
└── _shared/          # Context-specific shared resources
```

## 4. File Organization Within Entity Directories

### 4.1 Schema Files (schema.sql)
- Current state DDL (CREATE TABLE statements)
- Primary table with JSONB data column
- Supporting tables (many-to-many, event storage)
- Check constraints for invariants

### 4.2 Type Definitions (types.sql)
- Domain-specific enums
- Composite types for value objects
- Custom domains with constraints

### 4.3 Functions Directory Structure
```
functions/
├── commands/         # State-changing operations
│   ├── create_product.sql
│   ├── update_product_price.sql
│   └── discontinue_product.sql
├── queries/          # Read operations
│   ├── find_product_by_sku.sql
│   └── search_products.sql
├── validators/       # Business rule validation
│   ├── validate_price_change.sql
│   └── check_inventory_rules.sql
└── helpers/          # Internal utility functions
```

### 4.4 Views Directory Structure
```
views/
├── product_summary.sql      # Simplified read model
├── product_catalog.sql      # Public API view
├── product_search.sql       # Full-text search view
└── materialized/           # Performance optimization
    └── product_stats.sql
```

### 4.5 Triggers Directory
```
triggers/
├── product_created.sql      # Domain event emission
├── price_changed.sql        # Business rule enforcement
└── audit_trail.sql         # Change tracking
```

## 5. Separation of Current State and Migrations

### 5.1 Current State (DDL)
- Always represents the latest schema
- Used for fresh installations
- Source of truth for structure
- Version controlled for history

### 5.2 Migration Strategy
```
migrations/
├── 001_initial_catalog_context.sql
├── 002_add_product_aggregate.sql
├── 003_add_category_aggregate.sql
├── 004_product_pricing_rules.sql
└── 005_add_inventory_tracking.sql
```

### 5.3 Migration Patterns
- Forward-only migrations
- Idempotent operations
- Data migrations separate from schema
- Rollback strategies

## 6. Implementing DDD Patterns

### 6.1 Aggregate Implementation
```sql
-- Example: Product aggregate with JSONB
CREATE TABLE catalog.products (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sku TEXT UNIQUE NOT NULL,
    data JSONB NOT NULL,
    version INT NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,

    -- Invariants as constraints
    CONSTRAINT valid_product_data CHECK (
        data ? 'name' AND
        data ? 'price' AND
        (data->>'price')::DECIMAL > 0
    )
);
```

### 6.2 Value Objects
- As JSONB nested objects
- As PostgreSQL composite types
- Validation in functions
- Immutability patterns

### 6.3 Domain Events
```sql
-- Event storage table
CREATE TABLE catalog.domain_events (
    id BIGSERIAL PRIMARY KEY,
    aggregate_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL,
    event_type TEXT NOT NULL,
    event_data JSONB NOT NULL,
    occurred_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

-- Trigger for event emission
CREATE TRIGGER emit_product_created_event
    AFTER INSERT ON catalog.products
    FOR EACH ROW
    EXECUTE FUNCTION catalog.emit_domain_event('ProductCreated');
```

### 6.4 Business Rules as Functions
```sql
-- Command with business logic
CREATE FUNCTION catalog.update_product_price(
    p_product_id UUID,
    p_new_price DECIMAL,
    p_reason TEXT
) RETURNS catalog.products AS $$
DECLARE
    v_product catalog.products;
    v_old_price DECIMAL;
BEGIN
    -- Validate business rules
    SELECT * INTO v_product FROM catalog.products WHERE id = p_product_id FOR UPDATE;
    v_old_price := (v_product.data->>'price')::DECIMAL;

    IF p_new_price > v_old_price * 2 THEN
        RAISE EXCEPTION 'Price increase cannot exceed 100%';
    END IF;

    -- Update with event emission
    UPDATE catalog.products
    SET data = jsonb_set(data, '{price}', to_jsonb(p_new_price)),
        version = version + 1,
        updated_at = CURRENT_TIMESTAMP
    WHERE id = p_product_id
    RETURNING * INTO v_product;

    -- Emit domain event
    INSERT INTO catalog.domain_events (aggregate_id, aggregate_type, event_type, event_data)
    VALUES (
        p_product_id,
        'Product',
        'PriceChanged',
        jsonb_build_object(
            'old_price', v_old_price,
            'new_price', p_new_price,
            'reason', p_reason
        )
    );

    RETURN v_product;
END;
$$ LANGUAGE plpgsql;
```

### 6.5 Repository Pattern via Views
```sql
-- Repository view for GraphQL queries
CREATE VIEW catalog.product_repository AS
SELECT
    id,
    sku,
    data->>'name' AS name,
    (data->>'price')::DECIMAL AS price,
    data->'categories' AS categories,
    data->>'description' AS description,
    created_at,
    updated_at
FROM catalog.products
WHERE data->>'status' != 'discontinued';

-- Complex query function
CREATE FUNCTION catalog.find_products_by_category(
    p_category_id UUID
) RETURNS TABLE (
    id UUID,
    sku TEXT,
    name TEXT,
    price DECIMAL
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        p.id,
        p.sku,
        p.data->>'name' AS name,
        (p.data->>'price')::DECIMAL AS price
    FROM catalog.products p
    WHERE p.data->'categories' @> to_jsonb(p_category_id::TEXT);
END;
$$ LANGUAGE plpgsql;
```

## 7. Complete Example: Order Aggregate

### 7.1 Directory Structure
```
contexts/ordering/
├── order/
│   ├── schema.sql
│   ├── types.sql
│   ├── functions/
│   │   ├── commands/
│   │   │   ├── create_order.sql
│   │   │   ├── add_order_item.sql
│   │   │   ├── submit_order.sql
│   │   │   └── cancel_order.sql
│   │   ├── queries/
│   │   │   ├── get_order_details.sql
│   │   │   └── find_orders_by_customer.sql
│   │   └── validators/
│   │       └── validate_order_submission.sql
│   ├── views/
│   │   ├── order_summary.sql
│   │   └── order_history.sql
│   ├── triggers/
│   │   ├── order_submitted.sql
│   │   └── order_state_changed.sql
│   └── indexes.sql
└── _shared/
    ├── types/
    │   └── money.sql
    └── functions/
        └── calculate_tax.sql
```

### 7.2 Implementation Examples
[Detailed code examples for each file type]

## 8. Best Practices

### 8.1 Naming Conventions
- Snake_case for PostgreSQL objects
- Consistent prefixes for object types
- Domain language in naming

### 8.2 JSONB Design Patterns
- Denormalization strategies
- Index optimization for JSONB queries
- Schema validation approaches

### 8.3 Transaction Boundaries
- Aggregate consistency
- Event sourcing considerations
- Optimistic locking with version fields

### 8.4 Testing Strategies
- Unit tests for functions
- Integration tests for aggregates
- Event verification

## 9. Advanced Patterns

### 9.1 CQRS Implementation
- Separate read and write models
- Materialized views for queries
- Event-driven view updates

### 9.2 Event Sourcing
- Event store design
- Snapshot strategies
- Projection rebuilding

### 9.3 Saga Implementation
- Cross-aggregate transactions
- Compensation logic
- State machines in PostgreSQL

## 10. Integration with FraiseQL

### 10.1 Type Mapping
- PostgreSQL types to GraphQL scalars
- JSONB to GraphQL types
- Custom scalar implementation

### 10.2 Repository Pattern
- Views as GraphQL queries
- Functions as GraphQL mutations
- Subscription support via events

### 10.3 Performance Optimization
- Query analysis
- Index strategies for JSONB
- Connection pooling considerations

## 11. Tooling and Automation

### 11.1 Migration Tools
- Flyway/Liquibase integration
- Custom migration runners
- Version tracking

### 11.2 Code Generation
- Type definitions from schema
- GraphQL schema from database
- Repository interfaces

### 11.3 Development Workflow
- Local development setup
- CI/CD integration
- Database versioning

## 12. Conclusion
- Summary of key concepts
- Benefits of DDD approach
- Next steps for implementation
