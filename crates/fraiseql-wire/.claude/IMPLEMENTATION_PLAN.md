# Implementation Plan: Clippy Cleanliness + JSONB Field Projection

## Executive Summary

Two parallel work streams:

1. **Clippy Cleanup** (19 fixable issues + 22 doc comments)
   - Automatic fixes: unused imports, variable mutability, dead code
   - Manual fixes: documentation comments, logic improvements
   - Safe to ignore: large enum variants, too many arguments (architectural decisions)

2. **JSONB Field Projection** (New Feature)
   - Add optional field projection to SELECT clause
   - Support both simple fields (`data->>'name'`) and nested paths (`data->'user'->>'name'`)
   - Maintain backward compatibility (default: `SELECT data`)
   - Keep single-column contract for wire protocol

---

## Part 1: Clippy Cleanup

### 1.1 Dead Code & Unused Symbols (14 issues)

**Files to modify**:

1. **`src/operators/where_operator.rs:7`** - Unused import `FieldSource`
   - Remove import: `use super::order_by::FieldSource;`
   - Status: **SAFE TO AUTO-FIX**

2. **`src/stream/query_stream.rs:15`** - Unused import `StreamExt`
   - Remove import: `use futures::StreamExt;`
   - Status: **SAFE TO AUTO-FIX**

3. **`src/stream/json_stream.rs:19-20`** - Unused constants `STATE_COMPLETE`, `STATE_ERROR`
   - Keep `STATE_RUNNING` and `STATE_PAUSED` (used)
   - Delete unused two constants
   - Note: Comments reference these for documentation, update comments if needed
   - Status: **SAFE TO DELETE** (legacy Phase 8 optimization code)

4. **`src/stream/json_stream.rs:77-78`** - Unused field `soft_limit_warn_threshold`, `soft_limit_fail_threshold`
   - Delete from struct (was copied from QueryBuilder, never used here)
   - Clean up related initialization code
   - Status: **SAFE TO DELETE**

5. **`src/stream/json_stream.rs:351-445`** - Multiple unused methods
   - Methods like: `state_atomic_set_error()`, `is_error_atomic()`, etc.
   - Delete unused method bodies
   - Status: **VERIFY THESE AREN'T CALLED** before deleting

6. **`src/stream/adaptive_chunking.rs:34-36`** - Unused struct fields `items_buffered`, `timestamp`
   - Delete from struct
   - Status: **SAFE TO DELETE**

### 1.2 Variable Mutability Issues (7 issues)

**File**: `src/connection/conn.rs`

| Line | Variable | Fix | Status |
|------|----------|-----|--------|
| 704 | `strategy` | Remove `mut` (never modified) | AUTO-FIX |
| 709 | `adaptive` | Remove `mut` or rename to `_adaptive` (unused) | AUTO-FIX |
| 723 | `current_chunk_size` | Remove `mut` or rename | AUTO-FIX |
| 680 | `stream` | Remove `mut` (never modified) | AUTO-FIX |
| 603 | `row_desc` | Simplify: value overwritten before read | AUTO-FIX |
| 386 | `status` pattern | Change to `status: _` or `_` | AUTO-FIX |
| 619 | `secret_key` pattern | Change to `secret_key: _` or `_` | AUTO-FIX |

### 1.3 Logic & Code Quality Issues (8 issues)

**File**: `src/auth/scram.rs`

1. **Line 123** - `to_vec()` on already-owned data
   - Change: `vec.to_vec()` → `vec`
   - Status: **AUTO-FIX**

2. **Line 213** - Needless borrow for generic arg
   - Change: `&value` → `value`
   - Status: **AUTO-FIX**

3. **Lines 204, 239** - Unused `Result` from pbkdf2
   - Prefix with `let _ =` or `_ =`
   - Status: **VERIFY CONTEXT** before applying

**File**: `src/operators/sql_gen.rs`

4. **Line 65** - Redundant closure
   - Change: `|x| function(x)` → just `function`
   - Status: **AUTO-FIX**

**File**: `src/protocol/decode.rs`

5. **Line 159** - Unnecessary cast `i32 -> i32`
   - Remove cast
   - Status: **AUTO-FIX**

**File**: `src/stream/adaptive_chunking.rs`

