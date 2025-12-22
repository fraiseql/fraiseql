# API Reference

Complete API documentation for FraiseQL decorators, classes, and functions.

## Core Decorators

### Type System
- **[@type](../core/types-and-schema.md#type-decorator)** - Map PostgreSQL views to GraphQL types
  - Parameters: `sql_source`, `jsonb_column`, `table_view`, `pk_column`
- **[@input](../core/types-and-schema.md#input-types)** - Define GraphQL input types for mutations
- **[@success](../core/queries-and-mutations.md#success-failure-pattern)** - Define success response types
- **[@error](../core/queries-and-mutations.md#success-failure-pattern)** - Define failure/error response types

### Query & Mutation Decorators
- **[@query](../core/queries-and-mutations.md#query-decorator)** - Define GraphQL queries
  - Auto-generates `get_<name>` and `list_<name>` resolvers
- **[@mutation](../core/queries-and-mutations.md#mutation-decorator)** - Define GraphQL mutations
  - Supports success/failure patterns with explicit error handling

### Authorization
- **[@authorized](../advanced/authentication.md#authorization-decorator)** - Protect queries/mutations with role-based access
  - Parameters: `roles`, `permissions`, `custom_check`

## Database API

### Connection Management
- **[Database Pool](database.md#connection-pool)** - PostgreSQL connection pooling
  - `create_pool()` - Initialize connection pool
  - `acquire()` - Get connection from pool
  - `close()` - Cleanup connections

### Query Execution
- **[call_function()](database.md#calling-functions)** - Execute PostgreSQL functions
- **[execute_query()](database.md#raw-queries)** - Run raw SQL queries
- **[fetch_one() / fetch_all()](database.md#fetching-data)** - Retrieve query results

### Where Input Types
- **[create_graphql_where_input()](../advanced/where-input-types.md)** - Generate filtering types
  - Standard operators: `eq`, `neq`, `gt`, `gte`, `lt`, `lte`, `in`, `isnull`
  - Specialized operators: Network types, ltree hierarchy, date ranges
  - Nested array filtering: `AND`, `OR`, `NOT` logical operators

## Configuration

- **[FraiseQLConfig](../core/configuration.md)** - Application configuration
  - Database connection settings
  - APQ (Automatic Persisted Queries) configuration
  - Caching backend selection
  - Security and CORS settings

## Advanced Features

### Caching
- **[PostgresCache](../performance/index.md#postgresql-caching)** - PostgreSQL-based caching
  - `set()`, `get()`, `delete()`, `clear()` - Standard cache operations
  - `set_many()`, `get_many()` - Batch operations
  - TTL-based expiration

### Monitoring & Error Tracking
- **[init_error_tracker()](../production/monitoring.md#error-tracking)** - Configure error tracking
  - Automatic error fingerprinting and grouping
  - Stack trace capture
  - OpenTelemetry trace correlation

### APQ (Automatic Persisted Queries)
- **[APQConfig](../performance/apq-optimization-guide.md)** - Configure APQ
  - Storage backends: memory, PostgreSQL
  - Query hash validation
  - Multi-instance coordination

## Utilities

### Trinity Identifiers
- **[Trinity Pattern](../database/trinity-identifiers.md)** - Three-tier ID system
  - `pk_*` - Internal integer IDs for fast joins
  - `id` - Public UUID for API stability
  - `identifier` - Human-readable slugs for SEO

### Type Operators
- **[Type Operator Architecture](../architecture/type-operator-architecture.md)** - Advanced filtering
  - Network operators: `inet_eq`, `cidr_contains`
  - Hierarchy operators: `ancestor_of`, `descendant_of`
  - Range operators: `overlaps`, `contains`

## Auto-Generated Documentation

Full API reference with function signatures and parameter details:

**Coming Soon**: Auto-generated docs from source code docstrings (mkdocstrings)

For now, refer to:
- **[Source Code](../../src/fraiseql/)** - Comprehensive inline documentation
- **[Core Concepts Guide](../core/)** - Detailed explanations with examples
- **[Examples Directory](../../examples/)** - Real-world usage patterns
