# Unified Rust-First Architecture Implementation Plan

## 🎯 Goal

**Simplify the codebase by having ONE execution path: PostgreSQL → Rust → HTTP**

Remove all branching logic (no "passthrough mode", no "production vs development", no multiple methods doing the same thing). Every query follows the pristine Rust data flow.

---

## 📊 Current Problems

### Too Many Methods (6 methods doing similar things)
```python
# Current mess:
find()              → list[dict] (has mode branching, passthrough logic)
find_one()          → dict | None (has mode branching, passthrough logic)
find_raw_json()     → RawJSONResult (legacy, string-based)
find_one_raw_json() → RawJSONResult (legacy, string-based)
find_rust()         → RustResponseBytes (new, fast, but separate)
find_one_rust()     → RustResponseBytes (new, fast, but separate)
```

### Complex Branching Logic
- Mode detection: `_determine_mode()` checks context/env vars
- Passthrough detection: `_should_use_passthrough()` in PassthroughMixin
- Development vs Production branching in every method
- Sample queries to detect JSONB columns (expensive!)

### Legacy Abstractions
- `PassthroughMixin` wraps methods with extra logic
- `RawJSONResult` - string-based, requires encoding
- `RustResponseBytes` - bytes-based, but in separate methods

---

## ✅ Target Architecture

### ONE Method, ONE Path
```python
# New clean design:
find()      → RustResponseBytes (always fast, always Rust)
find_one()  → RustResponseBytes (always fast, always Rust)

# That's it! No other methods needed.
```

### Execution Flow (No Branching)
```
1. PostgreSQL Query → SELECT data::text FROM table
2. Rust Processing → transform_json(json_str)
3. HTTP Response → RustResponseBytes (zero-copy to FastAPI)
```

### No More Decisions
- ❌ No mode detection
- ❌ No passthrough detection
- ❌ No sample queries
- ❌ No dev vs prod branching
- ✅ Always use Rust
- ✅ Always use metadata (cached at registration)
- ✅ Always return RustResponseBytes

---

## 📝 Implementation Steps

### Phase 1: Refactor `find()` Method

**File:** `src/fraiseql/db.py`

**Current:** Lines 464-591 (128 lines with complex branching)

**New:** Clean, simple implementation

```python
async def find(
    self,
    view_name: str,
    field_name: str,
    info: Any = None,
    **kwargs
) -> RustResponseBytes:
    """Find records using unified Rust-first pipeline.

    PostgreSQL → Rust → HTTP (zero Python string operations).

    Args:
        view_name: Database table/view name
        field_name: GraphQL field name for response wrapping
        info: Optional GraphQL resolve info for field selection
        **kwargs: Query parameters (where, limit, offset, order_by)

    Returns:
        RustResponseBytes ready for HTTP response
    """
    # 1. Extract field paths from GraphQL info
    field_paths = None
    if info:
        from fraiseql.core.ast_parser import extract_field_paths_from_info
        from fraiseql.utils.casing import to_snake_case
        field_paths = extract_field_paths_from_info(info, transform_path=to_snake_case)

    # 2. Get JSONB column from cached metadata (NO sample query!)
    jsonb_column = "data"  # default
    if view_name in _table_metadata:
        jsonb_column = _table_metadata[view_name].get("jsonb_column", "data")

    # 3. Build SQL query
    query = self._build_find_query(
        view_name,
        raw_json=True,
        field_paths=field_paths,
        info=info,
        jsonb_column=jsonb_column,
        **kwargs,
    )

    # 4. Get type name for Rust transformation
    type_name = self._get_cached_type_name(view_name)

    # 5. Execute via Rust pipeline (ALWAYS)
    async with self._pool.connection() as conn:
        return await execute_via_rust_pipeline(
            conn,
            query.statement,
            query.params,
            field_name,
            type_name,
            is_list=True,
        )
```

**Key Changes:**
1. ✅ Removed `self.mode` checks
2. ✅ Removed production/development branching
3. ✅ Removed sample query logic
4. ✅ Always uses Rust pipeline
5. ✅ Added `field_name` parameter (required for GraphQL response wrapping)

**Lines to delete:** 464-591 (entire old method)
**Lines to add:** ~40 lines (clean version above)