6. **Line 153** - Collapsible nested if
   - Merge `if condition1 { if condition2 { ... } }` → `if condition1 && condition2 { ... }`
   - Status: **VERIFY LOGIC FIRST**

7. **Line 248** - Manual `!RangeInclusive::contains`
   - Change manual check to `.contains()` method
   - Status: **AUTO-FIX**

8. **Lines 268, 273** - Returning let binding
   - Change: `let x = expr; x` → just `expr`
   - Status: **AUTO-FIX**

### 1.4 Missing Documentation (22 issues)

**File**: `src/operators/where_operator.rs`

All in enum variants with struct-like fields. Add doc comments to **each field**:

```rust
L2Distance {
    /// The vector field to compare against
    field: Field,
    /// The embedding vector for distance calculation
    vector: Vec<f32>,
    /// Distance threshold for comparison
    threshold: f32,
}
```

**Affected variants** (22 fields total):

- `L2Distance` (3 fields)
- `CosineDistance` (3 fields)
- `InnerProduct` (3 fields)
- `JaccardDistance` (3 fields)
- `Matches` (3 fields)
- `PlainQuery` (2 fields)
- `PhraseQuery` (3 fields)
- `WebsearchQuery` (3 fields)
- `InSubnet` (2 fields)
- `ContainsSubnet` (2 fields)
- `ContainsIP` (2 fields)
- `IPRangeOverlap` (2 fields)

### 1.5 Issues to IGNORE (3 deliberate design decisions)

1. **Large enum variants** (`connection/transport.rs`)
   - `TcpVariant` with TLS context (1104 bytes)
   - **Reason**: Cryptographic operations require large buffers, unavoidable
   - **Action**: Add `#[allow(clippy::large_enum_variant)]` if needed

2. **Too many arguments** (`connection/conn.rs:568` - `streaming_query()`)
   - 9 parameters for adaptive chunking configuration
   - **Reason**: Could refactor into config struct, but currently acceptable
   - **Action**: Leave as-is for now

---

## Part 2: JSONB Field Projection

### 2.1 Architecture Overview

**Current State**:

```rust
client.query("users")
    .where_sql("status='active'")
    .execute()
    .await?
// → SELECT data FROM users WHERE status='active'
```

**Desired State**:

```rust
client.query("users")
    .select(Field::JsonbField("name".to_string()))
    .select(Field::JsonbField("email".to_string()))
    .where_sql("status='active'")
    .execute()
    .await?
// → SELECT (data->>'name'), (data->>'email') FROM users WHERE status='active'
```

**Key Constraints**:

- Wire protocol: still single JSON column (no changes to protocol)
- Backward compatible: default is `SELECT data` (all rows return as-is)
- API: fluent builder pattern (add `.select()` methods)

### 2.2 File Changes Required

#### 2.2.1 QueryBuilder Structure Changes

**File**: `src/client/query_builder.rs`

