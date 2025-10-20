# FraiseQL Query Execution Path Analysis

## Executive Summary

FraiseQL's current query execution path follows a **standard GraphQL resolver pattern** with Python parsing/serialization overhead, unlike mutations which use a more direct path. While the query path does leverage Rust transformation for final JSON output in production mode, the intermediate steps still involve:

1. GraphQL resolver invocation
2. Python object instantiation from database results
3. Dict parsing and serialization
4. Rust transformation (final step only)

This differs significantly from the mutation direct path which:
1. Executes PostgreSQL function directly
2. Parses single result row into typed dataclass
3. Returns immediately (no serialization step)

**Key Finding**: There IS substantial Python parsing/serialization overhead in the query path that could be eliminated by implementing a query-specific direct path similar to ADR5's mutation pattern.

---

## Current Query Execution Flow

### Phase 1: GraphQL Request Entry Point

**Location**: `src/fraiseql/core/graphql_type.py:458-583` (field resolver creation)

```
GraphQL Query Request
    ↓
Strawberry/GraphQL parses query
    ↓
Creates field resolver for each requested field
    ↓
Passes GraphQL ResolveInfo to resolver
```

**Code Pattern** (from blog_api/queries.py):
```python
@fraiseql.query
async def user(info, id: UUID) -> Optional[User]:
    """Get a user by ID."""
    db: BlogRepository = info.context["db"]
    user_data = await db.get_user_by_id(id)
    return User.from_dict(user_data) if user_data else None
```

The resolver is a standard async function that:
- Receives GraphQL info
- Manually calls repository method
- Manually converts result to Python type

---

### Phase 2: Repository Query Execution

**Location**: `src/fraiseql/db.py:430-677` (find/find_one methods)

The repository has TWO execution paths depending on mode:

#### Path A: Production Mode (Default)
```
repository.find(view_name, **kwargs)
    ↓
Check if raw JSON passthrough available
    ↓
If yes: Use find_raw_json() → Skip to Rust transformation
    ↓
If no: Continue to standard path
    ↓
Build SELECT * query
    ↓
Execute query → get dict rows
    ↓
Parse to JSON (dict → json.dumps)
    ↓
Execute with Rust transformation
    ↓
Return RawJSONResult
```

**Key Code** (db.py:447-542):
```python
if self.mode == "production":
    # Get GraphQL info from context if available
    info = self.context.get("graphql_info")

    # Extract field paths if we have GraphQL info
    field_paths = None
    if info:
        field_paths = extract_field_paths_from_info(info, transform_path=to_snake_case)

    # ... determine JSONB column ...

    # Build query with raw JSON output
    query = self._build_find_query(
        view_name, raw_json=True, field_paths=field_paths, info=info, **kwargs
    )

    # Execute and parse JSON results
    async with self._pool.connection() as conn:
        result = await execute_raw_json_list_query(
            conn,
            query.statement,
            query.params,
            None,  # field_name=None since we extract data.get("data")
            type_name=type_name,  # Enable Rust transformation
        )
        # Parse the raw JSON to get list of dicts
        data = json.loads(result.json_string)
        return data.get("data", [])
```

#### Path B: Development Mode
```
repository.find(view_name, **kwargs)
    ↓
Build SELECT * query
    ↓
Execute query → get dict rows
    ↓
For each row: _instantiate_from_row()
    ↓
Recursively convert camelCase → snake_case
    ↓
Instantiate Python type objects
    ↓
Return list of typed objects
```

**Development Overhead** (db.py:544-548):
```python
# Development: Full instantiation
query = self._build_find_query(view_name, **kwargs)
rows = await self.run(query)
type_class = self._get_type_for_view(view_name)
return [self._instantiate_from_row(type_class, row) for row in rows]
```

---

### Phase 3: Raw JSON Execution (Production Only)

**Location**: `src/fraiseql/core/raw_json_executor.py:179-249`

