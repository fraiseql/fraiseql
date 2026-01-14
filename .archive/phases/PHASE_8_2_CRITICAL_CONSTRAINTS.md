# Phase 8.2: Critical Design Constraints

**Purpose**: Prevent future drift from core design principle
**Status**: ✅ Added to all planning documents
**Enforcement**: Must be in all code comments, rustdoc, and PR reviews

---

## The Core Issue Being Prevented

Without explicit boundaries, future contributors might think:

> "Typed streaming means we can use type information to optimize SQL, filtering, or ordering"

This would violate fraiseql-wire's fundamental design:
- **One query family**: `SELECT data FROM v_{entity} WHERE predicate [ORDER BY ...]`
- **Minimal scope**: JSON streaming, nothing more
- **Zero "query planning"**: Type information never affects SQL generation or execution strategy

---

## Rule 1: Typing is Consumer-Side Only

### What Type T Does NOT Affect

✅ **SQL generation is identical**
```rust
// For ANY type T, this generates the same SQL:
client.query::<T>("entity").where_sql(...).execute()
// Result: SELECT data FROM v_entity WHERE ...
```

✅ **Filtering is identical**
```rust
// where_sql() - Still SQL, unaffected by T
// where_rust() - Still operates on serde_json::Value, unaffected by T
// ORDER BY - Still identical SQL regardless of T
```

✅ **Wire protocol is identical**
```rust
// Same row format, same chunking, same cancellation
// Whether T is Project or Value: network packets are identical
```

✅ **Performance characteristics are identical**
```rust
// < 2% overhead from serde deserialization, that's it
// No "optimized" path for typed queries
```

### What Type T ONLY Affects

✅ **Consumer-side deserialization**
```rust
fn poll_next(...) -> Poll<Option<Result<T>>> {
    match self.inner.poll_next_unpin(cx) {
        Poll::Ready(Some(Ok(value))) => {
            // Type T is resolved HERE, at poll_next
            let item: T = serde_json::from_value(value)?;
            Poll::Ready(Some(Ok(item)))
        }
        // ...
    }
}
```

✅ **Error messages**
```rust
Error::Deserialization {
    type_name: std::any::type_name::<T>(),  // ← T used here for debugging
    details: serde_error.to_string(),
}
```

---

## Rule 2: Escape Hatch is First-Class

### Always Support This Pattern

```rust
// This MUST be supported and MUST work identically to untyped queries:
let stream = client.query::<serde_json::Value>("entity").execute().await?;
```

### Why This Matters

1. **Debugging**: Inspect actual JSON structure
   ```rust
   let stream = client.query::<Value>("projects").execute().await?;
   while let Some(result) = stream.next().await {
       println!("Raw: {:?}", result?);  // See what's actually returned
   }
   ```

2. **Forward Compatibility**: Change types without changing code
   ```rust
   // Old: client.query::<Project>("projects")
   // New: client.query::<Value>("projects")  // No code changes needed
   ```

3. **Operations Workflows**: Generic handlers
   ```rust
   async fn export_entity(entity: &str) -> Result<()> {
       let mut stream = client.query::<Value>(entity).execute().await?;
       while let Some(result) = stream.next().await {
           println!("{}", serde_json::to_string(&result?)?);
       }
   }
   ```

4. **Partial Type Safety**: Opt-out from specific types
   ```rust
   let stream = client.query::<Value>("projects").execute().await?;
   // Extract and validate manually if needed
   ```

### Implementation Guarantee

- **No special cases** for `serde_json::Value`
- **No optimization** when T == Value
- **No conditional logic** based on type T
- **Identical behavior** for all T, including Value

---

## Critical Anti-Patterns: What to Forbid in Code Review

### ❌ Anti-Pattern 1: Type-Based SQL Generation

```rust
// REJECT THIS IN CODE REVIEW
fn execute(self) -> Result<...> {
    let mut sql = self.build_sql();

    // DON'T DO THIS - Type T should NOT affect SQL
    if T::needs_special_handling {
        sql.push_str(" /* optimized for Project */");
    }

    self.client.execute_query(&sql, ...)
}
```

**Correct approach**: SQL is identical regardless of T
```rust
fn execute(self) -> Result<...> {
    let sql = self.build_sql();  // Same for all T
    self.client.execute_query(&sql, ...)
}
```

### ❌ Anti-Pattern 2: Generic Rust Predicates

```rust
// REJECT THIS IN CODE REVIEW
pub fn where_rust<F: Fn(&T) -> bool>(self, pred: F) -> Self {
    // DON'T DO THIS - Predicate operates on T, not JSON
}
```

**Correct approach**: Predicates operate on JSON
```rust
pub fn where_rust<F: Fn(&Value) -> bool>(self, pred: F) -> Self {
    // RIGHT - Predicate receives JSON Value, regardless of T
}
```

### ❌ Anti-Pattern 3: Deserialize Before Filtering

```rust
// REJECT THIS IN CODE REVIEW
fn poll_next(...) -> Poll<Option<Result<T>>> {
    let json = next_json();
    let item = serde_json::from_value::<T>(json)?;  // WRONG

    // DON'T DO THIS - Deserialize after filtering, not before
    if (self.predicate)(&item) {
        return Poll::Ready(Some(Ok(item)));
    }
}
```

**Correct approach**: Filter JSON, then deserialize
```rust
fn poll_next(...) -> Poll<Option<Result<T>>> {
    loop {
        let json = next_json();
        if (self.predicate)(&json) {  // Filter JSON first
            let item = serde_json::from_value::<T>(json)?;  // Deserialize after
            return Poll::Ready(Some(Ok(item)));
        }
    }
}
```

