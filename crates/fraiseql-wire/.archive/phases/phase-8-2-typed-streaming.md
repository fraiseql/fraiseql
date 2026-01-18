# Phase 8.2: Typed Streaming Implementation Plan

**Status**: Design & Planning
**Target**: Add generic, type-safe streaming API with automatic JSON deserialization
**Priority**: 🟡 Medium (v0.2.0 nice-to-have after TLS/Config)
**Timeline**: 1-2 weeks
**Effort**: Medium (generic trait bounds, deserialization integration)

---

## Objective

Enable type-safe JSON streaming where rows are automatically deserialized into user-defined types:

```rust
#[derive(Deserialize)]
struct Project {
    id: String,
    name: String,
    status: String,
}

// Type-safe streaming (consumer-side only)
let mut stream = client
    .query::<Project>("projects")
    .where_sql("status='active'")    // ← Still SQL, unaffected by type T
    .execute()
    .await?;

while let Some(result) = stream.next().await {
    match result {
        Ok(project) => println!("Project: {}", project.name),
        Err(e) => eprintln!("Deserialization error: {}", e),
    }
}
```

**Key Requirements:**

- Generic `query::<T>()` API with automatic deserialization **at consumer boundary only**
- Preserve all existing filter/order APIs (`where_sql()`, `order_by()`, `where_rust()`)
- **Typing does NOT affect SQL, filtering, ordering, or wire protocol**
- Clear error messages for type mismatches
- Zero-copy JSON parsing where possible
- Backward compatible (serde_json::Value still works)
- Performance equivalent to current JSON path
- **Always support escape hatch: `query::<serde_json::Value>()`**

---

## Design Overview

### Current State

```rust
// Current API (v0.1.0) - JSON only
let mut stream = client
    .query("projects")
    .where_sql("status='active'")
    .execute()
    .await?;

while let Some(result) = stream.next().await {
    let json: serde_json::Value = result?;
    let name = json["name"].as_str();
    // Manual extraction
}
```

**Issue**: No compile-time type safety, manual JSON extraction prone to errors

### Proposed API

**Option A: Generic query builder (Recommended)**

```rust
// Type inference from context
let stream = client.query::<Project>("projects").execute().await?;

// Explicit type parameter
let stream = client.query::<Project>("projects").execute().await?;

// Still supports JSON if needed
let stream = client.query::<serde_json::Value>("projects").execute().await?;
```

**Option B: Separate typed method (Alternative)**

```rust
// Separate method, less ergonomic
let stream = client.query_typed::<Project>("projects").execute().await?;
```

**Recommendation**: Go with **Option A** (generic on `query()`) for consistency and ergonomics.

### Architecture

```
┌─────────────────────────┐
│   FraiseClient          │
│  - query()              │ ← Generic over T
│    query::<T>(entity)   │
└──────────┬──────────────┘
           │
           └─ QueryBuilder<T>
              ├─ where_sql()      → QueryBuilder<T>
              ├─ order_by()       → QueryBuilder<T>
              ├─ where_rust()     → QueryBuilder<T>
              └─ execute()        → Stream<Item = Result<T>>

┌─────────────────────────┐
│   FilteredStream<T>     │
│  - Filters & deserializes
└─────────────────────────┘

┌─────────────────────────┐
│   TypedJsonStream<T>    │
│  - Wraps JsonStream
│  - Deserializes each Value → T
└─────────────────────────┘
```

---

## ⚠️ CRITICAL: Typing is Consumer-Side Only

**This must be explicit everywhere in code, docs, and comments.**

### What Typing Does NOT Affect

✅ **SQL is unaffected**

```rust
// Typing parameter T has ZERO impact on generated SQL
client.query::<Project>("projects").where_sql("status='active'").execute()
// Still generates: SELECT data FROM v_projects WHERE status='active'
```

✅ **Filtering is unaffected**

```rust
// where_sql() produces same SQL regardless of T
// where_rust() operates on JSON regardless of T
// ORDER BY is the same regardless of T
```

✅ **Wire protocol is unaffected**

```rust
// Network communication identical for T=Project vs T=Value
// Same row format, same chunking, same cancellation
```

✅ **Escape hatch always available**

```rust
// Always supported, zero-cost
client.query::<serde_json::Value>("projects").execute()
// Identical to untyped query, perfect for debugging
```

