# Phase 8.2: Typed Streaming - Quick Navigation

**Status**: 📋 Planning Complete, Ready to Start Implementation
**Timeline**: 10-15 days (1-2 weeks)
**Complexity**: Medium
**Effort**: 7 implementation phases

---

## 📖 Documentation Quick Links

### START HERE ↓

**For Planning Overview** (5 min read):
→ `PHASE_8_2_PLANNING_SUMMARY.md`

**For Implementation Details** (comprehensive reference):
→ `.phases/phase-8-2-typed-streaming.md`

**For Session Context** (understanding why):
→ `PHASE_8_2_SESSION_NOTES.md`

---

## 🎯 What is Phase 8.2?

**Add type-safe JSON streaming to fraiseql-wire**

```rust
// Before (current)
let stream = client.query("projects").execute().await?;
while let Some(result) = stream.next().await {
    let json: serde_json::Value = result?;
}

// After (Phase 8.2) - Type T is CONSUMER-SIDE ONLY
let stream = client.query::<Project>("projects")
    .where_sql("status='active'")  // ← Still SQL, unaffected by T
    .execute()
    .await?;
while let Some(result) = stream.next().await {
    let project: Project = result?;  // Type-safe at poll_next() only!
}

// Escape hatch always available
let stream = client.query::<serde_json::Value>("projects").execute().await?;
```

### ⚠️ Critical Design Constraint

**Type T affects ONLY:**

- Consumer-side deserialization at `poll_next()`
- Error messages (type name included)

**Type T does NOT affect:**

- SQL generation
- Filtering (where_sql, where_rust)
- Ordering (ORDER BY)
- Wire protocol
- Chunking, cancellation, backpressure

---

## 🏗️ Architecture at a Glance

```
┌──────────────────────┐
│   FraiseClient       │
│  query::<T>()        │ ← Generic method
└──────────────────────┘
         ↓
┌──────────────────────┐
│  QueryBuilder<T>     │ ← Generic builder
│  - where_sql()       │
│  - where_rust()      │
│  - order_by()        │
│  - chunk_size()      │
│  - execute()         │
└──────────────────────┘
         ↓
┌──────────────────────┐
│ TypedJsonStream<T>   │ ← New stream type
│ - Deserializes to T  │
│ - Per-item (lazy)    │
└──────────────────────┘
```

---

## 📋 Implementation Phases

### Phase 8.2.1: Core Type System (2-3 days)

**What**: Generic QueryBuilder + TypedJsonStream
**Files**:

- `src/client/query_builder.rs` (refactor to generic)
- `src/stream/typed_stream.rs` (new)
- `src/error.rs` (add error variant)

**Key Tasks**:

- [ ] Make QueryBuilder generic: `QueryBuilder<T>`
- [ ] Add default type param: `T = serde_json::Value`
- [ ] Implement TypedJsonStream<T>
- [ ] Add Error::Deserialization variant

### Phase 8.2.2: Client Integration (1 day)

**What**: Update FraiseClient API
**Files**:

- `src/client/fraise_client.rs`
- `src/stream/mod.rs`
- `src/lib.rs`

**Key Tasks**:

- [ ] Make query() generic
- [ ] Support turbofish syntax
- [ ] Export TypedJsonStream

### Phase 8.2.3: Stream Enhancement (1 day)

**What**: Verify pipeline compatibility
**Files**:

- `src/stream/filter.rs` (verify)

**Key Tasks**:

- [ ] Ensure FilteredStream works with TypedJsonStream
- [ ] Verify filtering before deserialization

### Phase 8.2.4: Comprehensive Tests (2-3 days)

**What**: Unit + integration tests
**Files**:

- `tests/typed_streaming_integration.rs` (new)

**Test Categories**:

- [ ] Basic struct deserialization
- [ ] Type mismatches & errors
- [ ] Missing fields
- [ ] Nested types
- [ ] Optional/Collection fields
- [ ] SQL + Rust filtering
- [ ] ORDER BY with types
- [ ] Backward compatibility
- [ ] Error messages

### Phase 8.2.5: Example Program (1 day)

**What**: User-facing example
**Files**:

- `examples/typed_streaming.rs` (new)

**Examples**:

- [ ] Simple typed query
- [ ] With SQL filtering
- [ ] With Rust predicates
- [ ] Error handling

### Phase 8.2.6: Documentation (2-3 days)

**What**: Complete API documentation
**Files**:

- `docs/TYPED_STREAMING.md` (new)
- `README.md` (update)
- Rustdoc in source code

**Documentation**:

- [ ] User guide
- [ ] Common patterns
- [ ] Advanced types
- [ ] Error handling
- [ ] FAQ
- [ ] API rustdoc

### Phase 8.2.7: Performance & QA (1-2 days)

**What**: Benchmarking and review
**Files**:

- `benches/typed_streaming.rs` (new, optional)

**Quality Gates**:

- [ ] Benchmark < 2% overhead
- [ ] 90%+ test coverage
- [ ] Zero clippy warnings
- [ ] Complete rustdoc
- [ ] All examples compile