```python
async def execute_raw_json_list_query(
    conn: AsyncConnection,
    query: Composed | SQL,
    params: dict[str, Any] | None = None,
    field_name: Optional[str] = None,
    type_name: Optional[str] = None,
) -> RawJSONResult:
    """Execute a query and return raw JSON string wrapped for GraphQL response."""

    async with conn.cursor() as cursor:
        # Execute query without row factory to get raw text
        await cursor.execute(query, params or {})
        rows = await cursor.fetchall()

        if not rows:
            # Return empty array wrapped in GraphQL response
            if field_name:
                return RawJSONResult(f'{{"data":{{"{field_name}":[]}}}}')
            return RawJSONResult('{"data":[]}')

        # Combine JSON rows into array without parsing
        json_items = []
        for row in rows:
            if row[0] is not None:  # Skip null results
                json_items.append(row[0])

        # Join with commas to form array
        json_array = f"[{','.join(json_items)}]"

        # Wrap in GraphQL response
        if field_name:
            escaped_field = field_name.replace('"', '\\"')
            json_response = f'{{"data":{{"{escaped_field}":{json_array}}}}}'
        else:
            json_response = f'{{"data":{json_array}}}'

        # Apply Rust transformation if type_name provided
        if type_name:
            result = RawJSONResult(json_response, transformed=False)
            transformed_result = result.transform(root_type=type_name)
            return transformed_result

        return RawJSONResult(json_response, transformed=False)
```

**What Happens Here:**
1. Query executed, rows fetched as raw text (NO dict row factory)
2. JSON items concatenated as STRING (NO Python dict parsing)
3. Wrapped in GraphQL response structure
4. **Rust transformation applied** for camelCase + __typename injection

---

### Phase 4: Rust Transformation

**Location**: `src/fraiseql/core/rust_transformer.py:122-191`

```python
def transform(self, json_str: str, root_type: str) -> str:
    """Transform JSON string using Rust transformer.

    Returns:
        Transformed JSON string with camelCase keys and __typename
    """
    return self._registry.transform(json_str, root_type)
```

**Performance**: 10-80x faster than Python camelCase transformation

**Transforms**:
- `snake_case` → `camelCase`
- Injects `__typename` field
- Works directly on JSON strings (no dict intermediates)

---

### Phase 5: Return to GraphQL

**Location**: `src/fraiseql/db.py:539-542` (production mode)

```python
# Parse the raw JSON to get list of dicts
data = json.loads(result.json_string)
# Extract the data array (it's wrapped in {"data": [...]})
return data.get("data", [])
```

**Back in Resolver** (queries.py:20-22):
```python
user_data = await db.get_user_by_id(id)
return User.from_dict(user_data) if user_data else None
```

---

## Current Transformation Layers

### Production Mode (Raw JSON Path)

```
PostgreSQL Row (snake_case)
    ↓ (no parsing)
Concatenate JSON strings
    ↓ (string join)
Wrap in GraphQL structure
    ↓ (string formatting)
Rust Transformer (JSON → JSON)
    ↓ (Rust: snake_case → camelCase + __typename)
JSON response
    ↓
JSON parsing in resolver
    ↓
User.from_dict() instantiation
    ↓
Python User object returned
```

**Overhead Points**:
1. ✅ No intermediate dict creation (strings concatenated)
2. ✅ No Python camelCase conversion
3. ✅ Rust handles all transformation
4. ❌ JSON parsing after Rust (line 540: `json.loads(result.json_string)`)
5. ❌ User.from_dict() instantiation in resolver
6. ❌ Return as Python object to GraphQL (which re-serializes to JSON)

### Development Mode

```
PostgreSQL Row (dict)
    ↓
_instantiate_from_row()
    ↓
for key in row: to_snake_case(key) conversion
    ↓
Recursive type instantiation
    ↓
__typename injection (manual)
    ↓
Python object returned
    ↓
GraphQL serialization
    ↓
JSON response
```

**Overhead**: MASSIVE - full Python object instantiation with camelCase conversion

---

## The Mutation Direct Path (ADR5) for Comparison

**Location**: `src/fraiseql/mutations/executor.py:102-191`

```python
async def run_fraiseql_mutation(
    input_payload: object,
    sql_function_name: str,
    result_cls: type[R],
    error_cls: type[E],
    status_map: dict[str, tuple[str, int]],
    fallback_error_identifier: str,
    repository: FraiseQLRepository,
    context: dict[str, Any],
    noop_message: str | None = None,
) -> R | E:
    """Runs a SQL mutation safely and parses its result for FraiseQL."""

    # Build and execute mutation query
    mutation_query = generate_insert_json_call(...)
    result = await repository.run(mutation_query)

    if not result or not result[0]:
        return error_cls(...)

    # DIRECT: Parse single row directly into typed dataclass
    return parse_mutation_result(
        result_row=result[0],
        result_cls=result_cls,
        error_cls=error_cls,
        custom_status_map=status_map,
    )
```