### What Typing ONLY Affects

✅ **Consumer-side deserialization at poll_next()**

```rust
// Type T is resolved at: `stream.next().await`
// Deserialization happens here, nowhere else
fn poll_next(...) -> Poll<Option<Result<T>>> {
    // Only place T matters ↑
}
```

✅ **Error messages**

```rust
// Type name included in error context
// Helps debugging, doesn't affect operation
```

### Implementation Guarantee

Add this to all relevant comments in code:

```rust
/// Generic type parameter T is **consumer-side only**.
///
/// The type T does NOT affect:
/// - SQL generation (still `SELECT data FROM v_{entity}`)
/// - Filtering (where_sql, where_rust, order_by unchanged)
/// - Wire protocol (same as untyped streaming)
/// - Performance (< 2% overhead from serde deserialization)
///
/// Type T ONLY affects:
/// - How each row is deserialized when consumed
/// - Error messages (type name included)
///
/// Escape hatch:
/// Use `query::<serde_json::Value>(...)` for debugging
/// or forward-compatibility without code changes.
```

---

### Type System

```rust
/// Generic query builder (new)
pub struct QueryBuilder<T: DeserializeOwned> {
    client: FraiseClient,
    entity: String,
    sql_predicates: Vec<String>,
    rust_predicate: Option<Box<dyn Fn(&Value) -> bool + Send>>,
    order_by: Option<String>,
    chunk_size: usize,
    _phantom: std::marker::PhantomData<T>,
}

/// Typed stream implementation (new)
pub struct TypedJsonStream<T: DeserializeOwned> {
    inner: Box<dyn Stream<Item = Result<Value>> + Unpin>,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: DeserializeOwned> Stream for TypedJsonStream<T> {
    type Item = Result<T>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        // Deserialize Value → T on each item
    }
}
```

### Rust Predicate Handling

**Challenge**: Rust predicates operate on JSON values, but typed streams deserialize to T.

**Solution**: Keep predicates JSON-based (operate on filtered raw JSON before deserialization):

```rust
client
    .query::<Project>("projects")
    .where_sql("status='active'")
    .where_rust(|json| {
        // Still receives serde_json::Value
        // Applied BEFORE deserialization to T
        json["estimated_cost"].as_f64().unwrap_or(0.0) > 10_000.0
    })
    .execute()
    .await?
```

**Why**: Deserialization happens after filtering. Rust predicates filter the raw JSON stream, then successful matches are deserialized to T.

---

## Implementation Plan

### Phase 8.2.1: Core Type System

**Files**:

- `src/client/query_builder.rs` (MODIFY)
- `src/stream/typed_stream.rs` (NEW)

#### QueryBuilder Refactoring

Current (non-generic):

```rust
pub struct QueryBuilder {
    // ...
}
```

New (generic):

```rust
pub struct QueryBuilder<T: DeserializeOwned = serde_json::Value> {
    client: FraiseClient,
    entity: String,
    sql_predicates: Vec<String>,
    rust_predicate: Option<Box<dyn Fn(&Value) -> bool + Send>>,
    order_by: Option<String>,
    chunk_size: usize,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: DeserializeOwned> QueryBuilder<T> {
    pub fn new(client: FraiseClient, entity: impl Into<String>) -> Self { ... }

    /// Add SQL WHERE predicate (type T does NOT affect SQL)
    pub fn where_sql(mut self, predicate: impl Into<String>) -> Self { ... }

    /// Add Rust-side predicate on JSON (type T does NOT affect filtering)
    pub fn where_rust<F>(mut self, predicate: F) -> Self
    where
        F: Fn(&Value) -> bool + Send + 'static
    { ... }

    /// Set ORDER BY (type T does NOT affect ordering)
    pub fn order_by(mut self, order: impl Into<String>) -> Self { ... }

    pub fn chunk_size(mut self, size: usize) -> Self { ... }

    /// Execute query.
    ///
    /// Type T ONLY affects consumer-side deserialization at poll_next().
    /// SQL, filtering, ordering, and wire protocol are identical regardless of T.
    pub async fn execute(self) -> Result<Box<dyn Stream<Item = Result<T>> + Unpin>> {
        let sql = self.build_sql()?;
        let stream = self.client.execute_query(&sql, self.chunk_size).await?;

        if let Some(predicate) = self.rust_predicate {
            let filtered = FilteredStream::new(stream, predicate);
            Ok(Box::new(TypedJsonStream::<T>::new(Box::new(filtered))))
        } else {
            Ok(Box::new(TypedJsonStream::<T>::new(Box::new(stream))))
        }
    }
}
```