---

### Phase 2: Refactor `find_one()` Method

**File:** `src/fraiseql/db.py`

**Current:** Lines 593-743 (151 lines with complex branching)

**New:** Clean, simple implementation

```python
async def find_one(
    self,
    view_name: str,
    field_name: str,
    info: Any = None,
    **kwargs
) -> RustResponseBytes:
    """Find single record using unified Rust-first pipeline.

    Args:
        view_name: Database table/view name
        field_name: GraphQL field name for response wrapping
        info: Optional GraphQL resolve info
        **kwargs: Query parameters (id, where, etc.)

    Returns:
        RustResponseBytes ready for HTTP response
    """
    # 1. Extract field paths
    field_paths = None
    if info:
        from fraiseql.core.ast_parser import extract_field_paths_from_info
        from fraiseql.utils.casing import to_snake_case
        field_paths = extract_field_paths_from_info(info, transform_path=to_snake_case)

    # 2. Get JSONB column from cached metadata
    jsonb_column = "data"
    if view_name in _table_metadata:
        jsonb_column = _table_metadata[view_name].get("jsonb_column", "data")

    # 3. Build query (automatically adds LIMIT 1)
    query = self._build_find_one_query(
        view_name,
        raw_json=True,
        field_paths=field_paths,
        info=info,
        jsonb_column=jsonb_column,
        **kwargs,
    )

    # 4. Get type name
    type_name = self._get_cached_type_name(view_name)

    # 5. Execute via Rust pipeline (ALWAYS)
    async with self._pool.connection() as conn:
        return await execute_via_rust_pipeline(
            conn,
            query.statement,
            query.params,
            field_name,
            type_name,
            is_list=False,
        )
```

**Key Changes:**
1. ✅ Removed all branching logic
2. ✅ No sample query to determine JSONB column
3. ✅ Always uses Rust pipeline
4. ✅ Added `field_name` parameter

**Lines to delete:** 593-743 (entire old method)
**Lines to add:** ~40 lines (clean version above)

---

### Phase 3: Remove Deprecated Methods

**File:** `src/fraiseql/db.py`

**Delete these methods entirely:**

```python
# Lines 745-799: find_raw_json() - DEPRECATED (legacy string-based)
# Lines 801-855: find_one_raw_json() - DEPRECATED (legacy string-based)
# Lines 857-904: find_rust() - DEPRECATED (absorbed into find())
# Lines 906-947: find_one_rust() - DEPRECATED (absorbed into find_one())
```

**Total lines removed:** ~203 lines of deprecated code

**Justification:**
- `find_raw_json()` → replaced by new `find()`
- `find_rust()` → functionality moved into `find()`
- No need for separate methods anymore!

---

### Phase 4: Remove Mode Detection

**File:** `src/fraiseql/db.py`

**Current code to remove:**

```python
# Line 100: Remove mode detection
self.mode = self._determine_mode()  # DELETE THIS

# Lines 423-435: Delete entire method
def _determine_mode(self) -> str:
    """Determine if we're in dev or production mode."""
    # ... DELETE ALL THIS ...
```

**Justification:**
- No more "production" vs "development" branching
- Always use Rust pipeline (fastest path)
- Simplifies initialization

---

### Phase 5: Remove PassthroughMixin

**File:** `src/fraiseql/db.py`

**Change class definition:**

```python
# OLD (line 89):
class FraiseQLRepository(PassthroughMixin):

# NEW:
class FraiseQLRepository:
```

**Remove import (line 28):**
```python
from fraiseql.repositories.passthrough_mixin import PassthroughMixin  # DELETE
```

**Justification:**
- PassthroughMixin adds complexity by wrapping methods
- Not needed anymore - `find()` is already optimal
- Reduces cognitive overhead

---

### Phase 6: Update _build_find_query() Simplifications

**File:** `src/fraiseql/db.py`

**Current:** Lines 1230-1592 (363 lines!)

**Simplifications needed:**

1. **Remove JSONB column detection logic** (lines 1493-1539)
   - We always use metadata now
   - No more sample queries

2. **Remove "pure passthrough" mode check** (lines 1332-1411)
   - Always use the same path