```rust
pub struct QueryBuilder<T: DeserializeOwned + Unpin + 'static = serde_json::Value> {
    // ... existing fields ...

    // NEW FIELD
    /// Optional field projections. If None, defaults to SELECT data
    /// If Some, generates: SELECT (projection_1), (projection_2), ... FROM entity
    select_projections: Option<Vec<Field>>,
}

impl<T: DeserializeOwned + Unpin + 'static> QueryBuilder<T> {
    pub(crate) fn new(client: FraiseClient, entity: impl Into<String>) -> Self {
        Self {
            // ... existing initialization ...
            select_projections: None,  // NEW
            // ... rest ...
        }
    }

    // NEW METHODS

    /// Add a field projection to the SELECT clause
    ///
    /// # Examples
    ///
    /// ```ignore
    /// client.query("users")
    ///     .select(Field::JsonbField("name".to_string()))
    ///     .select(Field::JsonbField("email".to_string()))
    ///     .execute()
    ///     .await?
    /// // Generates: SELECT (data->>'name'), (data->>'email') FROM users
    /// ```
    pub fn select(mut self, field: Field) -> Self {
        self.select_projections
            .get_or_insert_with(Vec::new)
            .push(field);
        self
    }

    /// Set all field projections, replacing any previous selections
    ///
    /// # Examples
    ///
    /// ```ignore
    /// client.query("users")
    ///     .select_fields(vec![
    ///         Field::JsonbField("name".to_string()),
    ///         Field::JsonbField("email".to_string()),
    ///     ])
    ///     .execute()
    ///     .await?
    /// ```
    pub fn select_fields(mut self, fields: Vec<Field>) -> Self {
        self.select_projections = Some(fields);
        self
    }

    // MODIFIED EXISTING METHOD

    fn build_sql(&self) -> Result<String> {
        // Build SELECT clause
        let select_clause = if let Some(ref projections) = self.select_projections {
            if projections.is_empty() {
                // Empty projections means explicit "select nothing" - treat as SELECT data
                format!("SELECT data FROM {}", self.entity)
            } else {
                // Multiple projections - generate SELECT (proj1), (proj2), ...
                let projection_sqls: Vec<String> = projections
                    .iter()
                    .map(|f| f.to_sql())
                    .collect();
                format!("SELECT {} FROM {}", projection_sqls.join(", "), self.entity)
            }
        } else {
            // No projections specified - default to full data
            format!("SELECT data FROM {}", self.entity)
        };

        let mut sql = select_clause;

        // ... rest of method unchanged (WHERE, ORDER BY, LIMIT, OFFSET) ...

        Ok(sql)
    }
}
```

#### 2.2.2 Field Module Review

**File**: `src/operators/field.rs` - NO CHANGES NEEDED

The module already supports what we need:

- `Field::JsonbField(String)` → `(data->>'name')`
- `Field::JsonbPath(Vec<String>)` → `(data->'user'->>'name')`
- `Field::DirectColumn(String)` → `direct_column`

The `to_sql()` method is production-ready.

#### 2.2.3 Stream/Protocol Considerations

**NO PROTOCOL CHANGES NEEDED**

The key insight: we're still selecting a single column from Postgres, just extracting it differently:

```sql
-- Old: SELECT data FROM users
-- Result rows: [JSON object 1], [JSON object 2], ...

-- New: SELECT (data->>'name') FROM users
-- Result rows: ["name1"], ["name2"], ...
```

Both still return text from Postgres. The JSON stream decoder will receive text values instead of full JSON objects.

**Deserialization** (`stream/json_stream.rs`):

- If projections selected: receive text strings, deserialize to T
- If no projections: receive full JSON, deserialize to T as usual
- Type T is still consumer-side only, framework doesn't care what was selected

### 2.3 Testing Strategy

#### 2.3.1 Unit Tests

**File**: `tests/integration_tests.rs` (or create `tests/field_projection_tests.rs`)

```rust
#[tokio::test]
async fn test_select_single_field() {
    let client = setup_test_client().await;

    // Test: SELECT (data->>'name') FROM v_project
    let stream = client
        .query::<serde_json::Value>("v_project")
        .select(Field::JsonbField("name".to_string()))
        .execute()
        .await
        .expect("query failed");

    // Verify:
    // - SQL generated correctly
    // - Values are strings (name values extracted)
    // - Deserialization works
}

#[tokio::test]
async fn test_select_multiple_fields() {
    let client = setup_test_client().await;

    // Test: SELECT (data->>'id'), (data->>'name') FROM v_project
    let stream = client
        .query::<serde_json::Value>("v_project")
        .select(Field::JsonbField("id".to_string()))
        .select(Field::JsonbField("name".to_string()))
        .execute()
        .await
        .expect("query failed");

    // Verify multiple fields are selected
}

#[tokio::test]
async fn test_select_nested_path() {
    let client = setup_test_client().await;

    // Test: SELECT (data->'user'->>'name') FROM v_project
    let stream = client
        .query::<serde_json::Value>("v_project")
        .select(Field::JsonbPath(vec!["user".to_string(), "name".to_string()]))
        .execute()
        .await
        .expect("query failed");

    // Verify nested path extraction works
}

#[tokio::test]
async fn test_select_with_where_and_order() {
    let client = setup_test_client().await;

    // Test: SELECT (data->>'name') FROM v_project WHERE ... ORDER BY ...
    let stream = client
        .query::<serde_json::Value>("v_project")
        .select(Field::JsonbField("name".to_string()))
        .where_sql("status='active'")
        .order_by("name ASC")
        .execute()
        .await
        .expect("query failed");

    // Verify combination works correctly
}