**Key Points:**

- Default type parameter `T = serde_json::Value` for backward compatibility
- All chainable methods preserve generic type
- `execute()` returns `Stream<Item = Result<T>>`
- Internal stream stays JSON until deserialization

#### TypedJsonStream Implementation

```rust
pub struct TypedJsonStream<T: DeserializeOwned> {
    inner: Box<dyn Stream<Item = Result<Value>> + Unpin>,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: DeserializeOwned> TypedJsonStream<T> {
    pub fn new(inner: Box<dyn Stream<Item = Result<Value>> + Unpin>) -> Self {
        Self {
            inner,
            _phantom: PhantomData,
        }
    }

    fn deserialize_value(value: Value) -> Result<T> {
        serde_json::from_value::<T>(value)
            .map_err(|e| Error::Deserialization {
                type_name: std::any::type_name::<T>().to_string(),
                details: e.to_string(),
            })
    }
}

impl<T: DeserializeOwned + Unpin> Stream for TypedJsonStream<T> {
    type Item = Result<T>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        match self.inner.poll_next_unpin(cx) {
            Poll::Ready(Some(Ok(value))) => {
                Poll::Ready(Some(Self::deserialize_value(value)))
            }
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}
```

**Design Decisions:**

- Deserialization happens in `poll_next()` (lazy, per-item) **ONLY**
- Type name captured in errors for debugging
- PhantomData for compile-time type safety without runtime cost
- Zero-copy where serde_json supports it

---

## 🚪 Escape Hatch: Always Support `query::<Value>()`

This is **not a fallback for broken types** - it's a **first-class feature**.

### Use Cases

```rust
// 1. Debugging: Check actual JSON structure
let stream = client.query::<serde_json::Value>("projects").execute().await?;
while let Some(result) = stream.next().await {
    println!("Raw JSON: {:?}", result?);  // See what's actually there
}

// 2. Forward compatibility: Type definitions change, code doesn't
// Old code: client.query::<Project>("projects")
// New code: client.query::<serde_json::Value>("projects")  // No type change needed
//           Extract fields manually instead

// 3. Operations workflow: Generic handler for any entity
async fn export_entity(client: &FraiseClient, entity: &str) -> Result<()> {
    let mut stream = client.query::<serde_json::Value>(entity).execute().await?;
    while let Some(result) = stream.next().await {
        println!("{}", serde_json::to_string(&result?)?);
    }
    Ok(())
}

// 4. Partial type safety: Some fields typed, some untyped
#[derive(Deserialize)]
struct PartialProject {
    id: String,
    #[serde(skip)]
    _extra: (),
}
let stream = client.query::<PartialProject>("projects").execute().await?;

// 5. Opt-out for any reason: Just use Value
let stream = client.query::<serde_json::Value>("projects").execute().await?;
```

### Implementation Notes

The escape hatch **must work identically** to untyped queries:

```rust
// These MUST be identical in behavior, performance, wire protocol:
let stream1 = client.query::<serde_json::Value>("projects").execute().await?;

// Future: If typing ever added to query()
// let stream2 = client.query("projects").execute().await?;
// stream1 and stream2 must be identical
```

### Documentation Requirement

Always mention the escape hatch in rustdoc:

```rust
/// Create a type-safe query.
///
/// The generic type `T` controls consumer-side deserialization only.
/// SQL, filtering, ordering, and wire protocol are unaffected.
///
/// To access raw JSON (for debugging or compatibility):
/// ```ignore
/// let stream = client.query::<serde_json::Value>("entity").execute().await?;
/// ```
pub fn query<T: DeserializeOwned>(&self, entity: impl Into<String>) -> QueryBuilder<T>
```

#### Error Type Extension

Current:

```rust
pub enum Error {
    Protocol(String),
    Json(serde_json::error::Error),
    // ...
}
```

New variant:

```rust
pub enum Error {
    // ... existing variants

    Deserialization {
        type_name: String,
        details: String,
    },
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        // Could be JSON parsing or deserialization error
        // Use Display to capture the details
        Error::Deserialization {
            type_name: "serde_json::Value".to_string(),
            details: err.to_string(),
        }
    }
}
```

**Example error message:**

```
Deserialization error for type 'Project':
  missing field `name` at line 1 column 42
