# FraiseQL v1 - Component PRDs

Detailed Product Requirements Documents for each core component to rebuild.

---

## PRD 1: Core Type System

### **Overview**
Clean, Pythonic API for defining GraphQL types using decorators and dataclasses.

### **Goals**
- Intuitive API (feel like standard Python)
- Type-safe (leverage Python's type hints)
- GraphQL spec compliant
- Zero boilerplate

### **API Design**

```python
from fraiseql import type, input, field
from uuid import UUID
from datetime import datetime

@type
class User:
    """A user in the system"""
    id: UUID
    name: str
    email: str
    created_at: datetime
    is_active: bool = True  # Optional with default

    @field(description="User's full name in uppercase")
    def display_name(self) -> str:
        return self.name.upper()

    @field
    async def posts(self, info) -> list["Post"]:
        db = info.context["db"]
        return await db.find("tv_post", {"user_id": self.id})

@input
class CreateUserInput:
    name: str
    email: str
    is_active: bool = True
```

### **Features**

#### 1. Type Decorator
- Converts Python class → GraphQL Object Type
- Auto-generates GraphQL schema
- Supports:
  - Scalar types (int, str, bool, float)
  - Custom scalars (UUID, DateTime, etc.)
  - Lists (`list[T]`)
  - Optionals (`T | None`)
  - Nested types (`User` has `list[Post]`)

#### 2. Input Decorator
- Converts Python class → GraphQL Input Type
- Used for mutation arguments
- Validation support (future: Pydantic integration)

#### 3. Field Decorator
- Custom resolvers
- Async support
- Description for GraphQL schema
- Arguments support

### **Implementation Details**

#### Files
```
src/fraiseql/types/
├── __init__.py             # Public API
├── fraise_type.py          # @type decorator
├── fraise_input.py         # @input decorator
├── field_resolver.py       # @field decorator
└── scalars/                # Custom scalar types
    ├── uuid.py
    ├── datetime.py
    ├── json.py
    └── ...
```

#### Type Registration
```python
# Registry pattern for auto-discovery
class TypeRegistry:
    _types: dict[str, type] = {}

    @classmethod
    def register(cls, name: str, type_class: type):
        cls._types[name] = type_class

    @classmethod
    def get_all(cls) -> dict[str, type]:
        return cls._types.copy()
```

#### GraphQL Schema Generation
```python
def generate_graphql_type(python_class: type) -> GraphQLObjectType:
    """Convert Python class to GraphQL type"""
    fields = {}
    for name, type_hint in get_type_hints(python_class).items():
        graphql_type = map_python_to_graphql(type_hint)
        fields[to_camel_case(name)] = GraphQLField(graphql_type)

    return GraphQLObjectType(
        name=python_class.__name__,
        fields=fields,
        description=python_class.__doc__
    )
```

### **Testing Strategy**

```python
def test_type_decorator():
    @type
    class TestUser:
        id: UUID
        name: str

    # Verify registration
    assert "TestUser" in TypeRegistry.get_all()

    # Verify GraphQL type generation
    gql_type = generate_graphql_type(TestUser)
    assert gql_type.name == "TestUser"
    assert "id" in gql_type.fields
    assert "name" in gql_type.fields
```

### **Port from v0**
- ✅ `types/fraise_type.py` (simplify)
- ✅ `types/fraise_input.py` (simplify)
- ✅ `types/scalars/` (keep all)
- ❌ Remove N+1 tracking complexity

### **Success Criteria**
- [ ] All Python types map to GraphQL types
- [ ] Supports async field resolvers
- [ ] Clean error messages
- [ ] 100% test coverage

---

## PRD 2: Repository Pattern (Command/Query)

### **Overview**
Explicit command/query separation following CQRS pattern.

### **Goals**
- Clear separation of concerns
- Explicit sync (no magic triggers)
- Type-safe operations
- Transaction support

### **API Design**

```python
from fraiseql.repositories import CommandRepository, QueryRepository, sync_tv_user
from uuid import UUID

# ============================================
# COMMAND REPOSITORY (Writes)
# ============================================

class CommandRepository:
    def __init__(self, db: AsyncConnection):
        self.db = db

    async def execute(self, sql: str, *params) -> Any:
        """Execute SQL, return result"""
        return await self.db.fetchval(sql, *params)

    async def execute_many(self, sql: str, params: list) -> None:
        """Batch execute"""
        await self.db.executemany(sql, params)

    def transaction(self):
        """Transaction context manager"""
        return self.db.transaction()

# Usage in mutation
@mutation
async def create_user(info, name: str, email: str) -> User:
    db = info.context["db"]
    cmd_repo = CommandRepository(db)

    # 1. Write to tb_user
    user_id = await cmd_repo.execute(
        "INSERT INTO tb_user (name, email) VALUES ($1, $2) RETURNING id",
        name, email
    )

    # 2. Explicit sync
    await sync_tv_user(db, user_id)

    # 3. Read from tv_user
    query_repo = QueryRepository(db)
    return await query_repo.find_one("tv_user", id=user_id)

# ============================================
# QUERY REPOSITORY (Reads)
# ============================================

class QueryRepository:
    def __init__(self, db: AsyncConnection):
        self.db = db

    async def find_one(
        self,
        view: str,
        id: UUID,
        jsonb_column: str = "data"
    ) -> dict | None:
        """Get single entity by ID"""
        result = await self.db.fetchrow(
            f"SELECT {jsonb_column} FROM {view} WHERE id = $1",
            id
        )
        return result[jsonb_column] if result else None

    async def find(
        self,
        view: str,
        where: dict | None = None,
        order_by: str | None = None,
        limit: int = 100,
        offset: int = 0,
        jsonb_column: str = "data"
    ) -> list[dict]:
        """Query entities with filters"""
        # Build WHERE clause using WhereBuilder
        # Return list of JSONB dicts

    async def count(
        self,
        view: str,
        where: dict | None = None
    ) -> int:
        """Count entities matching filters"""

    async def paginate(
        self,
        view: str,
        first: int | None = None,
        after: str | None = None,
        where: dict | None = None,
        order_by: str = "id"
    ) -> dict:
        """Cursor-based pagination (Relay spec)"""

# Usage in query
@query
async def user(info, id: UUID) -> User:
    repo = QueryRepository(info.context["db"])
    return await repo.find_one("tv_user", id=id)

@query
async def users(
    info,
    limit: int = 10,
    offset: int = 0,
    where: dict | None = None
) -> list[User]:
    repo = QueryRepository(info.context["db"])
    return await repo.find("tv_user", where=where, limit=limit, offset=offset)
```

### **Sync Functions**

```python
# fraiseql/repositories/sync.py

async def sync_tv_user(db: AsyncConnection, user_id: UUID) -> None:
    """Sync tv_user from tb_user (explicit!)"""
    await db.execute("SELECT fn_sync_tv_user($1)", user_id)

async def sync_tv_post(db: AsyncConnection, post_id: UUID) -> None:
    """Sync tv_post from tb_post"""
    await db.execute("SELECT fn_sync_tv_post($1)", post_id)

async def batch_sync_tv_user(db: AsyncConnection, user_ids: list[UUID]) -> None:
    """Batch sync multiple users"""
    await db.executemany(
        "SELECT fn_sync_tv_user($1)",
        [(uid,) for uid in user_ids]
    )
```

### **Implementation Details**

#### Files
```
src/fraiseql/repositories/
├── __init__.py
├── command.py          # CommandRepository
├── query.py            # QueryRepository
└── sync.py             # Sync helper functions
```

#### Where Clause Builder Integration
```python
from fraiseql.sql.where_builder import build_where_clause

class QueryRepository:
    async def find(self, view: str, where: dict | None = None, ...):
        # Convert dict to SQL WHERE clause
        where_sql, params = build_where_clause(where, jsonb_column="data")

        query = f"SELECT data FROM {view} WHERE {where_sql} ORDER BY ..."
        results = await self.db.fetch(query, *params)
        return [r["data"] for r in results]
```

### **Testing Strategy**

```python
@pytest.mark.asyncio
async def test_command_repository(db):
    repo = CommandRepository(db)

    # Test execute
    user_id = await repo.execute(
        "INSERT INTO tb_user (name, email) VALUES ($1, $2) RETURNING id",
        "Alice", "alice@example.com"
    )
    assert user_id is not None

@pytest.mark.asyncio
async def test_query_repository(db):
    # Setup: Insert + sync
    await setup_test_user(db, user_id="...")

    repo = QueryRepository(db)

    # Test find_one
    user = await repo.find_one("tv_user", id=user_id)
    assert user["name"] == "Alice"

    # Test find with filters
    users = await repo.find("tv_user", where={"isActive": True})
    assert len(users) > 0

@pytest.mark.asyncio
async def test_explicit_sync(db):
    # Create user without sync
    user_id = await db.fetchval("INSERT INTO tb_user (...) RETURNING id")

    # Query side should be empty
    repo = QueryRepository(db)
    user = await repo.find_one("tv_user", id=user_id)
    assert user is None

    # Explicit sync
    await sync_tv_user(db, user_id)

    # Now it exists
    user = await repo.find_one("tv_user", id=user_id)
    assert user is not None
```

### **Port from v0**
- ✅ `cqrs/repository.py` (simplify command/query split)
- ✅ `cqrs/executor.py` (merge into repositories)
- ❌ Remove complex batch operations
- ❌ Remove automatic trigger support

### **Success Criteria**
- [ ] Clear command/query separation
- [ ] Explicit sync functions
- [ ] Transaction support
- [ ] Pagination (cursor-based)
- [ ] Where clause integration
- [ ] 100% test coverage

---

## PRD 3: Decorator System (@query, @mutation)

### **Overview**
Simple, intuitive decorators for registering GraphQL resolvers.

### **Goals**
- Auto-registration (no manual wiring)
- Type-safe (leverage Python type hints)
- Async/await support
- Clean syntax

### **API Design**

```python
from fraiseql import query, mutation, subscription
from uuid import UUID
from typing import AsyncGenerator

# ============================================
# QUERIES
# ============================================

@query
async def user(info, id: UUID) -> User:
    """Get user by ID"""
    db = info.context["db"]
    repo = QueryRepository(db)
    return await repo.find_one("tv_user", id=id)

@query
async def users(
    info,
    limit: int = 10,
    offset: int = 0,
    where: dict | None = None
) -> list[User]:
    """List users with pagination"""
    db = info.context["db"]
    repo = QueryRepository(db)
    return await repo.find("tv_user", where=where, limit=limit, offset=offset)

# ============================================
# MUTATIONS
# ============================================

@mutation
async def create_user(info, name: str, email: str) -> User:
    """Create a new user"""
    db = info.context["db"]

    # Write
    user_id = await db.fetchval(
        "INSERT INTO tb_user (name, email) VALUES ($1, $2) RETURNING id",
        name, email
    )

    # Sync
    await sync_tv_user(db, user_id)

    # Read
    repo = QueryRepository(db)
    return await repo.find_one("tv_user", id=user_id)

@mutation
async def update_user(info, id: UUID, name: str) -> User:
    """Update user name"""
    db = info.context["db"]

    await db.execute(
        "UPDATE tb_user SET name = $1 WHERE id = $2",
        name, id
    )
    await sync_tv_user(db, id)

    repo = QueryRepository(db)
    return await repo.find_one("tv_user", id=id)

# ============================================
# SUBSCRIPTIONS (v1.1)
# ============================================

@subscription
async def user_updated(info, user_id: UUID) -> AsyncGenerator[User, None]:
    """Subscribe to user updates"""
    # Implementation with pg_notify or similar
    pass
```

### **Implementation Details**

#### Registry Pattern
```python
# fraiseql/gql/registry.py

class SchemaRegistry:
    _instance = None
    _queries: list[Callable] = []
    _mutations: list[Callable] = []
    _subscriptions: list[Callable] = []

    @classmethod
    def get_instance(cls):
        if cls._instance is None:
            cls._instance = cls()
        return cls._instance

    def register_query(self, func: Callable):
        self._queries.append(func)

    def register_mutation(self, func: Callable):
        self._mutations.append(func)

    def get_all_queries(self) -> list[Callable]:
        return self._queries.copy()

    def get_all_mutations(self) -> list[Callable]:
        return self._mutations.copy()
```

#### Decorator Implementation
```python
# fraiseql/decorators.py

from functools import wraps

def query(fn: Callable) -> Callable:
    """Register function as GraphQL query"""
    registry = SchemaRegistry.get_instance()
    registry.register_query(fn)

    # Don't wrap - keep original function
    # Schema builder will create GraphQL resolver
    return fn

def mutation(fn: Callable) -> Callable:
    """Register function as GraphQL mutation"""
    registry = SchemaRegistry.get_instance()
    registry.register_mutation(fn)
    return fn
```

#### Schema Generation
```python
# fraiseql/gql/schema_builder.py

def build_query_type(registry: SchemaRegistry) -> GraphQLObjectType:
    """Build Query type from registered queries"""
    fields = {}

    for func in registry.get_all_queries():
        # Extract function signature
        sig = inspect.signature(func)
        return_type = sig.return_annotation

        # Convert to GraphQL field
        fields[func.__name__] = GraphQLField(
            type_=python_to_graphql_type(return_type),
            args=extract_args(sig),
            resolve=create_resolver(func),
            description=func.__doc__
        )

    return GraphQLObjectType(name="Query", fields=fields)
```

### **Testing Strategy**

```python
def test_query_decorator():
    @query
    async def test_query(info) -> str:
        return "test"

    registry = SchemaRegistry.get_instance()
    assert test_query in registry.get_all_queries()

def test_schema_generation():
    # Register some queries
    @query
    async def user(info, id: UUID) -> User:
        pass

    @query
    async def users(info) -> list[User]:
        pass

    # Build schema
    schema = build_schema()

    # Verify Query type
    query_type = schema.query_type
    assert "user" in query_type.fields
    assert "users" in query_type.fields
```

### **Port from v0**
- ✅ `decorators.py` (simplify)
- ✅ `gql/schema_builder.py`
- ❌ Remove @field complexity (N+1 tracking, etc.)
- ❌ Remove @turbo_query (save for v1.1)
- ❌ Remove @connection (build separate decorator)

### **Success Criteria**
- [ ] Auto-registration works
- [ ] Type hints → GraphQL types
- [ ] Async/await support
- [ ] Clean error messages
- [ ] Docstrings → GraphQL descriptions

---

## PRD 4: SQL Where Clause Builder

### **Overview**
Type-safe, composable WHERE clause generation for JSONB queries.

### **Goals**
- Support common operators (eq, ne, gt, lt, contains, in)
- JSONB-aware (query inside tv_* data column)
- SQL injection safe
- Composable (AND/OR/NOT in v1.1)

### **API Design**

```python
from fraiseql.sql.where_builder import build_where_clause

# Simple equality
where = {"status": "active"}
# → data->>'status' = 'active'

# Operators
where = {
    "age": {"gt": 18},
    "name": {"contains": "john"}
}
# → data->>'age' > '18' AND data->>'name' LIKE '%john%'

# IN operator
where = {
    "status": {"in": ["active", "pending"]}
}
# → data->>'status' = ANY(['active', 'pending'])

# Null checks
where = {
    "deleted_at": {"is_null": True}
}
# → data->>'deleted_at' IS NULL

# Combine
where = {
    "status": "active",
    "age": {"gte": 18},
    "name": {"contains": "john"}
}
# → data->>'status' = 'active'
#   AND data->>'age' >= '18'
#   AND data->>'name' LIKE '%john%'
```

### **Supported Operators**

| Operator | GraphQL | SQL | Example |
|----------|---------|-----|---------|
| `eq` | `{field: {eq: val}}` | `field = val` | `{"status": {"eq": "active"}}` |
| `ne` | `{field: {ne: val}}` | `field != val` | `{"status": {"ne": "deleted"}}` |
| `gt` | `{field: {gt: val}}` | `field > val` | `{"age": {"gt": 18}}` |
| `gte` | `{field: {gte: val}}` | `field >= val` | `{"age": {"gte": 18}}` |
| `lt` | `{field: {lt: val}}` | `field < val` | `{"age": {"lt": 65}}` |
| `lte` | `{field: {lte: val}}` | `field <= val` | `{"age": {"lte": 65}}` |
| `contains` | `{field: {contains: "x"}}` | `field LIKE '%x%'` | `{"name": {"contains": "john"}}` |
| `starts_with` | `{field: {startsWith: "x"}}` | `field LIKE 'x%'` | `{"email": {"starts_with": "admin"}}` |
| `ends_with` | `{field: {endsWith: "x"}}` | `field LIKE '%x'` | `{"email": {"ends_with": ".com"}}` |
| `in` | `{field: {in: [...]}}` | `field = ANY([...])` | `{"status": {"in": ["active", "pending"]}}` |
| `not_in` | `{field: {notIn: [...]}}` | `field != ALL([...])` | `{"role": {"not_in": ["admin"]}}` |
| `is_null` | `{field: {isNull: true}}` | `field IS NULL` | `{"deleted_at": {"is_null": True}}` |

### **Implementation**

```python
# fraiseql/sql/where_builder.py

from psycopg.sql import SQL, Literal, Composed

def build_where_clause(
    where: dict | None,
    jsonb_column: str = "data",
    table_alias: str | None = None
) -> tuple[Composed, list]:
    """
    Build WHERE clause from filter dict

    Returns:
        (sql_composed, params) tuple
    """
    if not where:
        return SQL("1=1"), []

    conditions = []
    params = []

    for field, value in where.items():
        if isinstance(value, dict):
            # Operator syntax: {"age": {"gt": 18}}
            condition = build_operator_condition(
                field, value, jsonb_column, table_alias
            )
        else:
            # Simple equality: {"status": "active"}
            condition = build_equality_condition(
                field, value, jsonb_column, table_alias
            )

        if condition:
            conditions.append(condition)

    if not conditions:
        return SQL("1=1"), []

    # Join with AND
    return SQL(" AND ").join(conditions), params


def build_operator_condition(
    field: str,
    operators: dict,
    jsonb_column: str,
    table_alias: str | None
) -> Composed:
    """Build condition with operator"""
    conditions = []

    for op, value in operators.items():
        if op == "eq":
            cond = SQL("{}.{}->>%s = %s").format(
                SQL(table_alias) if table_alias else SQL(""),
                SQL(jsonb_column),
                Literal(field),
                Literal(value)
            )
        elif op == "gt":
            cond = SQL("{}.{}->>%s > %s").format(...)
        # ... etc for all operators

        conditions.append(cond)

    return SQL(" AND ").join(conditions)


# Usage in QueryRepository
class QueryRepository:
    async def find(self, view: str, where: dict | None = None, ...):
        where_sql, params = build_where_clause(where)

        query = SQL("SELECT data FROM {} WHERE {}").format(
            SQL(view),
            where_sql
        )

        results = await self.db.fetch(query, *params)
        return [r["data"] for r in results]
```

### **Testing Strategy**

```python
def test_simple_equality():
    where = {"status": "active"}
    sql, params = build_where_clause(where)

    assert "data->>'status'" in str(sql)
    assert "'active'" in str(sql)

def test_operator_gt():
    where = {"age": {"gt": 18}}
    sql, params = build_where_clause(where)

    assert "data->>'age' >" in str(sql)
    assert "18" in str(sql)

def test_operator_contains():
    where = {"name": {"contains": "john"}}
    sql, params = build_where_clause(where)

    assert "LIKE" in str(sql)
    assert "%john%" in str(sql)

def test_sql_injection_safe():
    # Malicious input
    where = {"name": "'; DROP TABLE users; --"}
    sql, params = build_where_clause(where)

    # Should be parameterized
    assert "DROP TABLE" not in str(sql)
    # Actual value should be in params
    assert "'; DROP TABLE users; --" in params
```

### **Port from v0**
- ✅ `sql/where/` entire directory (it's already clean!)
- ✅ `sql/operators.py`
- ✅ Enhance for JSONB support

### **Success Criteria**
- [ ] All operators work
- [ ] JSONB column support
- [ ] SQL injection safe (parameterized queries)
- [ ] Composable (can combine conditions)
- [ ] 100% test coverage

---

## PRD 5: Rust Integration

### **Overview**
Python ↔ Rust bridge for high-performance JSON transformation.

### **Goals**
- 40x speedup over pure Python
- Transparent (user doesn't see it)
- Field selection
- Type coercion (UUID, datetime, etc.)

### **API Design**

```python
# User never calls this directly - QueryRepository uses it internally
from fraiseql.core.rust_transformer import transform_json

# Input: DB result (snake_case JSONB)
db_result = {
    "id": "123e4567-e89b-12d3-a456-426614174000",
    "first_name": "Alice",
    "last_name": "Smith",
    "created_at": "2024-01-01T00:00:00Z",
    "user_posts": [...]
}

# Transform to GraphQL (camelCase)
graphql_result = transform_json(
    json_str=json.dumps(db_result),
    schema=schema_info,
    selection=["id", "firstName", "lastName"]  # Only requested fields
)

# Output
{
    "id": "123e4567-e89b-12d3-a456-426614174000",
    "firstName": "Alice",
    "lastName": "Smith"
}
# Note: created_at and user_posts excluded (not in selection)
```

### **Rust Implementation**

```rust
// fraiseql_rs/src/lib.rs

use pyo3::prelude::*;
use serde_json::{Value, Map};

#[pyfunction]
fn transform_json(
    json_str: &str,
    selection: Vec<&str>
) -> PyResult<String> {
    let mut value: Value = serde_json::from_str(json_str)?;

    // 1. Filter fields by selection
    if let Value::Object(ref mut map) = value {
        filter_fields(map, &selection);
    }

    // 2. Convert snake_case → camelCase
    let transformed = convert_to_camel_case(value);

    // 3. Serialize back to JSON
    Ok(serde_json::to_string(&transformed)?)
}

fn filter_fields(map: &mut Map<String, Value>, selection: &[&str]) {
    map.retain(|key, _| {
        // Keep if in selection or if selection is empty (select all)
        selection.is_empty() || selection.contains(&key.as_str())
    });
}

fn convert_to_camel_case(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let new_map: Map<String, Value> = map
                .into_iter()
                .map(|(k, v)| {
                    let camel_key = to_camel_case(&k);
                    let camel_value = convert_to_camel_case(v);
                    (camel_key, camel_value)
                })
                .collect();
            Value::Object(new_map)
        }
        Value::Array(arr) => {
            Value::Array(
                arr.into_iter()
                    .map(convert_to_camel_case)
                    .collect()
            )
        }
        _ => value,
    }
}

fn to_camel_case(snake_str: &str) -> String {
    let mut camel = String::new();
    let mut capitalize_next = false;

    for ch in snake_str.chars() {
        if ch == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            camel.push(ch.to_uppercase().next().unwrap());
            capitalize_next = false;
        } else {
            camel.push(ch);
        }
    }

    camel
}

#[pymodule]
fn fraiseql_rs(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(transform_json, m)?)?;
    Ok(())
}
```

### **Python Bridge**

```python
# fraiseql/core/rust_transformer.py

try:
    import fraiseql_rs
    RUST_AVAILABLE = True
except ImportError:
    RUST_AVAILABLE = False

class RustTransformer:
    """Transparent Rust transformation"""

    def __init__(self):
        self.enabled = RUST_AVAILABLE

    def transform(
        self,
        data: dict,
        selection: list[str] | None = None
    ) -> dict:
        """Transform DB result to GraphQL format"""
        if not self.enabled:
            # Fallback to Python (slow but works)
            return self._python_transform(data, selection)

        # Use Rust (fast)
        json_str = json.dumps(data)
        result_str = fraiseql_rs.transform_json(
            json_str,
            selection or []
        )
        return json.loads(result_str)

    def _python_transform(self, data: dict, selection: list[str] | None) -> dict:
        """Fallback Python implementation"""
        # Filter fields
        if selection:
            data = {k: v for k, v in data.items() if k in selection}

        # Convert to camelCase
        def to_camel(snake: str) -> str:
            parts = snake.split('_')
            return parts[0] + ''.join(p.capitalize() for p in parts[1:])

        def convert_keys(obj):
            if isinstance(obj, dict):
                return {to_camel(k): convert_keys(v) for k, v in obj.items()}
            elif isinstance(obj, list):
                return [convert_keys(item) for item in obj]
            return obj

        return convert_keys(data)

# Singleton instance
_transformer = RustTransformer()

def transform_json(data: dict, selection: list[str] | None = None) -> dict:
    """Public API - uses Rust if available"""
    return _transformer.transform(data, selection)
```

### **Integration with QueryRepository**

```python
class QueryRepository:
    def __init__(self, db: AsyncConnection, use_rust: bool = True):
        self.db = db
        self.use_rust = use_rust

    async def find_one(self, view: str, id: UUID) -> dict | None:
        result = await self.db.fetchrow(
            "SELECT data FROM {} WHERE id = $1", view, id
        )

        if not result:
            return None

        data = result["data"]

        # Apply Rust transformation if enabled
        if self.use_rust:
            data = transform_json(data)

        return data
```

### **Benchmarking**

```python
# benchmarks/test_rust_performance.py

import pytest
import time
from fraiseql.core.rust_transformer import transform_json

def test_rust_vs_python_performance():
    # Large nested object
    data = {
        "id": "123",
        "first_name": "Alice",
        "user_posts": [
            {"id": str(i), "post_title": f"Post {i}"}
            for i in range(100)
        ]
    }

    # Python transformation
    start = time.perf_counter()
    for _ in range(1000):
        _ = transform_json(data, use_rust=False)
    python_time = time.perf_counter() - start

    # Rust transformation
    start = time.perf_counter()
    for _ in range(1000):
        _ = transform_json(data, use_rust=True)
    rust_time = time.perf_counter() - start

    speedup = python_time / rust_time
    print(f"Speedup: {speedup:.1f}x")
    assert speedup > 30  # At least 30x faster
```

### **Port from v0**
- ✅ `core/rust_transformer.py`
- ✅ `fraiseql_rs/` Rust crate
- ✅ Enhance with field selection

### **Success Criteria**
- [ ] 30-40x speedup over Python
- [ ] Field selection works
- [ ] snake_case → camelCase
- [ ] Handles nested objects/arrays
- [ ] Graceful fallback if Rust unavailable

---

## Summary Table

| Component | LOC Estimate | Port from v0 | Complexity | Priority |
|-----------|--------------|--------------|------------|----------|
| Type System | 800 | ✅ Yes | Low | 1 (CRITICAL) |
| Repositories | 600 | ⚠️ Partial | Medium | 1 (CRITICAL) |
| Decorators | 400 | ✅ Yes | Low | 1 (CRITICAL) |
| Where Builder | 500 | ✅ Yes | Low | 2 (HIGH) |
| Rust Integration | 300 (Py) + 200 (Rust) | ✅ Yes | Medium | 2 (HIGH) |
| **TOTAL** | **~2,800 lines** | | | |

**Timeline**: 4-6 weeks to implement all components

---

## Next Steps

1. Set up new `fraiseql-v1/` project structure
2. Implement in priority order:
   - Type System (week 1)
   - Decorators (week 1)
   - Repositories (week 2-3)
   - Where Builder (week 3)
   - Rust Integration (week 4)
3. Write tests as you go (TDD)
4. Create examples once core is working
5. Write documentation

**Ready to start coding?** Begin with the Type System PRD!
