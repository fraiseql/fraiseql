# FraiseQL Philosophy

Understanding FraiseQL's design principles and innovative approaches.

## Overview

FraiseQL is built on forward-thinking design principles that prioritize **developer experience**, **security by default**, and **PostgreSQL-native patterns**. Unlike traditional GraphQL frameworks, FraiseQL embraces conventions that reduce boilerplate while maintaining flexibility.

**Core Principles:**

1. **Automatic Database Injection** - Zero-config data access
2. **JSONB-First Architecture** - Embrace PostgreSQL's strengths
3. **Auto-Documentation** - Single source of truth
4. **Session Variable Injection** - Security without complexity
5. **Composable Patterns** - Framework provides tools, you control composition

## Automatic Database Injection

### The Problem with Traditional Frameworks

Most GraphQL frameworks require manual database setup in every resolver:

```python
# ❌ Traditional approach - repetitive and error-prone
@query
async def get_user(info, id: UUID) -> User:
    # Must manually get database from somewhere
    db = get_database_from_somewhere()
    # Or pass it through complex dependency injection
    return await db.find_one("users", {"id": id})
```

### FraiseQL's Solution

**FraiseQL automatically injects the database into `info.context["db"]`**:

```python
# ✅ FraiseQL - database automatically available
@query
async def get_user(info, id: UUID) -> User:
    db = info.context["db"]  # Always available!
    return await db.find_one("v_user", where={"id": id})
```

### How It Works

1. **Configuration** - Specify database URL once:
   ```python
   config = FraiseQLConfig(
       database_url="postgresql://localhost/mydb"
   )
   ```

2. **Automatic Setup** - FraiseQL creates and manages connection pool:
   ```python
   app = create_fraiseql_app(config=config)
   # Database pool created automatically
   ```

3. **Context Injection** - Every resolver gets `db` in context:
   ```python
   @query
   async def any_query(info) -> Any:
       db = info.context["db"]  # FraiseQLRepository instance
       # Ready to use immediately
   ```

### Benefits

- **Zero boilerplate** - No manual connection management
- **Type-safe** - `db` is always `FraiseQLRepository`
- **Connection pooling** - Automatic pool management
- **Transaction support** - Built-in transaction handling
- **Consistent** - Same API across all resolvers

### Advanced: Custom Context

You can extend context while keeping auto-injection:

```python
async def get_context(request: Request) -> dict:
    """Custom context with user + auto database injection."""
    return {
        # Your custom context
        "user_id": extract_user_from_jwt(request),
        "tenant_id": extract_tenant_from_jwt(request),
        # No need to add "db" - FraiseQL adds it automatically!
    }

app = create_fraiseql_app(
    config=config,
    context_getter=get_context  # Database still auto-injected
)
```

## JSONB-First Architecture

### Philosophy

FraiseQL embraces **PostgreSQL's JSONB** as a first-class storage mechanism, not just for flexible schemas, but as a performance and developer experience optimization.

### Traditional vs JSONB-First

**Traditional ORM Approach**:
```sql
-- Rigid schema, many columns
CREATE TABLE users (
    id UUID PRIMARY KEY,
    first_name VARCHAR(100),
    last_name VARCHAR(100),
    email VARCHAR(255),
    phone VARCHAR(20),
    address_line1 VARCHAR(255),
    address_line2 VARCHAR(255),
    city VARCHAR(100),
    -- ... 20 more columns
);
```

**FraiseQL JSONB-First Approach**:
```sql
-- Flexible, indexed, performant
CREATE TABLE tb_user (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    data JSONB NOT NULL
);

-- Indexes for commonly queried fields
CREATE INDEX idx_user_email ON tb_user USING GIN ((data->'email'));
CREATE INDEX idx_user_name ON tb_user USING GIN ((data->'name'));

-- View for GraphQL
CREATE VIEW v_user AS
SELECT
    id,
    tenant_id,
    data->>'first_name' as first_name,
    data->>'last_name' as last_name,
    data->>'email' as email,
    data
FROM tb_user;
```

### Why JSONB-First?