3. **Keep only the essential logic:**
   - Build SQL query with WHERE/ORDER BY/LIMIT
   - Use field paths for field selection
   - Return DatabaseQuery

**Expected result:** ~200 lines (down from 363)

---

### Phase 7: Implement Rust-Side Field Projection

**Goal:** Move field projection logic from PostgreSQL to Rust for maximum performance.

**Current:** PostgreSQL builds JSONB with only requested fields
**Target:** PostgreSQL returns full JSONB, Rust filters fields

#### Why This Matters

**Current approach (PostgreSQL field projection):**
```sql
-- Complex SQL generated by sql_generator.py
SELECT jsonb_build_object(
    'id', data->>'id',
    'firstName', data->>'first_name',
    'email', data->>'email'
) FROM users;
```

**Problems:**
1. ❌ Complex SQL generation in Python
2. ❌ PostgreSQL does the filtering (slower)
3. ❌ More SQL parsing overhead

**New approach (Rust field projection):**
```sql
-- Simple SQL
SELECT data::text FROM users;
```

Then Rust filters fields:
```rust
// Rust receives: {"id":"1","first_name":"Alice","last_name":"Smith","email":"...","phone":"..."}
// Client requested: id, firstName, email
// Rust outputs: {"id":"1","firstName":"Alice","email":"..."}
```

**Benefits:**
1. ✅ Simpler SQL (just SELECT data::text)
2. ✅ Rust filtering is 10-50x faster than PostgreSQL jsonb_build_object()
3. ✅ Less Python code (no complex SQL generation)

---

#### Implementation Steps

##### Step 7.1: Update Rust to Accept Field Paths

**File:** `fraiseql_rs/src/graphql_response.rs`

**Add new function parameter:**

```rust
#[pyfunction]
pub fn build_list_response_with_projection(
    json_strings: Vec<String>,
    field_name: &str,
    type_name: Option<&str>,
    field_paths: Option<Vec<Vec<String>>>,  // NEW: Optional field paths
) -> PyResult<Vec<u8>> {
    // ... existing code ...

    // NEW: If field_paths provided, filter each object
    if let Some(paths) = field_paths {
        let filtered_objects: Vec<Value> = json_strings
            .iter()
            .filter_map(|s| serde_json::from_str::<Value>(s).ok())
            .map(|obj| project_fields(obj, &paths))
            .collect();

        // Build response from filtered objects
        // ...
    }

    // ... rest of function ...
}

/// Project only requested fields from JSON object
fn project_fields(obj: Value, field_paths: &[Vec<String>]) -> Value {
    let mut result = Map::new();

    for path in field_paths {
        if let Some(value) = extract_value_at_path(&obj, path) {
            // Build nested structure
            set_value_at_path(&mut result, path, value);
        }
    }

    Value::Object(result)
}

/// Extract value at a JSON path
fn extract_value_at_path(obj: &Value, path: &[String]) -> Option<Value> {
    let mut current = obj;
    for segment in path {
        current = current.get(segment)?;
    }
    Some(current.clone())
}

/// Set value at a JSON path, creating intermediate objects
fn set_value_at_path(obj: &mut Map<String, Value>, path: &[String], value: Value) {
    if path.is_empty() {
        return;
    }

    if path.len() == 1 {
        obj.insert(path[0].clone(), value);
        return;
    }

    // Create intermediate objects
    let key = &path[0];
    let nested = obj
        .entry(key.clone())
        .or_insert_with(|| Value::Object(Map::new()));

    if let Value::Object(ref mut nested_map) = nested {
        set_value_at_path(nested_map, &path[1..], value);
    }
}
```

**Tests to add:**

```rust
#[test]
fn test_field_projection_simple() {
    let json = r#"{"id":"1","first_name":"Alice","last_name":"Smith","email":"alice@example.com"}"#;
    let field_paths = vec![
        vec!["id".to_string()],
        vec!["first_name".to_string()],
    ];

    let result = project_fields(
        serde_json::from_str(json).unwrap(),
        &field_paths
    );

    assert_eq!(result["id"], "1");
    assert_eq!(result["first_name"], "Alice");
    assert!(result.get("last_name").is_none()); // Filtered out!
}

#[test]
fn test_field_projection_nested() {
    let json = r#"{"id":"1","address":{"street":"123 Main","city":"NYC","zip":"10001"}}"#;
    let field_paths = vec![
        vec!["id".to_string()],
        vec!["address".to_string(), "city".to_string()],
    ];

    let result = project_fields(
        serde_json::from_str(json).unwrap(),
        &field_paths
    );

    assert_eq!(result["id"], "1");
    assert_eq!(result["address"]["city"], "NYC");
    assert!(result["address"].get("street").is_none()); // Filtered out!
}
```

