# Design Patterns

Common design patterns and architectural approaches for FraiseQL applications.

## Core Patterns

### Trinity Identifiers
**[Trinity Identifiers Pattern](../database/trinity-identifiers.md)** - Three-tier ID system for optimal performance and UX

The trinity pattern uses three types of identifiers per entity:
- **`pk_*`** - Internal integer IDs for fast database joins
- **`id`** - Public UUID for API stability (never changes)
- **`identifier`** - Human-readable slugs for SEO and usability

**Example:**
```python
@fraiseql.type(sql_source="v_post")
class Post:
    pk_post: int              # Internal: Fast joins, never exposed to API
    id: UUID                  # Public: Stable API identifier
    identifier: str           # Human: "how-to-use-fraiseql" (SEO-friendly)
```

**When to use**: Production applications requiring SEO-friendly URLs and stable API contracts.

---

### CQRS (Command Query Responsibility Segregation)

**[CQRS Pattern](../advanced/bounded-contexts.md#cqrs-pattern)** - Separate read and write models

FraiseQL implements CQRS at the database level:
- **Queries (Reads)**: Use views (`v_*`) or table views (`tv_*`) with pre-composed JSONB
- **Mutations (Writes)**: Use functions (`fn_*`) with business logic in PostgreSQL

**Benefits:**
- Read models optimized for GraphQL responses (no N+1 queries)
- Write models contain validation and business rules
- Clear separation of concerns
- Database-enforced consistency

**Example:**
```sql
-- Read model (view)
CREATE VIEW v_user AS
SELECT id, jsonb_build_object('id', id, 'name', name, 'email', email) as data
FROM tb_user;

-- Write model (function)
CREATE FUNCTION fn_create_user(p_email TEXT, p_name TEXT) RETURNS JSONB AS $$
BEGIN
    -- Validation logic here
    INSERT INTO tb_user (email, name) VALUES (p_email, p_name);
    RETURN jsonb_build_object('success', true);
END;
$$ LANGUAGE plpgsql;
```

**See also:** [Database Patterns Guide](../advanced/database-patterns.md)

---

### Table Views (tv_*) - Explicit Sync Pattern

**[Explicit Sync Pattern](../core/explicit-sync.md)** - Denormalized JSONB tables for complex queries

Table views (`tv_*`) are **denormalized tables** with JSONB columns, explicitly synchronized from source tables:

```sql
-- Table view (denormalized storage)
CREATE TABLE tv_user (
    id INT PRIMARY KEY,
    data JSONB NOT NULL,
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Sync function (called by mutations)
CREATE FUNCTION fn_sync_tv_user(p_user_id INT) RETURNS VOID AS $$
BEGIN
    INSERT INTO tv_user (id, data)
    SELECT id, data FROM v_user WHERE id = p_user_id
    ON CONFLICT (id) DO UPDATE SET data = EXCLUDED.data;
END;
$$ LANGUAGE plpgsql;
```

**When to use:**
- Complex queries requiring joins across multiple tables
- Performance-critical read paths
- Data that doesn't change frequently

**Trade-offs:**
- ✅ Instant lookups (pre-computed joins)
- ✅ Embedded relations (no N+1 queries)
- ❌ Requires explicit synchronization
- ❌ Storage overhead (denormalization)

---

## Advanced Patterns

### Multi-Tenancy
**[Multi-Tenancy Pattern](../advanced/multi-tenancy.md)** - Isolate data per tenant

Strategies:
- **Row-Level Security (RLS)**: PostgreSQL enforces tenant isolation
- **Schema-per-tenant**: Separate schemas for each customer
- **Database-per-tenant**: Complete isolation (enterprise)

**See:** [Multi-Tenancy Guide](../advanced/multi-tenancy.md)

---

### Event Sourcing
**[Event Sourcing Pattern](../advanced/event-sourcing.md)** - Store events instead of current state

FraiseQL supports event sourcing with PostgreSQL:
- Store domain events in append-only tables
- Project events into read models (views or table views)
- Replay events for rebuilding state

**Example use cases:**
- Audit logging with full history
- CQRS with event-driven architecture
- Temporal queries ("what was the state on date X?")

**See:** [Event Sourcing Guide](../advanced/event-sourcing.md)

---

### Bounded Contexts
**[Bounded Contexts Pattern](../advanced/bounded-contexts.md)** - Organize code by domain

Domain-Driven Design applied to FraiseQL:
- Separate modules per business domain
- Shared database with schema organization
- Clear boundaries between contexts

**Example structure:**
```
app/
├── domain/
│   ├── users/        # User management context
│   ├── posts/        # Content management context
│   └── payments/     # Billing context
└── shared/           # Shared types and utilities
```

**See:** [Bounded Contexts Guide](../advanced/bounded-contexts.md)

---

## Database Patterns

### Naming Conventions
**[DDL Organization](../core/ddl-organization.md)** - Consistent naming for clarity

- `tb_*` - Base tables (source of truth)
- `v_*` - Views (JSONB-generating queries for real-time data)
- `tv_*` - Table views (denormalized JSONB tables)
- `fn_*` - Functions (mutations and business logic)

---

### Hybrid Tables Pattern
**[Database Patterns](../advanced/database-patterns.md#hybrid-tables)** - Mix relational and JSONB storage

Store structured data in columns, flexible data in JSONB:

```sql
CREATE TABLE tb_product (
    id INT PRIMARY KEY,
    name TEXT NOT NULL,                    -- Structured
    price DECIMAL(10,2) NOT NULL,          -- Structured
    metadata JSONB DEFAULT '{}'::JSONB,    -- Flexible
    tags TEXT[]                            -- Array
);
```

**When to use:** Products, settings, or entities with variable attributes.

---

## Authentication & Authorization Patterns

**[Authentication Guide](../advanced/authentication.md)** - Common auth patterns

Strategies:
- JWT tokens with PostgreSQL validation
- Session-based authentication
- OAuth2 integration
- Row-Level Security for authorization

**Authorization decorator:**
```python
@authorized(roles=["admin", "editor"])
@fraiseql.mutation
class DeletePost:
    input: DeletePostInput
    success: DeleteSuccess
```

---

## Real-World Examples

### Blog API Patterns
- **Simple**: [blog_simple](../../examples/blog_simple/) - Basic CRUD
- **Intermediate**: [blog_api](../../examples/blog_api/) - Nested relations
- **Enterprise**: [blog_enterprise](../../examples/blog_enterprise/) - Full CQRS + bounded contexts

### E-commerce Patterns
- [ecommerce](../../examples/ecommerce/) - Product catalog, cart, orders
- [ecommerce_api](../../examples/ecommerce_api/) - Advanced filtering

### SaaS Patterns
- [saas-starter](../../examples/saas-starter/) - Multi-tenancy template
- [apq_multi_tenant](../../examples/apq_multi_tenant/) - APQ + multi-tenancy

---

## Pattern Selection Guide

| Pattern | Use When | Complexity | Performance |
|---------|----------|------------|-------------|
| **Trinity IDs** | Production APIs with SEO needs | Medium | High |
| **CQRS** | Separating reads from writes | Medium | Very High |
| **Table Views (tv_*)** | Complex joins, performance-critical | High | Excellent |
| **Regular Views (v_*)** | Real-time data, simple joins | Low | Good |
| **Event Sourcing** | Full audit trail required | High | Medium |
| **Multi-Tenancy (RLS)** | SaaS applications | Medium | Good |
| **Bounded Contexts** | Large applications (5+ domains) | High | N/A |

---

## Additional Resources

- **[Core Concepts](../core/concepts-glossary.md)** - Terminology and mental models
- **[Architecture Decisions](../architecture/decisions/)** - ADRs explaining why patterns were chosen
- **[Database Patterns](../advanced/database-patterns.md)** - Detailed database design patterns
- **[Examples Directory](../../examples/)** - Real implementations

**Need help choosing a pattern?** See [Architecture Decision Records](../architecture/decisions/) for context on trade-offs.