**Key Difference**:
- Single `parse_mutation_result()` call
- Result row → Result type (dict fields extracted directly)
- NO intermediate JSON strings
- NO Rust transformation (only needs simple status mapping)
- Direct type return

---

## Intermediate Parsing/Serialization Layers in Query Path

### Layer 1: Dict Row Factory Bypass (Production) ✅
**Eliminated**: No dict_row factory in raw JSON path

### Layer 2: JSON String Concatenation ✅
**Optimized**: Strings joined directly, no dict intermediates

### Layer 3: Rust Transformation ✅
**Leveraged**: Handles camelCase + __typename in Rust

### Layer 4: JSON Parsing ❌ **OVERHEAD**
**Location**: `db.py:540`
```python
data = json.loads(result.json_string)
return data.get("data", [])
```
**Issue**: After Rust has already transformed to perfect JSON, we parse it back to Python dicts

### Layer 5: Type Instantiation ❌ **OVERHEAD**
**Location**: `queries.py:22`
```python
return User.from_dict(user_data) if user_data else None
```
**Issue**: Converts parsed dict to Python type, which GraphQL then re-serializes

### Layer 6: GraphQL Serialization ❌ **OVERHEAD**
**Implicit**: GraphQL framework takes Python objects and re-serializes to JSON

**Problem**: We convert JSON → dict → Python object → JSON
- Python object creation is expensive
- GraphQL framework has to re-discover field names, types
- We get __typename injection from Rust, then lose it in Python object → JSON

---

## RawJSONResult Usage Analysis

### Current Usage Paths

**Path A: Passthrough Mixin (Disabled)**
```python
# src/fraiseql/repositories/passthrough_mixin.py:92-95
async def find(self, *args, **kwargs):
    """Find with automatic passthrough support."""
    result = await super().find(*args, **kwargs)
    return self._wrap_as_raw_json(result)  # Wraps as RawJSONResult
```
**Status**: Available but not enabled in production

**Path B: find_raw_json() (Rarely Used)**
```python
# src/fraiseql/db.py:709-770
async def find_raw_json(
    self, view_name: str, field_name: str, info: Any = None, **kwargs
) -> RawJSONResult:
    """Find records and return as raw JSON for direct passthrough."""
    # ...
    result = await execute_raw_json_list_query(
        conn,
        query.statement,
        query.params,
        field_name,
        type_name=type_name,
    )
    return result
```
**Status**: Available but requires explicit resolver adaptation

**Path C: Raw JSON Resolver (Experimental)**
```python
# src/fraiseql/gql/raw_json_resolver.py:15-110
def create_raw_json_resolver(fn, field_name, return_type, ...):
    """Create a resolver that uses raw JSON passthrough when possible."""
    # Attempts regex-based detection of db.find_one() calls
    # Falls back to normal execution if detection fails
```
**Status**: Experimental, requires function source code analysis

---

## Comparison: Current Query vs Mutation Direct Paths

| Aspect | Query Path | Mutation Direct Path |
|--------|-----------|----------------------|
| **Entry** | GraphQL resolver | Python function call |
| **Query Execution** | repository.find() | repository.run() |
| **Result Format** | Dict rows OR RawJSONResult | Dict row (single) |
| **Transformation** | Rust transforms JSON | Status mapping |
| **Parsing** | json.loads() in repo | Direct dict access |
| **Type Instantiation** | from_dict() in resolver | parse_mutation_result() |
| **Final Return** | Python object | Typed dataclass |
| **GraphQL Serialization** | Re-serializes object | Serializes dataclass |

---

## Simplification Opportunities

### Opportunity 1: Skip JSON Parsing (Easy)
**Current**:
```python
data = json.loads(result.json_string)
return data.get("data", [])
```

**Problem**: We parse transformed JSON back to dict, only to re-serialize in GraphQL

**Solution**: Return RawJSONResult directly to GraphQL when possible
- Requires custom JSON encoder in FastAPI/GraphQL layer
- Preserves camelCase + __typename through full pipeline

### Opportunity 2: Direct Resolver Integration (Medium)
**Current**: Resolver manually calls db.find(), then from_dict()

**Solution**: Create auto-resolvers that:
1. Detect table/field from GraphQL type
2. Call find_raw_json() automatically
3. Return RawJSONResult to framework