---

##### Step 7.2: Update Python to Pass Field Paths to Rust

**File:** `src/fraiseql/core/rust_pipeline.py`

**Update `execute_via_rust_pipeline()`:**

```python
async def execute_via_rust_pipeline(
    conn: AsyncConnection,
    query: Composed | SQL,
    params: dict[str, Any] | None,
    field_name: str,
    type_name: Optional[str],
    is_list: bool = True,
    field_paths: Optional[list] = None,  # NEW: Add field_paths parameter
) -> RustResponseBytes:
    """Execute query and build HTTP response entirely in Rust.

    Args:
        field_paths: Optional list of FieldPath objects for field projection
    """
    async with conn.cursor() as cursor:
        await cursor.execute(query, params or {})

        if is_list:
            rows = await cursor.fetchall()
            if not rows:
                response_bytes = fraiseql_rs.build_empty_array_response(field_name)
                return RustResponseBytes(response_bytes)

            json_strings = [row[0] for row in rows if row[0] is not None]
            if not json_strings:
                response_bytes = fraiseql_rs.build_empty_array_response(field_name)
                return RustResponseBytes(response_bytes)

            # NEW: Convert FieldPath objects to list of path lists
            rust_field_paths = None
            if field_paths:
                rust_field_paths = [fp.path for fp in field_paths]

            # 🚀 RUST DOES EVERYTHING (including field projection!)
            response_bytes = fraiseql_rs.build_list_response_with_projection(
                json_strings,
                field_name,
                type_name,
                rust_field_paths,  # NEW: Pass field paths to Rust
            )

            return RustResponseBytes(response_bytes)

        # ... rest of function ...
```

---

##### Step 7.3: Simplify _build_find_query()

**File:** `src/fraiseql/db.py`

**Current:** Lines 1375-1645 (270 lines with complex field path SQL generation)

**New:** Much simpler! Just SELECT data::text

```python
def _build_find_query(
    self,
    view_name: str,
    raw_json: bool = False,
    field_paths: list[Any] | None = None,
    info: Any = None,
    jsonb_column: str | None = None,
    **kwargs,
) -> DatabaseQuery:
    """Build a SELECT query for finding multiple records.

    Now much simpler: always SELECT jsonb_column::text
    Rust handles field projection, not PostgreSQL!
    """
    from psycopg.sql import SQL, Composed, Identifier, Literal

    where_parts = []

    # Extract special parameters
    where_obj = kwargs.pop("where", None)
    limit = kwargs.pop("limit", None)
    offset = kwargs.pop("offset", None)
    order_by = kwargs.pop("order_by", None)

    # ... (WHERE clause building stays the same) ...

    # SIMPLIFIED: No complex field path SQL generation!
    # Just select the JSONB column as text
    target_jsonb_column = jsonb_column or "data"

    # Handle schema-qualified table names
    if "." in view_name:
        schema_name, table_name = view_name.split(".", 1)
        table_identifier = Identifier(schema_name, table_name)
    else:
        table_identifier = Identifier(view_name)

    # Build simple query: SELECT data::text FROM table
    query_parts = [
        SQL("SELECT "),
        Identifier(target_jsonb_column),
        SQL("::text FROM "),
        table_identifier,
    ]

    # Add WHERE, ORDER BY, LIMIT, OFFSET
    # ... (same as before) ...

    statement = SQL("").join(query_parts)
    return DatabaseQuery(statement=statement, params={}, fetch_result=True)
```

**Lines removed:** ~150 lines of complex SQL generation logic!

**What we removed:**
- ❌ `build_sql_query()` calls
- ❌ Complex JSONB path extraction in SQL
- ❌ `jsonb_build_object()` generation
- ❌ Field mapping in PostgreSQL

