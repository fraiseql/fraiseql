<!-- Skip to main content -->
---

title: Execution Model Architecture
description: The **execution model** defines how the Rust runtime handles three orthogonal execution patterns:
keywords: ["design", "scalability", "performance", "patterns", "security"]
tags: ["documentation", "reference"]
---

# Execution Model Architecture

**Version:** 1.0
**Status:** Draft
**Audience:** Runtime developers, database architects, infrastructure teams

---

## 1. Overview

The **execution model** defines how the Rust runtime handles three orthogonal execution patterns:

1. **Queries** — Request-response: client asks, server answers (pull model)
2. **Mutations** — Request-response with side effects: client commands, server executes
3. **Subscriptions** — Event-driven: server pushes events as data changes (push model, v2.0+)

All three patterns compile to deterministic execution plans; only the delivery model differs.

**Core principles (apply to all three):**

- All execution is **planned at compile time** — runtime has no decisions to make
- **Authorization is metadata** — checked statically, not during execution
- **Database calls are fixed** — no dynamic joins or discovery at runtime
- **Result projection is deterministic** — JSONB aggregation specified in views
- **No resolvers** — all logic is in stored procedures or views
- **Subscriptions are database-driven** — events originate from database transactions, not application logic

---

## 2. Query Execution Pipeline

### 2.1 High-Level Flow

```text
<!-- Code example in TEXT -->
GraphQL Query (from client)
    ↓
Rust Runtime receives request
    ↓
 Query Result Cache & APQ Resolution
  - Check if query is in cache (if caching enabled)
  - Resolve persisted query (if APQ enabled)
  - Return cached result or proceed to validation
    ↓
 GraphQL Validation
  - Check schema conformance
  - Check argument types
  - Compile authorization rules
    ↓
 Authorization Enforcement
  - Extract auth context (JWT, session)
  - Check requires_auth / requires_role / requires_claim
  - Determine if query is allowed
    ↓
 Query Planning
  - Look up pre-compiled execution plan (from CompiledSchema)
  - Determine database call(s) needed
  - Plan result projection
    ↓
 Database Execution
  - Execute SQL against database
  - Stream or collect results
    ↓
 Result Projection
  - Extract fields from JSONB response
  - Nest results according to type graph
  - Apply field-level auth masks
    ↓
 Cache Invalidation Emission
  - Emit cache invalidation events
  - Return response to client
```text
<!-- Code example in TEXT -->

---

## 2.2 Phase 0: Query Result Cache & APQ Resolution

This optional phase occurs **before validation** to optimize response time for frequently-executed queries:

### 2.2.1 Query Result Caching

If caching is enabled, the runtime checks if this exact query (or query hash) has been executed before:

```python
<!-- Code example in Python -->
def check_cache(
    query: str,
    variables: dict,
    auth_context: AuthContext,
    cache_layer
) -> dict | None:
    """Check if query result is cached."""

    # Generate cache key (includes tenant isolation)
    cache_key = generate_cache_key(
        query=query,
        variables=variables,
        tenant_id=auth_context.tenant_id
    )

    # Look up in cache
    cached_result = cache_layer.get(cache_key)

    if cached_result:
        # Check if cache entry is still valid
        if not is_cache_expired(cached_result):
            return cached_result.data  # Cache hit!

    return None  # Cache miss, continue to validation
```text
<!-- Code example in TEXT -->

**Cache Layers Supported:**

- **Memory Cache** — Fast, in-process, volatile
- **Database Cache** — Persistent (PostgreSQL UNLOGGED tables)
- **Custom Backend** — Redis, Memcached, or other

**Cache Invalidation:**
Cache entries are invalidated when mutations trigger cascading changes. See **docs/specs/caching.md** for detailed cache architecture and invalidation strategies using graphql-cascade.

### 2.2.2 Automatic Persisted Queries (APQ) Resolution

If APQ is enabled, the runtime resolves persisted queries by their hash:

```python
<!-- Code example in Python -->
def resolve_apq(
    apq_hash: str,
    schema: CompiledSchema
) -> str | None:
    """Resolve APQ hash to full query string."""

    # Look up persisted query by hash (SHA-256)
    query = schema.apq_storage.get(apq_hash)

    if query is None and apq_mode == "REQUIRED":
        # Reject: client must use persisted query
        return {
            "errors": [{
                "message": f"Persisted query not found: {apq_hash}",
                "extensions": {"code": "APQ_NOT_FOUND"}
            }]
        }

    return query  # Return full query for processing
```text
<!-- Code example in TEXT -->

**APQ Security Modes:**

- **OPTIONAL** — Accept both persisted and ad-hoc queries
- **REQUIRED** — Only accept persisted queries (enforces allowlist)
- **DISABLED** — Ignore APQ extensions entirely

See **docs/specs/persisted-queries.md** for complete APQ specification including security considerations.

### 2.2.3 Phase 0 Resolution Algorithm