```

**Tests**:

- [ ] QueryBuilder generic type parameter
- [ ] where_sql() preserves type
- [ ] where_rust() preserves type
- [ ] order_by() preserves type
- [ ] chunk_size() preserves type
- [ ] TypedJsonStream creation
- [ ] Deserialization on poll_next()
- [ ] Type name in error messages
- [ ] PhantomData doesn't add size

---

### Phase 8.2.2: FraiseClient Integration

**File**: `src/client/fraise_client.rs` (MODIFY)

#### Current Implementation

```rust
pub struct FraiseClient {
    connection: Connection,
}

impl FraiseClient {
    pub fn query(&self, entity: impl Into<String>) -> QueryBuilder {
        QueryBuilder::new(self.clone(), entity)
    }
}
```

#### New Generic Implementation

```rust
impl FraiseClient {
    /// Create a new query builder with automatic deserialization
    ///
    /// # Type Parameters
    /// - `T`: Target type implementing [serde::Deserialize]
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use serde::Deserialize;
    ///
    /// #[derive(Deserialize)]
    /// struct Project {
    ///     id: String,
    ///     name: String,
    /// }
    ///
    /// let stream = client.query::<Project>("projects").execute().await?;
    /// ```
    pub fn query<T: DeserializeOwned>(
        &self,
        entity: impl Into<String>
    ) -> QueryBuilder<T> {
        QueryBuilder::new(self.clone(), entity)
    }
}
```

**Key Points:**

- Generic over `T: DeserializeOwned`
- Default type parameter in QueryBuilder handles backward compatibility
- Turbofish syntax: `client.query::<Project>()`
- Type inference from context works when possible

**Tests**:

- [ ] query::<Value>() works (backward compat)
- [ ] query::<CustomType>() works
- [ ] Type inference from context
- [ ] Turbofish syntax
- [ ] FraiseClient::clone() works with generic

---

### Phase 8.2.3: FilteredStream Enhancement

**File**: `src/stream/filter.rs` (MODIFY)

Current FilteredStream operates on JSON Values. We need to ensure it works with the new architecture:

```rust
pub struct FilteredStream {
    inner: Box<dyn Stream<Item = Result<Value>> + Unpin>,
    predicate: Box<dyn Fn(&Value) -> bool + Send>,
}