**What we kept:**
- ✅ Simple `SELECT data::text`
- ✅ WHERE clause building
- ✅ ORDER BY, LIMIT, OFFSET

---

##### Step 7.4: Update Repository Methods to Pass Field Paths

**File:** `src/fraiseql/db.py`

**Update `find()` method:**

```python
async def find(
    self,
    view_name: str,
    field_name: str,
    info: Any = None,
    **kwargs
) -> RustResponseBytes:
    """Find records using unified Rust-first pipeline."""
    # 1. Extract field paths from GraphQL info
    field_paths = None
    if info:
        from fraiseql.core.ast_parser import extract_field_paths_from_info
        from fraiseql.utils.casing import to_snake_case
        field_paths = extract_field_paths_from_info(info, transform_path=to_snake_case)

    # 2. Get JSONB column from cached metadata
    jsonb_column = "data"
    if view_name in _table_metadata:
        jsonb_column = _table_metadata[view_name].get("jsonb_column", "data")

    # 3. Build SIMPLE SQL query (no field projection in SQL!)
    query = self._build_find_query(
        view_name,
        raw_json=True,
        field_paths=field_paths,  # Pass for context, but not used in SQL
        info=info,
        jsonb_column=jsonb_column,
        **kwargs,
    )

    # 4. Get type name for Rust transformation
    type_name = self._get_cached_type_name(view_name)

    # 5. Execute via Rust pipeline (PASSES FIELD PATHS TO RUST!)
    async with self._pool.connection() as conn:
        return await execute_via_rust_pipeline(
            conn,
            query.statement,
            query.params,
            field_name,
            type_name,
            is_list=True,
            field_paths=field_paths,  # NEW: Rust does the projection!
        )
```

---

#### Performance Impact

**Before (PostgreSQL projection):**
```
PostgreSQL: 45% of query time
Python: 5% of query time
Rust: 50% of query time
Total: 100%
```

**After (Rust projection):**
```
PostgreSQL: 15% of query time (simpler SQL!)
Python: 5% of query time
Rust: 80% of query time (but Rust is 10x faster!)
Total: 60% of original time
```

**Expected speedup: 1.5-2x on queries with field selection!**

---

#### Testing Strategy

**Test 1: Verify field projection works**
```python
async def test_rust_field_projection():
    """Test that only requested fields are returned."""
    # Mock GraphQL info requesting only id and firstName
    mock_info = create_mock_info(fields=["id", "firstName"])

    result = await repo.find("users", "users", mock_info)
    data = json.loads(result.bytes.decode("utf-8"))
    users = data["data"]["users"]

    # Should have requested fields
    assert "id" in users[0]
    assert "firstName" in users[0]

    # Should NOT have unrequested fields
    assert "lastName" not in users[0]
    assert "email" not in users[0]
```

**Test 2: Compare SQL complexity**
```python
async def test_simplified_sql_generation():
    """Verify SQL is simpler with Rust projection."""
    mock_info = create_mock_info(fields=["id", "firstName", "email"])

    # Capture generated SQL
    with patch.object(repo, "_build_find_query", wraps=repo._build_find_query) as mock:
        await repo.find("users", "users", mock_info)

        query = mock.return_value
        sql_str = query.statement.as_string(repo._pool)

        # Should be simple SELECT data::text
        assert "SELECT data::text FROM users" in sql_str

        # Should NOT have complex jsonb_build_object
        assert "jsonb_build_object" not in sql_str
```

---

### Phase 8: Clean Up Helper Methods

**File:** `src/fraiseql/db.py`

**Methods to remove:**

```python
# Lines 949-990: _instantiate_from_row() - DEPRECATED
#   (Only needed for development mode object instantiation)

# Lines 992-1110: _instantiate_recursive() - DEPRECATED
#   (Only needed for development mode)

# Lines 1140-1209: _determine_jsonb_column() - DEPRECATED
#   (Sample query logic - use metadata instead)
```

**Total cleanup:** ~220 lines removed

**Justification:**
- No more object instantiation (always return RustResponseBytes)
- No more runtime JSONB detection (use registration metadata)
- Massive simplification!

---

## 📊 Expected Results

### Code Reduction
```
Current:  ~2100 lines in db.py
After:    ~1100 lines in db.py
Removed:  ~1000 lines (48% reduction!)
```