```python
<!-- Code example in Python -->
async def resolve_phase_0(request) -> dict | None:
    """Resolve cache or APQ, return early if possible."""

    # If APQ is enabled, resolve persisted query
    if schema.apq_enabled and request.apq_hash:
        query = resolve_apq(request.apq_hash, schema)
        if query is None:
            return apq_error_response  # APQ resolution failed
        request.query = query  # Use resolved query

    # If caching is enabled, check for cache hit
    if schema.cache_enabled:
        cached_result = check_cache(
            request.query,
            request.variables,
            request.auth_context,
            cache_layer
        )
        if cached_result:
            return {
                "data": cached_result,
                "extensions": {"cached": True}  # Indicate cache hit
            }

    # Cache miss or caching disabled — continue to Phase 1
    return None
```text
<!-- Code example in TEXT -->

**Performance Impact:**

- Cache hit: Response in 1-10 ms (depending on cache layer)
- APQ resolution: < 1 ms
- Cache miss: Continue to validation (normal path)

---

## 3. Phase 1: GraphQL Validation

### 3.1 Static vs Runtime Validation

**Compile-time validation** (in compiler):

- Type closure
- Field existence
- Argument types
- Authorization references

**Runtime validation** (in executor):

- Argument values conform to declared types
- Required arguments provided
- Query structure conforms to schema

### 3.2 Validation Algorithm

```python
<!-- Code example in Python -->
def validate_graphql_query(query_ast: QueryAST, schema: CompiledSchema):
    """Validate GraphQL query against schema."""

    errors = []

    # Check query is known
    if query_ast.name not in schema.queries:
        errors.append(f"Unknown query: {query_ast.name}")
        return errors

    query_def = schema.queries[query_ast.name]

    # Check arguments
    for arg_name, arg_value in query_ast.arguments.items():
        if arg_name not in query_def.arguments:
            errors.append(f"Unknown argument: {arg_name}")
            continue

        arg_def = query_def.arguments[arg_name]
        if not is_type_compatible(arg_value.type, arg_def.type):
            errors.append(f"Argument {arg_name} type mismatch")

    # Check requested fields
    validate_fields(query_ast.selection_set, query_def.return_type, schema, errors)

    return errors
```text
<!-- Code example in TEXT -->

### 3.3 Error Handling

If validation fails, return GraphQL error response:

```json
<!-- Code example in JSON -->
{
  "errors": [
    {
      "message": "Unknown field 'fooBar' on type 'User'",
      "locations": [{"line": 3, "column": 5}],
      "extensions": {"code": "GRAPHQL_VALIDATION_ERROR"}
    }
  ]
}
```text
<!-- Code example in TEXT -->

---

## 4. Phase 2: Authorization Enforcement

### 4.1 Authorization Architecture

All authorization is **declarative metadata** in the CompiledSchema:

```json
<!-- Code example in JSON -->
{
  "authorization": {
    "queries": {
      "me": {
        "requires_auth": true,
        "requires_roles": ["user"],
        "requires_claims": []
      },
      "admin_users": {
        "requires_auth": true,
        "requires_roles": ["admin"],
        "requires_claims": ["tenant_id"]
      }
    },
    "types": {
      "User": {
        "password_hash": {
          "requires_roles": ["admin"]
        }
      }
    }
  }
}
```text
<!-- Code example in TEXT -->

### 4.2 Auth Context Extraction

The runtime receives auth context from the request (typically from JWT or session):

```python
<!-- Code example in Python -->
class AuthContext:
    subject: str           # User ID
    roles: list[str]       # ["user", "admin"]
    tenant_id: str         # Multi-tenant isolation
    email: str
    # ... custom fields
```text
<!-- Code example in TEXT -->

**Extraction happens externally** (middleware layer):

```python
<!-- Code example in Python -->
# Middleware (outside FraiseQL runtime)
def extract_auth_context(request) -> AuthContext:
    token = extract_jwt(request.headers["Authorization"])
    return AuthContext(
        subject=token.sub,
        roles=token.realm_access.roles,
        tenant_id=token.tenant_id,
        email=token.email
    )
```text
<!-- Code example in TEXT -->

### 4.3 Authorization Decision Algorithm

```python
<!-- Code example in Python -->
def authorize_query(
    query_name: str,
    auth_context: AuthContext,
    schema: CompiledSchema
) -> bool:
    """Determine if query is allowed."""

    query_auth = schema.authorization.queries[query_name]

    # Check: requires_auth
    if query_auth.requires_auth and not auth_context.subject:
        return False  # Not authenticated

    # Check: requires_roles
    if query_auth.requires_roles:
        user_roles = set(auth_context.roles)
        required_roles = set(query_auth.requires_roles)
        if not user_roles.intersection(required_roles):
            return False  # User lacks required role

    # Check: requires_claims
    for claim in query_auth.requires_claims:
        if not hasattr(auth_context, claim):
            return False  # Required claim missing

    return True
```text
<!-- Code example in TEXT -->