---

## 🔍 Key Design Decisions

| Decision | Choice | Why |
|----------|--------|-----|
| Generic vs method | Generic `query::<T>()` | Idiomatic, cleaner |
| Deserialization | Lazy per-item | Skip filtered rows |
| Predicates | JSON-based | Simpler, flexible |
| Errors | Deserialization variant | Type info included |
| Compatibility | Default type param | Zero breaking changes |

---

## ✅ Success Criteria

### Functionality

- [ ] Generic `query::<T>()` works
- [ ] All APIs preserve generic type
- [ ] Deserialization is lazy (per-item)
- [ ] Error messages are clear
- [ ] Backward compatible (Value still works)

### Quality

- [ ] > 90% test coverage
- [ ] Zero clippy warnings
- [ ] Complete rustdoc
- [ ] All examples compile
- [ ] < 2% performance overhead

### Documentation

- [ ] Full API documentation
- [ ] User guide
- [ ] Example program
- [ ] README section
- [ ] FAQ/Troubleshooting

---

## 📅 Timeline

| Phase | Days | Cumulative |
|-------|------|-----------|
| 8.2.1 | 2-3  | 2-3 days |
| 8.2.2 | 1    | 3-4 days |
| 8.2.3 | 1    | 4-5 days |
| 8.2.4 | 2-3  | 6-8 days |
| 8.2.5 | 1    | 7-9 days |
| 8.2.6 | 2-3  | 9-12 days |
| 8.2.7 | 1-2  | 10-14 days |
| **Total** | **10-15** | **1-2 weeks** |

---

## 🚀 Implementation Path

### Day 1-3: Core Type System

1. Read `.phases/phase-8-2-typed-streaming.md` (Phase 8.2.1 section)
2. Refactor QueryBuilder<T>
3. Implement TypedJsonStream<T>
4. Add error variant
5. Basic unit tests

### Day 4-5: Integration

1. Update FraiseClient API
2. Implement Phase 8.2.2-3
3. Verify pipeline
4. Integration tests

### Day 6-8: Testing

1. Comprehensive test suite
2. Test all scenarios
3. Achieve 90%+ coverage

### Day 9-10: Examples & Docs

1. Create example program
2. Write user guide
3. API documentation
4. README updates

### Day 11-14: Performance & QA

1. Benchmarking
2. Code review
3. Final cleanup
4. Prepare for merge

---

## 📚 Documentation Files

### Planning Documents

- `.phases/phase-8-2-typed-streaming.md` - **550 lines, comprehensive implementation plan**
- `PHASE_8_2_PLANNING_SUMMARY.md` - **350 lines, executive summary**
- `PHASE_8_2_SESSION_NOTES.md` - **400 lines, session context**

### To Be Created

- `docs/TYPED_STREAMING.md` - User guide
- `examples/typed_streaming.rs` - Example program
- Source code rustdoc - Full API documentation

---

## 💡 Key Implementation Notes

### Generic Type System

```rust
// Default type parameter for backward compat
pub struct QueryBuilder<T: DeserializeOwned = serde_json::Value> {
    // ...
}

// All methods preserve type
pub fn where_sql(mut self, predicate: impl Into<String>) -> Self { ... }
```

### TypedJsonStream Design

```rust
pub struct TypedJsonStream<T: DeserializeOwned> {
    inner: Box<dyn Stream<Item = Result<Value>> + Unpin>,
    _phantom: std::marker::PhantomData<T>,
}

// Deserialization happens here (lazy)
impl<T: DeserializeOwned> Stream for TypedJsonStream<T> {
    fn poll_next(...) -> Poll<Option<Result<T>>> {
        // Deserialize Value → T
    }
}
```

### Rust Predicates (Unchanged)

```rust
// Still work with JSON values, applied before deserialization
.where_rust(|json| json["cost"].as_f64().unwrap_or(0.0) > 10_000.0)
```

---

## ⚠️ Critical Pitfalls to Avoid

### 1. **Don't Add "Typed Query Planning"**

❌ WRONG: Use T to optimize SQL generation

```rust
// BAD: Typing affects SQL
if T::SOME_CONST { ... different SQL ... }
```

✅ RIGHT: Type is consumer-side only

```rust
// Type parameter has ZERO impact on SQL generation
pub async fn execute(self) -> Result<Box<dyn Stream<Item = Result<T>> + Unpin>> {
    let sql = self.build_sql();  // ← Same SQL regardless of T
    // ...
}
```

### 2. **Don't Make Predicates Generic**

❌ WRONG: Generic Rust predicates

```rust
pub fn where_rust<F, T>(mut self, predicate: F) -> Self
where
    F: Fn(&T) -> bool  // WRONG: operates on T
```

✅ RIGHT: JSON-based predicates

```rust
pub fn where_rust<F>(mut self, predicate: F) -> Self
where
    F: Fn(&serde_json::Value) -> bool  // RIGHT: operates on JSON
```