**1. Schema Evolution Without Migrations**:
```python
# Add new field - no migration needed!
@type(sql_source="v_user")
class User:
    """User account.

    Fields:
        id: User identifier
        email: Email address
        name: Full name
        preferences: User preferences (NEW! Just add it)
    """
    id: UUID
    email: str
    name: str
    preferences: UserPreferences | None = None  # Added without ALTER TABLE
```

**2. JSON Passthrough Performance**:
```python
# PostgreSQL JSONB → GraphQL JSON directly
# No Python object instantiation needed!
@query
async def user(info, id: UUID) -> User:
    db = info.context["db"]
    # Returns JSONB directly - 10-100x faster
    return await db.find_one("v_user", where={"id": id})
```

**3. Flexible Data Models**:
```sql
-- Different tenants can have different user fields
-- Tenant A users
{"first_name": "John", "last_name": "Doe", "department": "Sales"}

-- Tenant B users (different structure!)
{"full_name": "Jane Smith", "division": "Marketing", "employee_id": "E123"}
```

### JSONB Best Practices

**1. Use Views for GraphQL**:
```sql
CREATE VIEW v_product AS
SELECT
    id,
    tenant_id,
    data->>'name' as name,
    (data->>'price')::decimal as price,
    data->>'sku' as sku,
    data  -- Full JSONB for passthrough
FROM tb_product;
```

**2. Index Frequently Queried Fields**:
```sql
-- GIN index for contains queries
CREATE INDEX idx_product_search ON tb_product
USING GIN ((data->'name') gin_trgm_ops);

-- B-tree for exact matches
CREATE INDEX idx_product_sku ON tb_product ((data->>'sku'));
```

**3. Validate in PostgreSQL, Not Python**:
```sql
CREATE FUNCTION validate_user_data(data jsonb) RETURNS boolean AS $$
BEGIN
    -- Email required
    IF NOT (data ? 'email') THEN
        RAISE EXCEPTION 'email is required';
    END IF;

    -- Email format
    IF NOT (data->>'email' ~ '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}$') THEN
        RAISE EXCEPTION 'invalid email format';
    END IF;

    RETURN true;
END;
$$ LANGUAGE plpgsql;

-- Use in constraint
ALTER TABLE tb_user
ADD CONSTRAINT check_user_data
CHECK (validate_user_data(data));
```

### When NOT to Use JSONB

- **High-cardinality numeric queries** - Use regular columns for complex numeric aggregations
- **Foreign key relationships** - Use UUID columns, not nested JSONB
- **Frequently joined data** - Extract to separate table with foreign keys

```sql
-- ❌ Don't do this
CREATE TABLE tb_order (
    id UUID,
    data JSONB  -- Contains user_id, product_id
);

-- ✅ Do this
CREATE TABLE tb_order (
    id UUID,
    user_id UUID REFERENCES tb_user(id),      -- FK for joins
    product_id UUID REFERENCES tb_product(id), -- FK for joins
    data JSONB  -- Additional flexible data
);
```

## Auto-Documentation from Code

### Single Source of Truth

FraiseQL extracts documentation from Python docstrings, eliminating manual schema documentation:

```python
@type(sql_source="v_user")
class User:
    """User account with authentication and profile information.

    Users are created during registration and can access the system
    based on their assigned roles and permissions.

    Fields:
        id: Unique user identifier (UUID v4)
        email: Email address used for login (must be unique)
        first_name: User's first name
        last_name: User's last name
        created_at: Account creation timestamp
        is_active: Whether user account is active
    """

    id: UUID
    email: str
    first_name: str
    last_name: str
    created_at: datetime
    is_active: bool
```

**Result** - GraphQL schema includes all documentation:

```graphql
"""
User account with authentication and profile information.

Users are created during registration and can access the system
based on their assigned roles and permissions.
"""
type User {
  "Unique user identifier (UUID v4)"
  id: UUID!

  "Email address used for login (must be unique)"
  email: String!

  "User's first name"
  firstName: String!

  # ... etc
}
```

### Benefits for LLM Integration