### 4.4 Field-Level Authorization

After results are returned, filter fields based on field-level auth:

```python
<!-- Code example in Python -->
# Example: User type, password_hash field requires admin role
user_data = {
    "id": "123",
    "email": "user@example.com",
    "password_hash": "$2a$10$..."  # ← Should be filtered out
}

# If user is NOT admin, remove field
if "admin" not in auth_context.roles:
    del user_data["password_hash"]

# Return to client
return user_data  # {"id": "123", "email": "user@example.com"}
```text
<!-- Code example in TEXT -->

### 4.5 Authorization Failure Response

If authorization fails:

```json
<!-- Code example in JSON -->
{
  "errors": [
    {
      "message": "Unauthorized: Query 'adminUsers' requires role 'admin'",
      "extensions": {
        "code": "FORBIDDEN",
        "required_role": "admin",
        "user_roles": ["user"]
      }
    }
  ]
}
```text
<!-- Code example in TEXT -->

---

## 4.5. Phase 2.5: Aggregation Resolution

For queries targeting fact tables (tables with `tf_*` prefix).5 performs aggregation planning and validation before general query planning.

### 4.5.1 Fact Table Detection

The compiler identifies fact tables during schema compilation by:

1. Detecting `tf_*` table prefix
2. Identifying measure columns (numeric types: INT, DECIMAL, FLOAT)
3. Detecting dimension JSONB column (default: `data`)
4. Mapping denormalized filter columns

At runtime, when a query targets a fact table aggregate query (e.g., `sales_aggregate`).5 is triggered.

### 4.5.2 GROUP BY Clause Generation

Aggregation resolver:

1. Parses `groupBy` input to extract dimension paths
2. Generates SQL GROUP BY expressions:
   - Direct SQL columns: `GROUP BY customer_id`
   - JSONB dimensions: `GROUP BY data->>'category'` (PostgreSQL)
   - Nested paths: `GROUP BY data#>>'{customer,segment}'`
   - Temporal buckets: `GROUP BY DATE_TRUNC('day', occurred_at)`

### 4.5.3 Aggregate Function Selection

For each requested aggregate measure:

1. Validate measure column exists and is numeric
2. Select database-specific aggregate function from capability manifest
   - PostgreSQL: COUNT, SUM, AVG, MIN, MAX, STDDEV, VARIANCE
   - MySQL: COUNT, SUM, AVG, MIN, MAX
   - SQLite: COUNT, SUM, AVG, MIN, MAX
   - SQL Server: COUNT, SUM, AVG, MIN, MAX, STDEV, VAR
3. Generate SQL: `SUM(revenue) AS revenue_sum`

### 4.5.4 Conditional Aggregates

For filtered aggregates (e.g., `revenue_sum(filter: {status: "completed"})`):

- **PostgreSQL**: Use FILTER clause: `SUM(revenue) FILTER (WHERE status = 'completed')`
- **Others**: Emulate with CASE WHEN: `SUM(CASE WHEN status = 'completed' THEN revenue ELSE 0 END)`

### 4.5.5 HAVING Clause Validation

For post-aggregation filters (`having` input):

1. Validate references are to aggregated measures (not raw columns)
2. Generate HAVING SQL: `HAVING SUM(revenue) > $1`
3. Bind filter values as query parameters

### 4.5.6 Temporal Bucketing

For temporal dimensions (e.g., `occurred_at_day`, `occurred_at_month`):

- **PostgreSQL**: `DATE_TRUNC('day', occurred_at)`
- **MySQL**: `DATE_FORMAT(occurred_at, '%Y-%m-%d')`
- **SQLite**: `strftime('%Y-%m-%d', occurred_at)`
- **SQL Server**: `DATEPART(day, occurred_at)`

### 4.5.7 Execution Plan

.5 produces an AggregationExecutionPlan:

```json
<!-- Code example in JSON -->
{
  "type": "aggregation",
  "table": "tf_sales",
  "measures": [
    {"column": "revenue", "function": "SUM", "alias": "revenue_sum"},
    {"column": "quantity", "function": "AVG", "alias": "quantity_avg"}
  ],
  "dimensions": [
    {"path": "data->>'category'", "alias": "category"},
    {"temporal": {"column": "occurred_at", "bucket": "day", "alias": "occurred_at_day"}}
  ],
  "where": {...},
  "having": {"revenue_sum": {"_gt": 10000}},
  "order_by": [{"column": "revenue_sum", "direction": "DESC"}],
  "limit": 100
}
```text
<!-- Code example in TEXT -->

This plan is passed to Phase 3 (Query Planning) which converts it to database-specific SQL in Phase 4.

**Related documentation**:

- `docs/architecture/analytics/aggregation-model.md` - Complete aggregation architecture
- `docs/specs/aggregation-operators.md` - Database-specific aggregate functions
- `docs/specs/analytical-schema-conventions.md` - Fact table naming and structure