impl Stream for FilteredStream {
    type Item = Result<Value>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        loop {
            match self.inner.poll_next_unpin(cx) {
                Poll::Ready(Some(Ok(value))) => {
                    if (self.predicate)(&value) {
                        return Poll::Ready(Some(Ok(value)));
                    }
                    // Continue looping to filter out non-matching items
                }
                Poll::Ready(Some(Err(e))) => return Poll::Ready(Some(Err(e))),
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}
```

**No changes needed** - FilteredStream operates on JSON before deserialization, so it stays the same.

**Tests**:

- [ ] FilteredStream still works with typed streams
- [ ] Filtering happens before deserialization
- [ ] Performance impact minimal

---

### Phase 8.2.4: Comprehensive Tests

**File**: `tests/typed_streaming_integration.rs` (NEW)

#### Test Categories

**Basic Type Deserialization**

```rust
#[derive(Deserialize)]
struct SimpleProject {
    id: String,
    name: String,
}

#[tokio::test]
async fn test_deserialize_simple_type() {
    // Create client, stream, verify deserialization
}

#[tokio::test]
async fn test_field_type_mismatch() {
    // Test error when field type doesn't match
}

#[tokio::test]
async fn test_missing_field() {
    // Test error when required field missing
}
```

**Nested Types**

```rust
#[derive(Deserialize)]
struct ProjectWithOwner {
    id: String,
    owner: UserInfo,
}

#[derive(Deserialize)]
struct UserInfo {
    id: String,
    email: String,
}

#[tokio::test]
async fn test_nested_deserialization() {
    // Test nested struct deserialization
}

#[tokio::test]
async fn test_deeply_nested() {
    // Test multiple levels of nesting
}
```

**Optional & Collection Types**

```rust
#[derive(Deserialize)]
struct ProjectWithTags {
    id: String,
    #[serde(default)]
    tags: Vec<String>,
    description: Option<String>,
}

#[tokio::test]
async fn test_optional_fields() {
    // Test Option<T> handling
}

#[tokio::test]
async fn test_collection_fields() {
    // Test Vec<T> handling
}

#[tokio::test]
async fn test_default_fields() {
    // Test #[serde(default)]
}
```

**Filtering & Ordering with Types**

```rust
#[tokio::test]
async fn test_where_sql_with_typed_stream() {
    // Stream<Project> with SQL filter
}

#[tokio::test]
async fn test_where_rust_with_typed_stream() {
    // Stream<Project> with Rust predicate
}

#[tokio::test]
async fn test_order_by_with_typed_stream() {
    // Stream<Project> with ORDER BY
}

#[tokio::test]
async fn test_combined_filters_and_order() {
    // Test all together
}
```

**Backward Compatibility**

```rust
#[tokio::test]
async fn test_query_value_still_works() {
    // Verify serde_json::Value works as before
    let stream = client.query::<serde_json::Value>("projects").execute().await?;
}

#[tokio::test]
async fn test_default_type_parameter() {
    // Test that default T = Value works
    let stream = client.query("projects").execute().await?;
}
```

**Error Messages**

```rust
#[tokio::test]
async fn test_deserialization_error_message() {
    // Verify clear error messages with type info
}

#[tokio::test]
async fn test_error_contains_type_name() {
    // Verify type name in error
}

#[tokio::test]
async fn test_error_contains_field_details() {
    // Verify serde details in error
}
```

**Performance**

```rust
#[bench]
fn bench_typed_vs_json(b: &mut Bencher) {
    // Compare Stream<Project> vs Stream<Value>
}

#[bench]
fn bench_deserialization_overhead(b: &mut Bencher) {
    // Measure deserialization cost
}
```

**Tests Checklist**:

- [ ] Basic struct deserialization
- [ ] Field type mismatches
- [ ] Missing required fields
- [ ] Nested types
- [ ] Optional fields
- [ ] Collections (Vec, HashMap, etc)
- [ ] Custom serde attributes (#[serde(rename)], etc)
- [ ] SQL filtering with typed stream
- [ ] Rust predicates with typed stream
- [ ] ORDER BY with typed stream
- [ ] Backward compatibility (Value still works)
- [ ] Default type parameter
- [ ] Error messages include type names
- [ ] Error messages include field details
- [ ] Zero-copy where possible
- [ ] Performance overhead < 2%

---

### Phase 8.2.5: Example Program

**File**: `examples/typed_streaming.rs` (NEW)

```rust
use fraiseql_wire::client::FraiseClient;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Project {
    id: String,
    name: String,
    status: String,
    #[serde(rename = "estimated_cost")]
    cost: f64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to database
    let client = FraiseClient::connect("postgres://localhost/testdb").await?;

    println!("=== Example 1: Simple typed query ===");
    simple_query(&client).await?;

    println!("\n=== Example 2: Type-safe filtering ===");
    filtered_query(&client).await?;

    println!("\n=== Example 3: Combining SQL + Rust filters ===");
    combined_filters(&client).await?;

    println!("\n=== Example 4: Error handling ===");
    error_handling(&client).await?;

    Ok(())
}

async fn simple_query(client: &FraiseClient) -> Result<(), Box<dyn std::error::Error>> {
    let mut stream = client
        .query::<Project>("project")
        .execute()
        .await?;

    let mut count = 0;
    while let Some(result) = stream.next().await {
        match result {
            Ok(project) => {
                println!("  [{}] {} ({})", project.id, project.name, project.status);
                count += 1;
            }
            Err(e) => eprintln!("  Error: {}", e),
        }
    }
    println!("  Total: {} projects", count);
    Ok(())
}

async fn filtered_query(client: &FraiseClient) -> Result<(), Box<dyn std::error::Error>> {
    let mut stream = client
        .query::<Project>("project")
        .where_sql("status='active'")
        .execute()
        .await?;

    let mut count = 0;
    while let Some(result) = stream.next().await {
        match result {
            Ok(project) => {
                println!("  Active: {} (cost: ${:.2})", project.name, project.cost);
                count += 1;
            }
            Err(e) => eprintln!("  Error: {}", e),
        }
    }
    println!("  Total: {} active projects", count);
    Ok(())
}

async fn combined_filters(client: &FraiseClient) -> Result<(), Box<dyn std::error::Error>> {
    let mut stream = client
        .query::<Project>("project")
        .where_sql("status='active'")
        .where_rust(|json| {
            // Rust predicates still work with JSON for flexibility
            json["estimated_cost"].as_f64().unwrap_or(0.0) > 10_000.0
        })
        .order_by("name ASC")
        .execute()
        .await?;

    println!("  High-value active projects:");
    while let Some(result) = stream.next().await {
        match result {
            Ok(project) => {
                println!("  - {} (${:.2})", project.name, project.cost);
            }
            Err(e) => eprintln!("  Error: {}", e),
        }
    }
    Ok(())
}

async fn error_handling(client: &FraiseClient) -> Result<(), Box<dyn std::error::Error>> {
    // This will fail if 'price' field doesn't exist
    #[derive(Deserialize)]
    struct InvalidType {
        id: String,
        price: u64, // Might not exist in actual data
    }

    let mut stream = client
        .query::<InvalidType>("project")
        .execute()
        .await?;

    while let Some(result) = stream.next().await {
        match result {
            Ok(_) => println!("  Got project"),
            Err(e) => {
                println!("  Deserialization error: {}", e);
                println!("  This is expected if the field doesn't exist or has wrong type");
            }
        }
    }
    Ok(())
}
```

**Example output:**

```
=== Example 1: Simple typed query ===
  [uuid-1] Project A (active)
  [uuid-2] Project B (completed)
  Total: 2 projects

=== Example 2: Type-safe filtering ===
  Active: Project A (cost: $50000.00)
  Total: 1 active projects

=== Example 3: Combining SQL + Rust filters ===
  High-value active projects:
  - Project A ($50000.00)

=== Example 4: Error handling ===
  Deserialization error for type 'InvalidType':
    missing field `price` at line 1 column 42
  This is expected if the field doesn't exist or has wrong type
```

---

### Phase 8.2.6: Documentation

#### API Documentation

Update rustdoc:

- [ ] `QueryBuilder<T>` - generic query builder
- [ ] `TypedJsonStream<T>` - typed stream struct
- [ ] `Error::Deserialization` - deserialization errors
- [ ] `FraiseClient::query::<T>()` - generic query method

Example rustdoc:

```rust
/// Create a new query builder with automatic deserialization
///
/// The returned `QueryBuilder<T>` will automatically deserialize each row
/// into the specified type `T` using [serde](https://serde.rs/).
///
/// # Type Parameters
///
/// - `T`: Target type implementing [serde::Deserialize]
///
/// # Examples
///
/// ```ignore
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Project {
///     id: String,
///     name: String,
///     status: String,
/// }
///
/// let client = FraiseClient::connect("postgres://localhost/db").await?;
/// let mut stream = client.query::<Project>("projects")
///     .where_sql("status='active'")
///     .execute()
///     .await?;
///
/// while let Some(result) = stream.next().await {
///     let project = result?;
///     println!("Project: {}", project.name);
/// }
/// ```
pub fn query<T: DeserializeOwned>(
    &self,
    entity: impl Into<String>
) -> QueryBuilder<T>
```

#### Guide Document

Create `docs/TYPED_STREAMING.md`:

**Contents**:

1. **Introduction** - What is typed streaming?
2. **Basic Usage** - Simple example with struct
3. **Common Patterns** - Filtering, ordering, predicates
4. **Error Handling** - Understanding deserialization errors
5. **Advanced Types** - Nested structs, Optional, Collections
6. **Custom Serde Attributes** - #[serde(rename)], #[serde(default)], etc
7. **Performance** - Benchmark comparison with JSON approach
8. **FAQ** - Common questions
9. **Troubleshooting** - Common errors and solutions

#### README Update

Add section to README.md:

```markdown
## Typed Streaming (Type-Safe Queries)

fraiseql-wire supports automatic JSON deserialization to strongly-typed Rust structs:

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct Project {
    id: String,
    name: String,
}

// Type-safe streaming
let mut stream = client.query::<Project>("projects").execute().await?;

while let Some(result) = stream.next().await {
    let project = result?;
    println!("Project: {}", project.name);
}
```

See [Typed Streaming Guide](docs/TYPED_STREAMING.md) for details.

```

---

## Implementation Checklist

### Code Changes
- [ ] Make QueryBuilder generic: `QueryBuilder<T: DeserializeOwned>`
- [ ] Add default type parameter: `T = serde_json::Value`
- [ ] Create `TypedJsonStream<T>` struct
- [ ] Implement `Stream` for `TypedJsonStream<T>`
- [ ] Add `Deserialization` error variant
- [ ] Update `FraiseClient::query()` to be generic
- [ ] Ensure PhantomData doesn't affect size
- [ ] All existing APIs preserve generic type

### Tests
- [ ] Basic struct deserialization works
- [ ] Field type mismatches produce clear errors
- [ ] Missing fields produce clear errors
- [ ] Nested types work
- [ ] Optional fields work
- [ ] Collections work
- [ ] SQL filtering with typed stream
- [ ] Rust predicates with typed stream
- [ ] ORDER BY with typed stream
- [ ] Backward compatibility (Value still works)
- [ ] Default type parameter works
- [ ] Type inference works
- [ ] Turbofish syntax works
- [ ] Error messages include type name
- [ ] Error messages include serde details
- [ ] Performance overhead < 2% vs JSON
- [ ] Integration with real Postgres

### Documentation
- [ ] QueryBuilder<T> rustdoc
- [ ] TypedJsonStream<T> rustdoc
- [ ] FraiseClient::query::<T>() rustdoc
- [ ] Example program with comments
- [ ] Typed Streaming Guide (docs/TYPED_STREAMING.md)
- [ ] README update with typed example
- [ ] API documentation examples
- [ ] Common patterns guide
- [ ] Error handling guide
- [ ] Advanced types guide

### Performance
- [ ] Benchmark: Typed vs JSON streaming
- [ ] Benchmark: Deserialization overhead
- [ ] Verify < 2% overhead
- [ ] Memory impact analysis

### Quality
- [ ] > 90% test coverage
- [ ] Zero clippy warnings
- [ ] Format with rustfmt
- [ ] All rustdoc compiles
- [ ] Examples compile and run
- [ ] Backward compatible with v0.1.0

---

## Backward Compatibility Strategy

### Phase 1: Full Backward Compatibility (v0.2.0)

```rust
// Both work identically
let stream1 = client.query::<serde_json::Value>("projects").execute().await?;
let stream2 = client.query("projects").execute().await?;
```

**Default type parameter ensures existing code works without changes.**

### Phase 2: Deprecation Notice (v0.3.0, optional)

Document JSON-only approach as less preferred than typed streaming.

### Phase 3: Future (v1.0+, hypothetical)

Could make T mandatory, but early versions prioritize compatibility.

---

## Architectural Decisions

### 1. Where Does Deserialization Happen?

**Decision**: In `TypedJsonStream::poll_next()` (lazy, per-item)

**Rationale**:

- Deserialization is cheap compared to network I/O
- Per-item deserialization allows backpressure to flow correctly
- Stream stays JSON internally (compatible with FilteredStream)
- Errors are propagated naturally

**Alternative considered**: Deserialize at query builder creation time (rejected - can't know schema until runtime)

### 2. How to Handle Rust Predicates?

**Decision**: Keep predicates JSON-based, applied BEFORE deserialization

**Rationale**:

- Avoids deserializing filtered-out rows (optimization)
- Simpler mental model (filters work on raw data)
- Flexibility to use JSON accessors

**Alternative considered**: Generic Rust predicates for both JSON and T (rejected - too complex)

### 3. Error Representation

**Decision**: Add `Deserialization { type_name, details }` variant to Error enum

**Rationale**:

- Clear error categorization (not protocol/json error)
- Includes type information for debugging
- Includes serde error details

**Alternative considered**: Wrap serde_json::Error (rejected - loses type information)

### 4. Generic vs Separate Method?

**Decision**: Generic `query::<T>()` not separate `query_typed::<T>()`

**Rationale**:

- Cleaner, more idiomatic Rust
- One API rather than two
- Type inference from context
- Turbofish syntax still works

### 5. Performance Approach

**Decision**: Lazy deserialization, aim for < 2% overhead

**Rationale**:

- Streaming model means deserialization is typically not the bottleneck
- Network I/O dominates, crypto is fast
- Only deserialize items that pass filters

---

## Performance Analysis

### Expected Overhead

| Operation | Overhead | Justification |
|-----------|----------|---------------|
| Deserialization | < 1% | Serde is highly optimized |
| PhantomData | 0 bytes | Zero-cost abstraction |
| Poll dispatch | < 0.5% | Single function call |
| **Total** | **< 2%** | Negligible vs network I/O |

### Benchmarks to Measure

```bash
# Compare Stream<Project> vs Stream<Value> with real data
cargo bench --bench typed_streaming

# Expected results:
# - Throughput similar (< 2% variance)
# - Latency similar
# - Memory usage identical
```

---

## Testing Strategy

### Unit Tests (in-memory, no Postgres)

```bash
cargo test --lib typed_streaming
```

Tests:

- Generic type parameter handling
- Deserialization with valid data
- Error construction and messages
- PhantomData size (zero)

### Integration Tests (with real Postgres)

```bash
# Requires Postgres running
cargo test --test typed_streaming_integration

# Or with Docker
docker run -e POSTGRES_HOST_AUTH_METHOD=trust postgres:17-alpine &
cargo test --test typed_streaming_integration
killall postgres
```

Tests:

- Real data deserialization
- Nested types
- Optional fields
- Collections
- Error handling with real data

### Example Verification

```bash
# Run example against test database
POSTGRES_URL=postgres://localhost/testdb cargo run --example typed_streaming
```

---

## Success Criteria

### Functionality ✅

- [ ] `QueryBuilder<T>` generic over type
- [ ] `TypedJsonStream<T>` impl Stream correctly
- [ ] `FraiseClient::query::<T>()` works
- [ ] Deserialization happens per-item
- [ ] All filters preserve generic type
- [ ] Error messages include type info

### Backward Compatibility ✅

- [ ] Default `T = serde_json::Value` works
- [ ] Existing code compiles unchanged
- [ ] `query("entity")` works (no turbofish)
- [ ] All v0.1.0 APIs still work

### Error Handling ✅

- [ ] Type mismatches are clear
- [ ] Missing fields are clear
- [ ] Error messages include type name
- [ ] Error messages include serde details
- [ ] Actionable error messages

### Quality ✅

- [ ] > 90% test coverage
- [ ] Zero clippy warnings
- [ ] Complete rustdoc
- [ ] All examples compile
- [ ] Performance < 2% overhead
- [ ] Backward compatible

### Documentation ✅

- [ ] Full rustdoc
- [ ] Example program
- [ ] Integration guide
- [ ] Common patterns guide
- [ ] Error handling guide
- [ ] FAQ section

---

## Timeline Estimate

### Code Implementation

- Phase 8.2.1: Core type system (2-3 days)
- Phase 8.2.2: Client integration (1 day)
- Phase 8.2.3: Stream enhancement (1 day)
- **Subtotal**: 4-5 days

### Testing

- Unit tests (1-2 days)
- Integration tests (2-3 days)
- Example program (1 day)
- **Subtotal**: 4-6 days

### Documentation

- API documentation (1 day)
- Guide document (2 days)
- README updates (0.5 day)
- **Subtotal**: 3-4 days

### Performance & Review

- Benchmarking (1 day)
- Code review & fixes (2 days)
- **Subtotal**: 3 days

**Total Estimate: 1-2 weeks**

---

## Next Steps (After Planning)

1. **8.2.1**: Refactor QueryBuilder to be generic
2. **8.2.2**: Implement TypedJsonStream
3. **8.2.3**: Update FraiseClient::query()
4. **8.2.4**: Write comprehensive tests
5. **8.2.5**: Create example program
6. **8.2.6**: Write documentation
7. **8.2.7**: Performance benchmarking
8. **8.2.8**: Code review
9. **8.2.9**: PR and merge to main

---

## Related Documentation

- **PHASE_8_PLAN.md** - Overall Phase 8 features
- **phase-8-1-tls-support.md** - TLS implementation (completed)
- **PERFORMANCE_TUNING.md** - Performance guidelines
- **CONTRIBUTING.md** - Development workflow
- **docs/TYPED_STREAMING.md** - User guide (to be created)

---

## Dependency Notes

**No new dependencies required** for basic typed streaming:

- `serde` (already in Cargo.toml)
- `serde_json` (already in Cargo.toml)

Users must provide `#[derive(Deserialize)]` via serde, but this is standard practice.

---

**Ready to proceed with Phase 8.2 implementation! 🚀**