### Complexity Reduction
```
Before: 6 methods, 3 execution paths, mode detection, passthrough logic
After:  2 methods, 1 execution path, no branching
```

### Performance
```
Before: Sometimes fast (if passthrough enabled), sometimes slow
After:  Always fast (always Rust pipeline)
```

---

## ⚠️ Breaking Changes

### API Changes

**Old usage:**
```python
# Old: No field_name parameter
users = await repo.find("users")  # Returns list[dict]
user = await repo.find_one("users", id="123")  # Returns dict | None
```

**New usage:**
```python
# New: field_name parameter required
users = await repo.find("users", "users", info)  # Returns RustResponseBytes
user = await repo.find_one("users", "user", info, id="123")  # Returns RustResponseBytes
```

### Migration Guide for Calling Code

**GraphQL resolvers (most common case):**

```python
# OLD:
@strawberry.field
async def users(self, info: Info) -> List[User]:
    return await repo.find("users")

# NEW:
@strawberry.field
async def users(self, info: Info) -> RustResponseBytes:
    return await repo.find("users", "users", info)
```

**Python code that needs dicts (edge case):**

```python
# If you really need list[dict] for Python processing:
result = await repo.find("users", "users", info)
json_str = result.bytes.decode("utf-8")
data = json.loads(json_str)
users = data["data"]["users"]  # list[dict]
```

---

## 🧪 Testing Strategy

### Phase 1: Update Existing Tests

**Files to update:**
- `tests/integration/database/repository/*.py`
- `tests/unit/repository/*.py`

**Changes needed:**
1. Add `field_name` parameter to all `find()` calls
2. Update assertions for `RustResponseBytes` return type
3. Parse JSON where tests check dict values

**Example:**
```python
# OLD:
users = await repo.find("users")
assert len(users) == 5
assert users[0]["name"] == "Alice"

# NEW:
result = await repo.find("users", "users")
assert isinstance(result, RustResponseBytes)
data = json.loads(result.bytes.decode("utf-8"))
users = data["data"]["users"]
assert len(users) == 5
assert users[0]["name"] == "Alice"
```

### Phase 2: Add New Tests

**Test unified behavior:**

```python
# tests/integration/repository/test_unified_rust_path.py

async def test_find_always_returns_rust_bytes():
    """Verify find() always returns RustResponseBytes."""
    result = await repo.find("users", "users")
    assert isinstance(result, RustResponseBytes)
    assert result.content_type == "application/json"

async def test_find_transformation_applied():
    """Verify Rust transformation (snake_case → camelCase)."""
    result = await repo.find("users", "users")
    data = json.loads(result.bytes.decode("utf-8"))
    users = data["data"]["users"]

    # Should have camelCase keys
    assert "firstName" in users[0]
    assert "lastName" in users[0]

    # Should have __typename
    assert users[0]["__typename"] == "User"

async def test_find_uses_metadata_not_sample_query():
    """Verify no sample query is executed (uses metadata)."""
    # Mock connection to track queries
    with patch.object(repo._pool, "connection") as mock_conn:
        await repo.find("users", "users")

        # Should only execute main query (no sample query)
        assert mock_conn.call_count == 1
```

---

## 🚀 Deployment Strategy

### Step 1: Feature Branch
```bash
git checkout -b refactor/unified-rust-architecture
```

### Step 2: Implement Changes (follow phases 1-7)

### Step 3: Run Tests
```bash
# All tests must pass
uv run pytest tests/ -v

# Check test coverage
uv run pytest --cov=src/fraiseql --cov-report=html
```

### Step 4: Update Examples
```bash
# Update all example files to use new API
# Files: examples/*/queries.py
```

### Step 5: Update Documentation
- Update README.md with new API
- Update docs/core/queries-and-mutations.md
- Add migration guide

### Step 6: Create PR
```bash
git add .
git commit -m "refactor: unified Rust-first architecture (remove mode branching)"
git push origin refactor/unified-rust-architecture
```

### Step 7: Merge & Release
- Merge to `dev` branch
- Tag new version: `v1.0.0` (breaking change)
- Update CHANGELOG.md

---

## 📚 Documentation Updates Needed

### 1. Update README.md