### ❌ Anti-Pattern 4: Lose Type Info in Errors

```rust
// REJECT THIS IN CODE REVIEW
fn deserialize_item(value: Value) -> Result<T> {
    serde_json::from_value(value)
        .map_err(Error::Json)  // WRONG - Lost type name
}
```

**Correct approach**: Include type name
```rust
fn deserialize_item(value: Value) -> Result<T> {
    serde_json::from_value(value)
        .map_err(|e| Error::Deserialization {
            type_name: std::any::type_name::<T>().to_string(),
            details: e.to_string(),
        })
}
```

### ❌ Anti-Pattern 5: Special-Case Value Type

```rust
// REJECT THIS IN CODE REVIEW
fn execute(self) -> Result<...> {
    // DON'T DO THIS - Value is not special
    if T::is_value {
        return self.execute_untyped();  // Special path
    }

    self.execute_typed()
}
```

**Correct approach**: Single code path for all T
```rust
fn execute(self) -> Result<...> {
    // RIGHT - No special cases, Value is treated like any other type
    let sql = self.build_sql();
    self.client.execute_query(&sql, self.chunk_size).await?
        .map(|stream| Box::new(TypedJsonStream::<T>::new(stream)))
}
```

---

## PR Review Checklist

When reviewing Phase 8.2 implementation and future changes:

### Before Merge

- [ ] **Type isolation verified**: SQL is identical for all T
- [ ] **No "typed query planning"**: No conditionals based on T
- [ ] **Predicates are JSON-based**: `where_rust` takes `&Value`
- [ ] **Filtering before deserialization**: Correct pipeline order
- [ ] **Type names in errors**: Error messages include `std::any::type_name::<T>()`
- [ ] **Escape hatch tested**: `query::<Value>()` works identically
- [ ] **No special cases for Value**: Value is treated like any other type
- [ ] **Documentation explicit**: All relevant rustdoc mentions boundary constraints
- [ ] **Comments in code**: Key locations have constraint documentation

### Example Comment to Add

```rust
// Type parameter T is **consumer-side only**. This means:
// - T does NOT affect SQL generation (always SELECT data FROM v_entity)
// - T does NOT affect filtering (where_sql, where_rust, order_by)
// - T does NOT affect wire protocol (identical packets)
// - T ONLY affects deserialization at poll_next()
//
// This is critical to maintain fraiseql-wire's design principle:
// "one query family, minimal scope, no typed query planning"
```

---

## Future-Proofing

### If Someone Proposes "Optimized Query Planning"

```
"We could use type T to optimize the SQL for Project queries"

Response: That violates the fundamental design constraint.
Typing is consumer-side only. Close the PR.
```

### If Someone Proposes "Generic Filtering"

```
"Let's make where_rust generic over T for type safety"

Response: Rust predicates operate on JSON, not T.
This prevents deserializing filtered-out rows.
Revert the change.
```

### If Someone Proposes "Special Value Handling"

```
"We should optimize query::<Value>() since it's untyped"

Response: Value is first-class, no special cases.
Same code path for all T.
Revert the change.
```

---

## Documentation Requirements

### Every Code File Must Include

```rust
/// Type parameter T is **consumer-side only**.
///
/// The type T does NOT affect:
/// - SQL generation (always `SELECT data FROM v_{entity}`)
/// - Filtering (where_sql, where_rust, order_by)
/// - Wire protocol (identical for all T)
/// - Memory characteristics (same as Value)
///
/// Type T ONLY affects:
/// - Consumer-side deserialization at poll_next()
/// - Error messages (type name included)
///
/// This design prevents "typed query planning" which would violate
/// fraiseql-wire's fundamental principle: minimal scope, one query family.
```

### User-Facing Documentation Must Include

```markdown
## Important: Typing is Consumer-Side Only

The generic type parameter `T` affects **only** how rows are deserialized
when consumed. It does **NOT** affect:

- The SQL query generated
- Filtering (where_sql, where_rust, order_by)
- The wire protocol
- Network communication

This ensures type-safe queries remain a consumer convenience, not a SQL
optimization mechanism.

### Escape Hatch (Always Available)

For debugging or forward compatibility, use raw JSON:

```rust
let stream = client.query::<serde_json::Value>("entity").execute().await?;
```

This is identical to any typed query - no special handling, same SQL,
same filtering, same performance.
```

---

## Rationale: Why This Boundary Matters

fraiseql-wire's value proposition is **simplicity**:
- One query shape (WHERE + ORDER BY only)
- Minimal protocol (Simple Query only)
- Predictable performance (no query planning)
- Bounded memory (streaming architecture)

**Typed streaming must NOT become a slippery slope toward:**
- "Optimized queries for specific types"
- "Conditional SQL based on type information"
- "Query planning based on T"

Once you cross that line, you've transformed fraiseql-wire into a general-purpose driver. The next person wants "SELECT a, b FROM table", then "JOIN support", then "aggregation", then "prepared statements"...

**Explicit boundaries prevent this drift.**

---

## Success Metric

Phase 8.2 is successful when:

✅ Type-safe streaming works
✅ All existing code works unchanged
✅ Error messages are clear
✅ Performance is < 2% overhead
✅ **AND**: Every code reviewer can point to the constraint documentation

Code review checklist is the enforcement mechanism.

---

**Approval Gate**: This document must be referenced in every Phase 8.2 PR review.