This auto-documentation is perfect for LLM-powered applications:

1. **Rich Context** - LLMs see full descriptions via introspection
2. **Always Updated** - Docs can't get out of sync with code
3. **Consistent Format** - Standardized across entire API
4. **Zero Maintenance** - No separate documentation files

## Session Variable Injection

### Security by Default

FraiseQL **automatically sets PostgreSQL session variables** from GraphQL context:

```python
# Context from authenticated request
async def get_context(request: Request) -> dict:
    token = extract_jwt(request)
    return {
        "tenant_id": token["tenant_id"],
        "user_id": token["user_id"]
    }

# FraiseQL automatically executes:
# SET LOCAL app.tenant_id = '<tenant_id>';
# SET LOCAL app.contact_id = '<user_id>';
```

### Multi-Tenant Isolation

Views automatically filter by tenant:

```sql
CREATE VIEW v_order AS
SELECT *
FROM tb_order
WHERE tenant_id = current_setting('app.tenant_id')::uuid;
```

Now all queries are automatically tenant-isolated:

```python
@query
async def orders(info) -> list[Order]:
    db = info.context["db"]
    # Automatically filtered by tenant from JWT!
    return await db.find("v_order")
```

**Security Benefits**:

- ✅ Tenant ID from verified JWT, not user input
- ✅ Impossible to query other tenant's data
- ✅ Works at database level (defense in depth)
- ✅ Zero application-level filtering logic

## Composable Over Opinionated

### Framework Provides Tools

FraiseQL gives you composable utilities, not rigid patterns:

```python
from fraiseql.monitoring import HealthCheck, check_database

# Create health check
health = HealthCheck()

# Add only checks you need
health.add_check("database", check_database)

# Optionally add custom checks
health.add_check("redis", my_redis_check)
health.add_check("s3", my_s3_check)

# Use in your endpoints
@app.get("/health")
async def health_endpoint():
    return await health.run_checks()
```

### You Control Composition

Unlike opinionated frameworks that dictate:
- ❌ Where files go
- ❌ How to structure modules
- ❌ What patterns to use

FraiseQL provides:
- ✅ Building blocks (HealthCheck, @mutation, @query)
- ✅ Clear interfaces (CheckResult, CheckFunction)
- ✅ Flexibility in composition

## Performance Through Simplicity

### JSON Passthrough

Skip Python object creation entirely:

```python
# PostgreSQL JSONB → GraphQL JSON
# No intermediate Python objects!

@query
async def users(info) -> list[User]:
    db = info.context["db"]
    # Returns JSONB directly - 10-100x faster
    return await db.find("v_user")

# With Rust transformer: 80x faster
# With APQ: 3-5x additional speedup
# With TurboRouter: 2-3x additional speedup
```

### Database-First Operations

Move logic to PostgreSQL when possible:

```sql
-- Complex business logic in database
CREATE FUNCTION calculate_order_totals(order_id uuid)
RETURNS jsonb AS $$
    -- SQL aggregations, JOINs, window functions
    -- Much faster than Python loops
$$ LANGUAGE sql;
```

```python
@query
async def order_totals(info, id: UUID) -> OrderTotals:
    db = info.context["db"]
    # Database does the heavy lifting
    return await db.execute_function(
        "calculate_order_totals",
        {"order_id": id}
    )
```

## Conclusion

FraiseQL's philosophy:

1. **Automate the obvious** - Database injection, session variables, documentation
2. **Embrace PostgreSQL** - JSONB, functions, views, RLS
3. **Security by default** - Session variables, context injection
4. **Performance through simplicity** - JSON passthrough, minimal abstractions
5. **Composable patterns** - Tools, not opinions

These principles enable rapid development without sacrificing security or performance.

## See Also

- [Database API](../api-reference/database.md) - Auto-injected database methods
- [Session Variables](../api-reference/database.md#context-and-session-variables) - Automatic injection details
- [Decorators](../api-reference/decorators.md) - FraiseQL decorator patterns
- [Performance](../performance/index.md) - JSON passthrough and optimization layers