#[tokio::test]
async fn test_default_select_data() {
    let client = setup_test_client().await;

    // Test backward compatibility: no select() call → SELECT data
    let stream = client
        .query::<serde_json::Value>("v_project")
        .execute()
        .await
        .expect("query failed");

    // Verify full JSON object is returned
}

#[tokio::test]
async fn test_select_fields_replaces_previous() {
    let client = setup_test_client().await;

    // Test: select_fields() replaces select() additions
    let stream = client
        .query::<serde_json::Value>("v_project")
        .select(Field::JsonbField("name".to_string()))
        .select_fields(vec![Field::JsonbField("email".to_string())])
        .execute()
        .await
        .expect("query failed");

    // Verify only email is selected (not name + email)
}
```

#### 2.3.2 SQL Generation Verification

```rust
#[test]
fn test_build_sql_with_projections() {
    // Mock QueryBuilder with projections
    // Verify build_sql() generates correct SQL strings

    // Case 1: Single field
    // .select(Field::JsonbField("name")) → "SELECT (data->>'name') FROM users"

    // Case 2: Multiple fields
    // .select(Field::JsonbField("id")).select(Field::JsonbField("name"))
    // → "SELECT (data->>'id'), (data->>'name') FROM users"

    // Case 3: No projections (default)
    // No select() calls → "SELECT data FROM users"

    // Case 4: With WHERE/ORDER
    // .select(...).where_sql(...).order_by(...)
    // → "SELECT (...) FROM users WHERE ... ORDER BY ..."
}
```

### 2.4 Breaking Changes & Compatibility

**BACKWARD COMPATIBLE** ✅

- Existing code without `.select()` calls continues to work
- Default behavior unchanged: `SELECT data`
- New API is additive (optional `.select()` methods)

**Public API Surface Changes**:

- Add `select(Field) -> Self`
- Add `select_fields(Vec<Field>) -> Self`
- Export `Field` from `client` module if not already exported

---

## Part 3: Implementation Order

### Phase 1: Clippy Cleanup (Quick Wins)

1. Remove unused imports (2 files)
2. Remove unused constants and fields (2 files)
3. Fix variable mutability (1 file - auto-fix friendly)
4. Fix logic improvements (4 files)
5. Add documentation comments (1 file - 22 doc comments)

**Expected outcome**: `cargo clippy -- -D warnings` passes with 0 errors

### Phase 2: JSONB Field Projection

1. Add `select_projections` field to `QueryBuilder`
2. Add `.select()` and `.select_fields()` methods
3. Update `build_sql()` to generate correct SELECT clause
4. Add SQL generation tests
5. Add integration tests
6. Verify backward compatibility

**Expected outcome**: New feature works, old tests pass, new tests pass

### Phase 3: Verification

1. Run `cargo clippy -- -D warnings` (0 errors)
2. Run `cargo test --all` (all tests pass)
3. Run `cargo test --test integration` (projection tests pass)
4. Code review of field projection implementation
5. Documentation update in README/examples

---

## Implementation Notes

### Important Implementation Details

1. **Field::to_sql() already works perfectly** - no changes needed
2. **Wire protocol is unchanged** - single column, just extracted differently
3. **Deserialization stays consumer-side** - T parameter still works as-is
4. **Backward compatible by default** - None means SELECT data

### Potential Edge Cases

1. **Empty projections vector**: Treat same as `None` (SELECT data)
2. **Type T deserialization with text strings**: Should work fine for most types (String, serde_json::Value)
3. **ORDER BY with projected fields**: User must ensure ORDER BY field is one of the selected fields
4. **Memory implications**: Projected fields reduce network transfer size (bonus!)

### Why This Works

The brilliance of the current design:

```
QueryBuilder<T> generates SQL → Postgres executes → Stream receives values
Deserialization(values) → T → User

T is ONLY used by deserialization, so it doesn't matter what we SELECT
as long as the wire format stays compatible (single column = text)
```

So we can change what we SELECT without touching the type system! 🎯

---

## Deliverables

1. ✅ All 61 clippy warnings fixed
2. ✅ JSONB field projection feature added
3. ✅ Integration tests passing
4. ✅ Backward compatibility verified
5. ✅ Documentation updated
6. ✅ Clean `cargo clippy -- -D warnings`