---

## 5. Phase 3: Query Planning

### 5.1 Pre-Compiled Execution Plans

Every query/mutation has a **pre-compiled execution plan** stored in the CompiledSchema:

```json
<!-- Code example in JSON -->
{
  "queries": [
    {
      "name": "users",
      "execution_plan": {
        "type": "view_query",
        "view": "v_user",
        "where_column": null,
        "order_by_columns": ["created_at"],
        "limit_column": "limit",
        "offset_column": "offset",
        "projection": {
          "id": "column:id",
          "email": "column:email",
          "name": "column:name",
          "posts": "jsonb:data->>'posts'",
          "createdAt": "column:created_at"
        }
      }
    }
  ]
}
```text
<!-- Code example in TEXT -->

### 5.2 Plan Types

#### 5.2.1 Simple View Query

```python
<!-- Code example in Python -->
class ViewQueryPlan:
    """Query single view without complex joins."""
    view: str
    where_column: str | None         # For single-entity queries
    filter_mapping: dict[str, str]   # WHERE input → SQL column
    order_by_columns: list[str]
    limit_column: str | None
    offset_column: str | None
    projection: dict[str, str]       # field_name → source
```text
<!-- Code example in TEXT -->

**SQL generated:**

```sql
<!-- Code example in SQL -->
SELECT id, email, name, data, created_at
FROM v_user
WHERE (WHERE conditions applied by filter_mapping)
ORDER BY created_at
LIMIT $1 OFFSET $2
```text
<!-- Code example in TEXT -->

#### 5.2.2 Stored Procedure Call

```python
<!-- Code example in Python -->
class ProcedureCallPlan:
    """Call stored procedure for mutation."""
    procedure: str
    input_mapping: dict[str, str]   # GraphQL arg → param
    output_mapping: dict[str, str]  # response field → GraphQL field
    return_type: str                 # "json" or "jsonb"
```text
<!-- Code example in TEXT -->

**SQL generated:**

```sql
<!-- Code example in SQL -->
SELECT fn_create_user(
    email_param := $1,
    name_param := $2
)
```text
<!-- Code example in TEXT -->

#### 5.2.3 Federation Query

```python
<!-- Code example in Python -->
class FederationQueryPlan:
    """Query across federated subgraphs."""
    subgraph_name: str
    query_name: str
    # ... cross-subgraph fields
```text
<!-- Code example in TEXT -->

### 5.3 WHERE Clause Compilation

WHERE input is compiled to SQL based on introspected columns:

```graphql
<!-- Code example in GraphQL -->
# GraphQL WHERE input
query {
  users(where: {
    email: { _like: "%@example.com" }
    createdAt: { _gte: "2026-01-01T00:00:00Z" }
    _and: [
      { name: { _eq: "Alice" } }
    ]
  }) {
    id
    email
  }
}
```text
<!-- Code example in TEXT -->

**Compiles to:**

```sql
<!-- Code example in SQL -->
SELECT id, email
FROM v_user
WHERE email LIKE '%@example.com'
  AND created_at >= '2026-01-01T00:00:00Z'
  AND name = 'Alice'
```text
<!-- Code example in TEXT -->

### 5.4 Plan Resolution

```python
<!-- Code example in Python -->
def resolve_execution_plan(
    query_name: str,
    schema: CompiledSchema,
    query_ast: QueryAST
) -> ExecutionPlan:
    """Look up pre-compiled plan."""

    # Get plan from schema
    query_def = schema.queries[query_name]
    plan = query_def.execution_plan

    # Apply runtime parameters from query_ast
    # (WHERE values, LIMIT/OFFSET, etc.)
    plan.where_filter = query_ast.arguments.get("where")
    plan.limit = query_ast.arguments.get("limit")
    plan.offset = query_ast.arguments.get("offset")

    return plan
```text
<!-- Code example in TEXT -->

---

## 6. Phase 4: Database Execution

### 6.1 SQL Translation

The execution plan is translated to SQL:

```python
<!-- Code example in Python -->
def translate_to_sql(plan: ExecutionPlan) -> str:
    """Translate execution plan to SQL."""

    if isinstance(plan, ViewQueryPlan):
        return translate_view_query(plan)
    elif isinstance(plan, ProcedureCallPlan):
        return translate_procedure_call(plan)
    else:
        raise ValueError(f"Unknown plan type: {type(plan)}")
```text
<!-- Code example in TEXT -->

### 6.2 Database-Specific Translation

SQL translation is **database-agnostic** at the plan level, but **database-specific** at SQL generation:

```python
<!-- Code example in Python -->
# For PostgreSQL
def translate_view_query_postgresql(plan: ViewQueryPlan) -> str:
    sql = f"SELECT {', '.join(plan.select_columns)} FROM {plan.view}"

    # WHERE conditions
    where_sql = translate_where_filter(plan.where_filter)
    sql += f" WHERE {where_sql}"

    # ORDER BY
    if plan.order_by_columns:
        sql += f" ORDER BY {', '.join(plan.order_by_columns)}"

    # LIMIT/OFFSET
    if plan.limit:
        sql += f" LIMIT ${plan.limit}"
    if plan.offset:
        sql += f" OFFSET ${plan.offset}"

    return sql

# For PostgreSQL - Aggregation queries
def translate_aggregation_postgresql(plan: AggregationExecutionPlan) -> str:
    # SELECT with aggregates
    select_parts = []
    for dim in plan.dimensions:
        if dim.temporal:
            select_parts.append(f"DATE_TRUNC('{dim.temporal.bucket}', {dim.temporal.column}) AS {dim.alias}")
        else:
            select_parts.append(f"{dim.path} AS {dim.alias}")

    for measure in plan.measures:
        select_parts.append(f"{measure.function}({measure.column}) AS {measure.alias}")

    sql = f"SELECT {', '.join(select_parts)} FROM {plan.table}"

    # WHERE
    if plan.where:
        sql += f" WHERE {translate_where_filter(plan.where)}"

    # GROUP BY
    group_by = [dim.path for dim in plan.dimensions]
    sql += f" GROUP BY {', '.join(group_by)}"

    # HAVING
    if plan.having:
        sql += f" HAVING {translate_having_filter(plan.having)}"

    # ORDER BY, LIMIT
    if plan.order_by:
        sql += f" ORDER BY {', '.join(plan.order_by)}"
    if plan.limit:
        sql += f" LIMIT {plan.limit}"

    return sql

# For SQLite
def translate_view_query_sqlite(plan: ViewQueryPlan) -> str:
    # SQLite has LIMIT/OFFSET but no JSONB aggregation
    # Falls back to application-level projection
    ...

# For SQL Server
def translate_view_query_sqlserver(plan: ViewQueryPlan) -> str:
    # SQL Server uses TOP/OFFSET FETCH instead of LIMIT
    ...
```text
<!-- Code example in TEXT -->

### 6.3 Query Execution

```python
<!-- Code example in Python -->
async def execute_query(
    sql: str,
    parameters: dict,
    db_connection
) -> list[dict]:
    """Execute SQL and return rows."""

    try:
        result = await db_connection.fetch(sql, **parameters)
        return result
    except Exception as e:
        # Translate DB error to GraphQL error
        return {
            "errors": [
                {
                    "message": f"Database error: {str(e)}",
                    "extensions": {"code": "DATABASE_ERROR"}
                }
            ]
        }
```text
<!-- Code example in TEXT -->

### 6.4 Result Streaming (Optional)

For large result sets, stream results:

```python
<!-- Code example in Python -->
async def execute_query_streaming(
    sql: str,
    parameters: dict,
    db_connection,
    batch_size: int = 1000
):
    """Stream results in batches."""

    cursor = await db_connection.cursor(sql, **parameters)
    while True:
        rows = await cursor.fetchmany(batch_size)
        if not rows:
            break
        yield rows
```text
<!-- Code example in TEXT -->

---

## 7. Phase 5: Result Projection

### 7.1 JSONB Extraction

When a view returns JSONB data, extract nested fields:

```python
<!-- Code example in Python -->
def project_result(row: dict, projection_plan: dict) -> dict:
    """Project row to GraphQL type."""

    result = {}

    for field_name, source in projection_plan.items():
        if source.startswith("column:"):
            # SQL column
            col_name = source.replace("column:", "")
            result[field_name] = row[col_name]

        elif source.startswith("jsonb:"):
            # JSONB path
            jsonb_path = source.replace("jsonb:", "")
            result[field_name] = extract_jsonb(row["data"], jsonb_path)

    return result
```text
<!-- Code example in TEXT -->

### 7.2 Nested Type Projection

For nested types (like `User.posts`), extract from JSONB:

```python
<!-- Code example in Python -->
# Row from v_user:
{
    "id": "123",
    "email": "user@example.com",
    "data": {
        "posts": [
            {"id": "p1", "title": "First Post"},
            {"id": "p2", "title": "Second Post"}
        ]
    }
}

# Projected to GraphQL User type:
{
    "id": "123",
    "email": "user@example.com",
    "posts": [
        {"id": "p1", "title": "First Post"},
        {"id": "p2", "title": "Second Post"}
    ]
}
```text
<!-- Code example in TEXT -->

### 7.3 Recursive Projection

For deeply nested types:

```python
<!-- Code example in Python -->
def project_recursive(
    row: dict,
    type_def: TypeDef,
    schema: CompiledSchema
) -> dict:
    """Recursively project nested types."""

    result = {}

    for field_name, field_def in type_def.fields.items():
        # Get value from row
        value = extract_field(row, field_name)

        # If field is object/list, recursively project
        if is_nested_type(field_def):
            nested_type_def = schema.types[field_def.type_name]
            if isinstance(value, list):
                result[field_name] = [
                    project_recursive(item, nested_type_def, schema)
                    for item in value
                ]
            else:
                result[field_name] = project_recursive(value, nested_type_def, schema)
        else:
            result[field_name] = value

    return result
```text
<!-- Code example in TEXT -->