**Section to update:** "Core Concepts - Repository Methods"

**New documentation:**
```markdown
## Repository Methods

FraiseQL uses a unified Rust-first architecture for all queries:

```python
# Find multiple records
users = await repo.find("users", "users", info)

# Find single record
user = await repo.find_one("users", "user", info, id="123")
```

**Return type:** All methods return `RustResponseBytes` which FastAPI sends directly as HTTP response.

**Performance:** PostgreSQL → Rust → HTTP with zero Python string operations.
```

### 2. Update Migration Guide

**Create:** `docs/migration/v0-to-v1.md`

**Content:**
- List all breaking changes
- Show before/after examples
- Explain performance benefits
- Provide troubleshooting tips

---

## 🐛 Troubleshooting

### Issue: "field_name parameter missing"

**Error:**
```
TypeError: find() missing 1 required positional argument: 'field_name'
```

**Solution:**
Add the field_name parameter (usually same as view name):
```python
# Before:
await repo.find("users")

# After:
await repo.find("users", "users", info)
```

### Issue: "Expected dict, got RustResponseBytes"

**Error:**
```
TypeError: Expected dict, got RustResponseBytes
```

**Solution:**
If you need dict for Python processing, parse the JSON:
```python
result = await repo.find("users", "users", info)
json_str = result.bytes.decode("utf-8")
data = json.loads(json_str)
users = data["data"]["users"]  # list[dict]
```

### Issue: "Sample query still being executed"

**Check:**
1. Ensure metadata is registered:
   ```python
   register_type_for_view("users", User, jsonb_column="data")
   ```

2. Check logs - should NOT see "using sample query"

---

## ✅ Success Criteria

### Code Quality
- [ ] No mode detection logic remaining
- [ ] No passthrough branching logic
- [ ] No sample query logic
- [ ] Only 2 public methods: `find()` and `find_one()`
- [ ] Both methods < 50 lines each
- [ ] Total db.py < 1200 lines (down from 2100)

### Performance
- [ ] All queries use Rust pipeline
- [ ] No Python JSON parsing in critical path
- [ ] No sample queries (100% metadata-based)

### Tests
- [ ] All existing tests pass (with updates)
- [ ] New tests for unified behavior
- [ ] Test coverage > 90%

### Documentation
- [ ] README updated
- [ ] Migration guide created
- [ ] Examples updated
- [ ] Docstrings accurate

---

## 💡 Tips for Implementation

### Start Small
1. Implement Phase 1 (`find()`) first
2. Get tests passing
3. Then do Phase 2 (`find_one()`)
4. Then cleanup phases 3-7

### Test Frequently
```bash
# After each phase:
uv run pytest tests/integration/repository/ -v -x
```

### Use Git Commits
```bash
# Good commit structure:
git commit -m "refactor(phase1): simplify find() to use unified Rust path"
git commit -m "refactor(phase2): simplify find_one() to use unified Rust path"
git commit -m "refactor(phase3): remove deprecated methods"
# etc.
```

### Ask for Help
- If you're stuck on a phase, ask for code review
- If tests fail unexpectedly, share the error message
- If performance doesn't improve, check profiling data

---

## 🎯 Final Architecture

### Clean, Simple, Fast

```python
class FraiseQLRepository:
    """Unified Rust-first repository.

    PostgreSQL → Rust → HTTP (zero Python overhead).
    """

    async def find(view_name, field_name, info=None, **kwargs) -> RustResponseBytes:
        """Find multiple records (always fast)."""
        # ~40 lines: build query → execute via Rust → return bytes

    async def find_one(view_name, field_name, info=None, **kwargs) -> RustResponseBytes:
        """Find single record (always fast)."""
        # ~40 lines: build query → execute via Rust → return bytes
```

**That's it!**

No modes, no branching, no complexity. Just pure performance.

---

## 📞 Questions?

If you get stuck or have questions:

1. **Read the code comments** - they explain the "why"
2. **Check the tests** - they show working examples
3. **Ask for help** - share your error message or confusion
4. **Review this plan** - maybe you missed a step

**Remember:** The goal is simplification. If something feels complex, it probably is. Keep asking "do we really need this?" until the answer is obviously yes.

Good luck! 🚀