### 3. **Don't Deserialize Before Filtering**

❌ WRONG: Deserialize all rows

```rust
fn poll_next(...) -> Poll<Option<Result<T>>> {
    let json = next_json();
    let item = serde_json::from_value::<T>(json)?;  // WRONG: deserialize first
    if predicate(&item) { ... }
}
```

✅ RIGHT: Filter then deserialize

```rust
fn poll_next(...) -> Poll<Option<Result<T>>> {
    loop {
        let json = next_json();
        if (self.rust_predicate)(&json) {  // RIGHT: filter JSON first
            let item = serde_json::from_value::<T>(json)?;
            return Poll::Ready(Some(Ok(item)));
        }
    }
}
```

### 4. **Don't Lose Type Info in Errors**

❌ WRONG: Generic error messages

```rust
Err(Error::Json(e))  // WRONG: Lost type information
```

✅ RIGHT: Include type name

```rust
Err(Error::Deserialization {
    type_name: std::any::type_name::<T>(),
    details: e.to_string(),
})
```

### 5. **Don't Break the Escape Hatch**

❌ WRONG: Make Value special-cased

```rust
if T == serde_json::Value { ... special handling ... }
```

✅ RIGHT: Value is first-class, no special handling

```rust
// query::<Value>() works identically to any other type
// No special cases, no optimization for Value
```

### 6. **Don't Forget to Document This**

❌ WRONG: Unclear API documentation

```rust
pub fn query<T>(...) -> QueryBuilder<T> { ... }
```

✅ RIGHT: Explicit boundary documentation

```rust
/// Type T controls consumer-side deserialization at poll_next() ONLY.
///
/// Type T does NOT affect:
/// - SQL generation (still `SELECT data FROM v_{entity}`)
/// - Filtering (where_sql, where_rust, order_by unchanged)
/// - Wire protocol (identical for all T)
/// - Performance (< 2% overhead, mostly serde)
///
/// Escape hatch (debugging, forward-compat):
/// ```ignore
/// client.query::<serde_json::Value>("entity").execute().await?
/// ```
pub fn query<T: DeserializeOwned>(
    &self,
    entity: impl Into<String>
) -> QueryBuilder<T>
```

---

## 🔗 Related Features

| Phase | Status | Impact |
|-------|--------|--------|
| 8.1: TLS | ✅ Complete | Independent |
| 8.2: Typed | 📋 This phase | Type safety |
| 8.3: Config | 📋 Planned | Connection options |
| 8.5: Metrics | 📋 Planned | Observability |

---

## 📖 How to Use These Documents

### I want a quick overview

→ Read `PHASE_8_2_PLANNING_SUMMARY.md` (10 min)

### I want to understand the design

→ Read `PHASE_8_2_SESSION_NOTES.md` (15 min)

### I want to implement Phase 8.2

→ Read `.phases/phase-8-2-typed-streaming.md` (detailed reference)

### I want specific details on Phase 8.2.1

→ Jump to "Phase 8.2.1: Core Type System" in `.phases/phase-8-2-typed-streaming.md`

### I want to see example code

→ Look for code blocks in `.phases/phase-8-2-typed-streaming.md`

---

## ✨ Key Features

- ✅ **Generic API** - `client.query::<T>()` with auto deserialization
- ✅ **Backward compatible** - Default type param `T = Value`
- ✅ **All filters work** - where_sql, where_rust, order_by preserved
- ✅ **Clear errors** - Type name + serde details in error messages
- ✅ **Lazy deserialization** - Per-item, skip filtered rows
- ✅ **Zero-copy** - Where serde_json supports it
- ✅ **Performance** - < 2% overhead vs JSON approach
- ✅ **No new deps** - Uses existing serde + serde_json

---

## 🎯 Success Definition

Phase 8.2 is **complete and successful** when:

1. ✅ `query::<T>()` works for any `T: DeserializeOwned`
2. ✅ All APIs (where_sql, where_rust, order_by, chunk_size) preserve type
3. ✅ Deserialization errors are clear and actionable
4. ✅ Backward compatibility is maintained (Value still works)
5. ✅ > 90% test coverage achieved
6. ✅ < 2% performance overhead verified
7. ✅ Complete API documentation provided
8. ✅ User guide and examples available
9. ✅ Zero clippy warnings
10. ✅ All examples compile and run

---

## 🚀 Ready to Start?

1. **Understand the design**: Read `PHASE_8_2_PLANNING_SUMMARY.md`
2. **Dive into details**: Start with `Phase 8.2.1` in `.phases/phase-8-2-typed-streaming.md`
3. **Begin coding**: Follow the 7-phase implementation plan
4. **Reference often**: Keep `.phases/phase-8-2-typed-streaming.md` open as detailed guide

---

**Status**: ✅ Fully Planned
**Next Step**: Begin Phase 8.2.1 Implementation
**Questions**: Reference the detailed plan in `.phases/phase-8-2-typed-streaming.md`

🚀 **Ready to proceed with implementation!**