### 7.3 Field-Level Authorization Filtering

During result projection, apply field-level authorization rules to hide sensitive fields from unauthorized users:

```python
<!-- Code example in Python -->
def project_result_with_auth(
    row: dict,
    projection_plan: dict,
    auth_context: AuthContext,
    schema: CompiledSchema
) -> dict:
    """Project row, filtering unauthorized fields."""

    result = {}

    for field_name, source in projection_plan.items():
        # Extract field value
        if source.startswith("column:"):
            col_name = source.replace("column:", "")
            value = row[col_name]
        elif source.startswith("jsonb:"):
            jsonb_path = source.replace("jsonb:", "")
            value = extract_jsonb(row["data"], jsonb_path)

        # Check field-level authorization
        field_auth = schema.get_field_auth(type_name, field_name)

        if field_auth and not authorize_field(field_auth, auth_context):
            # Skip this field — user not authorized
            continue

        result[field_name] = value

    return result
```text
<!-- Code example in TEXT -->

**Authorization Examples:**

```python
<!-- Code example in Python -->
# Hide password_hash from non-admin users
class User:
    id: ID
    email: str
    password_hash: str  # @requires_role("admin")
    admin_notes: str    # @requires_role("admin")

# If user has "user" role (not "admin"):
# password_hash and admin_notes are automatically removed
# Result: {"id": "123", "email": "user@example.com"}
```text
<!-- Code example in TEXT -->

**Performance:**

- Field-level auth uses Rust FFI when available (< 0.1 ms per field)
- Falls back to Python implementation if needed (< 1 ms per field)

See **docs/enterpri../../guides/authorization-quick-start.md** for complete Role-Based Access Control documentation, including:

- Hierarchical role inheritance
- Permission caching strategies
- Multi-tenant RBAC patterns
- Fine-grained field authorization

### 7.4 Pagination

Handle LIMIT/OFFSET at projection level:

```python
<!-- Code example in Python -->
def apply_pagination(
    rows: list[dict],
    limit: int | None,
    offset: int | None
) -> dict:
    """Apply pagination to results."""

    total_count = len(rows)

    # Apply offset
    if offset:
        rows = rows[offset:]

    # Apply limit
    if limit:
        rows = rows[:limit]

    return {
        "items": rows,
        "pageInfo": {
            "totalCount": total_count,
            "hasNextPage": offset + limit < total_count if (offset and limit) else False,
            "offset": offset or 0,
            "limit": limit
        }
    }
```text
<!-- Code example in TEXT -->

---

## 8. Mutation Execution

### 8.1 Mutation Flow

Mutations follow the same pipeline, but call stored procedures:

```text
<!-- Code example in TEXT -->
GraphQL Mutation
    ↓
Validation
    ↓
Authorization Check (requires_role, etc.)
    ↓
Call Stored Procedure
    ↓
Procedure returns JSON response
    ↓
Parse Response (success/error/noop)
    ↓
Extract Entity and Cascade
    ↓
Emit Cache Invalidation Events
    ↓
Return to Client
```text
<!-- Code example in TEXT -->

### 8.2 Procedure Call Execution

```python
<!-- Code example in Python -->
async def execute_mutation(
    mutation_name: str,
    arguments: dict,
    schema: CompiledSchema,
    db_connection,
    auth_context: AuthContext
) -> dict:
    """Execute mutation via stored procedure."""

    # Get mutation definition
    mutation_def = schema.mutations[mutation_name]
    plan = mutation_def.execution_plan

    # Build procedure call
    procedure_name = plan.procedure
    params = {}

    # Map GraphQL arguments to procedure parameters
    for graphql_arg, value in arguments.items():
        param_name = plan.input_mapping[graphql_arg]
        params[param_name] = value

    # Call procedure
    sql = f"SELECT {procedure_name}(" + ", ".join(
        f"{k} := ${i}" for i, k in enumerate(params.keys(), 1)
    ) + ")"

    try:
        result = await db_connection.fetchval(sql, *params.values())
        response = json.loads(result)  # Procedure returns JSON string

        # Parse response
        if response.get("status") == "success":
            # Extract entity for result projection
            entity = response.get("entity")
            cascade = response.get("cascade")

            # Emit cache invalidation
            emit_cache_events(cascade, auth_context)

            return {"data": entity}
        else:
            # Error or noop
            return {
                "errors": [
                    {
                        "message": response.get("message"),
                        "extensions": {"status": response.get("status")}
                    }
                ]
            }

    except Exception as e:
        return {
            "errors": [
                {
                    "message": f"Mutation failed: {str(e)}",
                    "extensions": {"code": "MUTATION_FAILED"}
                }
            ]
        }
```text
<!-- Code example in TEXT -->