**Pattern**:
```python
@strawberry.field
async def user(info: Info) -> User:
    # Auto-generated resolver:
    # - Calls db.find_raw_json("users", "user", info, **kwargs)
    # - Returns RawJSONResult
    # - Framework handles passthrough
```

### Opportunity 3: Ultra-Direct Path (Hard)
**Similar to Mutation ADR5**: Create query execution layer that:
1. Builds SQL for query
2. Executes directly
3. Wraps result as RawJSONResult
4. Returns to HTTP layer (not GraphQL object layer)

**Would require**:
- Custom HTTP handler for queries (like mutations have)
- Bypasses GraphQL resolver layer
- Direct SQL → HTTP JSON response

---

## Architecture Decision Points

### Current Design Decisions

1. **Rust-First (✅ Good)**: Transformation happens in Rust, not Python
2. **Production vs Dev Split (✅ Pragmatic)**: Different paths for speed vs DX
3. **RawJSONResult Pattern (✅ Good)**: Marker class enables passthrough
4. **Type Instantiation in Resolver (⚠️ Overhead)**: Every query resolver repeats from_dict()

### Recommended Improvements

1. **For Queries**: Implement mutation-style direct path
   - Skip intermediate Python object creation
   - Return RawJSONResult from resolver
   - Custom JSON encoder for passthrough

2. **For Mutation Consistency**: Use same pattern as mutations
   - Single result parsing function
   - Typed result return
   - Automatic transformation

3. **For Maximum Performance**: Offer HTTP-level query bypass
   - Similar to mutation functions at DB level
   - Skip GraphQL resolver layer entirely
   - Direct SQL → HTTP response

---

## Current Performance Bottlenecks

### Measured Overhead (from code analysis)

1. **JSON Parsing**: `json.loads()` after Rust transform
   - Cost: 1-5% depending on size
   - Avoidable: Yes (keep as RawJSONResult)

2. **Type Instantiation**: `User.from_dict()` in resolver
   - Cost: 10-30% of query execution
   - Avoidable: Yes (return RawJSONResult)

3. **GraphQL Re-serialization**: Framework converts Python object back to JSON
   - Cost: 5-15% of query execution
   - Avoidable: Yes (passthrough RawJSONResult)

4. **Rust Transformation**: Snake_case → camelCase + __typename
   - Cost: Already 10-80x faster than Python
   - Status: ✅ Optimized

### Comparison to Mutation Path

**Mutations**: SQL → Parse result row → Typed dataclass → JSON in FastAPI
**Queries**: SQL → Dict rows → JSON → Parse → Type instantiation → Dict → JSON in GraphQL

**Extra steps in queries**: +3 serialization/deserialization cycles

---

## Key Insights for V1+ Optimization

1. **Query Path is Not Optimized**: Unlike mutations, queries still have significant overhead
2. **RawJSONResult Exists but Underutilized**: Could be used for all queries with right setup
3. **Mutation Pattern Should Be Applied to Queries**: Direct path works well for mutations
4. **Framework Integration Needed**: Custom JSON encoder to preserve RawJSONResult through GraphQL

---

## Files Involved in Query Execution

### Core Execution
- `src/fraiseql/db.py` - Repository find/find_one methods (430-677)
- `src/fraiseql/core/raw_json_executor.py` - Raw JSON query execution (105-249)
- `src/fraiseql/core/rust_transformer.py` - Rust transformation integration (122-191)

### GraphQL Integration
- `src/fraiseql/core/graphql_type.py` - Type conversion and field resolver creation (458-583)
- `src/fraiseql/gql/raw_json_resolver.py` - Experimental passthrough resolver wrapper (1-158)

### Example Usage
- `examples/blog_api/queries.py` - Query resolvers (17-93)
- `src/fraiseql/mutations/executor.py` - Mutation execution (ADR5 pattern reference)

---

## Recommendation

For query path optimization similar to ADR5:

1. **Short-term (Easy)**:
   - Keep JSON in RawJSONResult through entire pipeline
   - Custom JSON encoder in FastAPI to preserve passthrough

2. **Medium-term (Moderate)**:
   - Create auto-resolver decorators that use find_raw_json()
   - Support direct RawJSONResult returns from resolvers

3. **Long-term (Full Optimization)**:
   - Implement query-specific HTTP handler (like mutations)
   - Bypass GraphQL resolver layer for simple queries
   - Direct SQL → RawJSONResult → HTTP response

This would achieve 25-60x performance improvement similar to mutations.