---

## 9. Phase 6: Cache Invalidation Emission

### 9.1 Cache Invalidation Events

After successful mutations, emit cache invalidation events:

```python
<!-- Code example in Python -->
def emit_cache_events(
    cascade: dict,
    auth_context: AuthContext
):
    """Emit cache invalidation events."""

    # cascade structure:
    # {
    #   "updated": [...],
    #   "deleted": [...],
    #   "invalidations": [...],
    #   "metadata": {...}
    # }

    for updated in cascade.get("updated", []):
        # Emit: entity_type:id was updated
        event = CacheInvalidationEvent(
            type="updated",
            entity_type=updated["entity_type"],
            entity_id=updated["id"],
            timestamp=updated.get("updated_at"),
            tenant_id=auth_context.tenant_id
        )
        emit_to_cache_layer(event)

    for deleted in cascade.get("deleted", []):
        # Emit: entity_type:id was deleted
        event = CacheInvalidationEvent(
            type="deleted",
            entity_type=deleted["entity_type"],
            entity_id=deleted["id"],
            timestamp=deleted.get("deleted_at"),
            tenant_id=auth_context.tenant_id
        )
        emit_to_cache_layer(event)

    for invalidation in cascade.get("invalidations", []):
        # Emit: query/list cache should be cleared
        event = CacheInvalidationEvent(
            type="invalidation",
            query_name=invalidation["query"],
            reason=invalidation.get("reason"),
            tenant_id=auth_context.tenant_id
        )
        emit_to_cache_layer(event)
```text
<!-- Code example in TEXT -->

### 9.2 Cache Event Format

```json
<!-- Code example in JSON -->
{
  "type": "updated|deleted|invalidation",
  "entity_type": "User",
  "entity_id": "123e4567-e89b-12d3-a456-426614174000",
  "timestamp": "2026-01-11T15:35:00Z",
  "tenant_id": "tenant-456",
  "query_name": "users",
  "reason": "user_created"
}
```text
<!-- Code example in TEXT -->

### 9.3 Integration with CDC Event Streaming

Cache invalidation events are emitted from the `cascade` section of mutation responses. For real-time systems that stream these events via Change Data Capture (CDC), see:

- **docs/specs/cdc-format.md** — Event structure and ordering guarantees
- **docs/guides/observability.md section 9** — CDC event streaming patterns and real-time monitoring

The `tb_entity_change_log` table (documented in **docs/specs/schema-conventions.md section 6**) serves as the central audit log for all mutations, enabling CDC systems to consume change events in monotonic order across all data sources.

---

## 10. Error Handling

### 10.1 Error Propagation

Errors at any phase result in GraphQL error response:

```json
<!-- Code example in JSON -->
{
  "errors": [
    {
      "message": "User not found",
      "locations": [{"line": 1, "column": 1}],
      "path": ["user"],
      "extensions": {
        "code": "NOT_FOUND",
        "phase": "result_projection"
      }
    }
  ],
  "data": null
}
```text
<!-- Code example in TEXT -->

### 10.2 Partial Results

For multi-field queries, return partial results if allowed:

```json
<!-- Code example in JSON -->
{
  "errors": [
    {
      "message": "Failed to fetch posts",
      "path": ["user", "posts"]
    }
  ],
  "data": {
    "user": {
      "id": "123",
      "email": "user@example.com",
      "posts": null
    }
  }
}
```text
<!-- Code example in TEXT -->

---

## 11. Performance Considerations

### 11.1 Query Planning Optimization

Pre-compile plans to avoid runtime decision-making:

```python
<!-- Code example in Python -->
# Compile-time: Generate plan once
plan = compile_query("users", schema)

# Runtime: Reuse plan for every request
for request in requests:
    result = execute_plan(plan, request.variables)
```text
<!-- Code example in TEXT -->

### 11.2 Connection Pooling

Use database connection pool for efficiency:

```python
<!-- Code example in Python -->
pool = await create_pool(
    database_url,
    min_size=10,
    max_size=50
)

# Reuse connections
async with pool.acquire() as conn:
    result = await execute_query(sql, conn)
```text
<!-- Code example in TEXT -->

### 11.3 Result Caching

Cache execution results at multiple layers:

```text
<!-- Code example in TEXT -->
HTTP Layer Cache (if request is identical)
    ↓
Query Result Cache (same query + variables)
    ↓
Database Query Cache (database-specific)
```text
<!-- Code example in TEXT -->

---

## 12. Multi-Database Execution

### 12.1 Dialect Detection

Determine database type at startup:

```python
<!-- Code example in Python -->
async def detect_database_dialect(db_connection) -> str:
    """Detect database product."""

    result = await db_connection.fetchval("SELECT version()")

    if "PostgreSQL" in result:
        return "postgresql"
    elif "SQLite" in result:
        return "sqlite"
    elif "MySQL" in result or "MariaDB" in result:
        return "mysql"
    elif "Microsoft SQL Server" in result:
        return "sqlserver"
    else:
        raise ValueError(f"Unsupported database: {result}")
```text
<!-- Code example in TEXT -->

### 12.2 Dialect-Specific Optimizations

Each database can have different execution strategies:

```python
<!-- Code example in Python -->
# PostgreSQL: Use native JSONB aggregation
def execute_postgresql(plan: ViewQueryPlan, db_connection):
    sql = f"SELECT {plan.view}.* FROM {plan.view}"
    # Uses PostgreSQL-specific JSONB functions
    return await db_connection.fetch(sql)

# SQLite: Use JSON functions (slower)
def execute_sqlite(plan: ViewQueryPlan, db_connection):
    sql = f"SELECT * FROM {plan.view}"
    # SQLite JSON functions are slower; may need post-processing
    rows = await db_connection.fetch(sql)
    return [project_result(row, plan.projection) for row in rows]

# SQL Server: Use JSON functions
def execute_sqlserver(plan: ViewQueryPlan, db_connection):
    sql = f"SELECT * FROM {plan.view}"
    # SQL Server has JSON_VALUE and JSON_QUERY functions
    return await db_connection.fetch(sql)
```text
<!-- Code example in TEXT -->

---

## 13. Subscription Event Streaming

### 13.1 Subscription Architecture

While queries and mutations are **request-response** patterns (client asks, server answers), subscriptions are **event-driven** patterns (server pushes events as they happen).

FraiseQL subscriptions operate outside the 6-phase query execution pipeline. Instead, they use the **event backbone**:

```text
<!-- Code example in TEXT -->
Database Transaction Commits
    ↓
Change Detection (LISTEN/NOTIFY or CDC)
    ↓
Event Buffering (tb_entity_change_log)
    ↓
Subscription Matching (compiled WHERE filters)
    ↓
Transport Adapter Dispatch (graphql-ws, webhook, Kafka, gRPC)
    ↓
Client Delivery
```text
<!-- Code example in TEXT -->

**Key difference from queries:**

- Queries: "Give me data" (pull model, request-driven)
- Subscriptions: "Tell me when data changes" (push model, event-driven)

### 13.2 Compiled Subscription Plans

Like queries and mutations, subscriptions are **compiled** at schema build time:

```python
<!-- Code example in Python -->
# Schema authoring time
@FraiseQL.subscription
class OrderCreated:
    where: WhereOrder = FraiseQL.where(user_id=context.user_id)
    id: ID
    amount: Decimal
    created_at: DateTime

# Compiled to subscription plan
{
    "subscription": "OrderCreated",
    "entity_type": "Order",
    "operation": "CREATE",
    "filter_sql": "WHERE user_id = $1 AND deleted_at IS NULL",
    "filter_params": ["user_id"],
    "projection": ["id", "amount", "created_at"],
    "auth_required": True
}
```text
<!-- Code example in TEXT -->

### 13.3 Event Processing Pipeline

When a database change occurs:

```text
<!-- Code example in TEXT -->

1. Event Capture
   - PostgreSQL: LISTEN/NOTIFY triggers
   - MySQL: CDC via Debezium
   - SQL Server: Native Change Data Capture

2. Event Buffering
   - Insert into tb_entity_change_log
   - Assign monotonic sequence number
   - Include Debezium envelope

3. Subscription Matching
   - Load subscription plans from CompiledSchema
   - For each event, evaluate WHERE predicates
   - Determine which subscriptions match

4. Projection
   - Extract requested fields from CDC envelope
   - Apply field-level authorization
   - Transform to GraphQL response format

5. Transport Dispatch
   - Queue to appropriate transport adapter
   - Handle backpressure (slow subscribers)
   - Implement retry logic (webhooks)
```text
<!-- Code example in TEXT -->

### 13.4 Multi-Transport Event Delivery

Same event stream serves multiple consumers:

```text
<!-- Code example in TEXT -->
Order Created Event
    ├─→ graphql-ws adapter
    │   └─→ Browser client (real-time dashboard) [<10ms]
    │
    ├─→ Webhook adapter
    │   └─→ External analytics system [50-200ms with retries]
    │
    └─→ Kafka adapter
        └─→ Data warehouse [async, offset-tracked]
```text
<!-- Code example in TEXT -->

**Each adapter independently:**

- Manages connection lifecycle
- Handles authentication
- Implements delivery semantics (at-least-once, exactly-once)
- Tracks subscription lag

### 13.5 Related Specifications

For complete subscription architecture and implementation details, see:

- **`docs/architecture/realtime/subscriptions.md`** — Full subscription specification
- **`docs/specs/cdc-format.md`** — Event format and structure
- **`docs/specs/schema-conventions.md` section 6** — Event buffering table

---

*End of Execution Model Architecture*
