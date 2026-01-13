# Phase 8.2: Typed Streaming - Planning Summary

**Date**: 2026-01-13
**Status**: 🚀 Ready to Start Implementation
**Effort**: 1-2 weeks (Medium complexity)
**Priority**: 🟡 Nice-to-have for v0.2.0 (after TLS/Config)

---

## Overview

Phase 8.2 adds **generic, type-safe JSON streaming** to fraiseql-wire, enabling automatic deserialization of rows into user-defined types.

### Before (Current API)
```rust
let mut stream = client.query("projects").execute().await?;

while let Some(result) = stream.next().await {
    let json: serde_json::Value = result?;
    let name = json["name"].as_str();  // Manual extraction
}
```

### After (Typed API)
```rust
#[derive(Deserialize)]
struct Project {
    id: String,
    name: String,
}

// Type T is CONSUMER-SIDE ONLY: affects deserialization at poll_next()
// SQL, filtering, ordering, wire protocol are IDENTICAL
let mut stream = client.query::<Project>("projects")
    .where_sql("status='active'")  // ← Still SQL, unaffected by T
    .execute()
    .await?;

while let Some(result) = stream.next().await {
    let project: Project = result?;  // Type-safe deserialization
    println!("Project: {}", project.name);
}

// Escape hatch: Always available, identical to untyped
let raw_stream = client.query::<serde_json::Value>("projects").execute().await?;
```

---

## Key Features

✅ **Generic query builder** - `client.query::<T>()` with automatic deserialization
✅ **Backward compatible** - Default type parameter `T = serde_json::Value`
✅ **All filters work** - SQL predicates, Rust predicates, ORDER BY all preserved
✅ **Clear errors** - Deserialization errors include type name and serde details
✅ **Zero-copy** - JSON parsing happens per-item, filters applied before deserialization
✅ **Performance** - < 2% overhead vs current JSON approach

---

## ⚠️ CRITICAL DESIGN CONSTRAINT

### Typing is Consumer-Side Only

**Type T does NOT affect:**
- SQL generation (still `SELECT data FROM v_{entity}`)
- Filtering (where_sql, where_rust, order_by unchanged)
- Ordering (ORDER BY is identical)
- Wire protocol (network communication identical)
- Chunking, cancellation, backpressure

**Type T ONLY affects:**
- Consumer-side deserialization at `poll_next()`
- Error messages (type name included for debugging)

### Escape Hatch is First-Class Feature

```rust
// Always supported, always identical to untyped:
let stream = client.query::<serde_json::Value>("projects").execute().await?;

// Use cases:
// 1. Debugging actual JSON structure
// 2. Forward compatibility without code changes
// 3. Generic ops workflows (any entity handler)
// 4. Partial opt-out from type safety
```

This prevents future contributors from accidentally adding "typed query planning" that violates the core design principle: **one query family, minimal scope**.

---

## Architecture Summary

### Type System
- **QueryBuilder<T>** - Generic over `DeserializeOwned`
- **TypedJsonStream<T>** - Wraps `Box<dyn Stream<Item = Result<Value>>>`, deserializes to T
- **Error::Deserialization** - New error variant with type info

### Processing Pipeline
```
Raw Data → JsonStream → FilteredStream (optional) → TypedJsonStream<T> → User Code
                        ↑ filters on JSON        ↑ deserializes to T
                        (before deserialization)
```

### Rust Predicates
- Still operate on JSON Values
- Applied BEFORE deserialization (optimization)
- Avoids deserializing filtered-out rows

---

## Implementation Phases

### Phase 8.2.1: Core Type System (2-3 days)
- Refactor QueryBuilder to be generic: `QueryBuilder<T: DeserializeOwned>`
- Implement TypedJsonStream<T>
- Add Error::Deserialization variant
- All chainable APIs preserve generic type

### Phase 8.2.2: Client Integration (1 day)
- Update FraiseClient::query() to be generic
- Ensure PhantomData doesn't add size
- Support turbofish syntax and type inference

### Phase 8.2.3: Stream Enhancement (1 day)
- Verify FilteredStream works with typed streams
- Ensure filtering happens before deserialization
- Minimal changes needed (backward compatible)

### Phase 8.2.4: Comprehensive Tests (2-3 days)
- Basic struct deserialization
- Field type mismatches
- Missing fields
- Nested types, Optional fields, Collections
- SQL filtering + Rust predicates + ORDER BY
- Backward compatibility (Value still works)
- Error messages include type names

### Phase 8.2.5: Example Program (1 day)
- `examples/typed_streaming.rs`
- Shows: simple query, filtering, combined filters, error handling
- Demonstrates best practices

### Phase 8.2.6: Documentation (2-3 days)
- API rustdoc for QueryBuilder<T>, TypedJsonStream<T>, Error variants
- User guide: `docs/TYPED_STREAMING.md`
- README section with typed example
- FAQ and troubleshooting

### Phase 8.2.7: Performance & Quality (1-2 days)
- Benchmarking: Typed vs JSON streaming
- Verify < 2% overhead
- Code review and cleanup
- 90%+ test coverage, zero clippy warnings

---

## Code Changes Required

### New Files
- `src/stream/typed_stream.rs` - TypedJsonStream implementation

### Modified Files
- `src/client/query_builder.rs` - Make generic, add default type parameter
- `src/error.rs` - Add Deserialization variant
- `src/client/fraise_client.rs` - Make query() generic
- `src/stream/mod.rs` - Export TypedJsonStream
- `src/lib.rs` - Update public API

### New Documentation
- `.phases/phase-8-2-typed-streaming.md` - Detailed implementation plan (created ✅)
- `docs/TYPED_STREAMING.md` - User guide
- `examples/typed_streaming.rs` - Example program

---

## Success Criteria

### Functionality
- [ ] `query::<T>()` works with any type implementing Deserialize
- [ ] All APIs (where_sql, where_rust, order_by, chunk_size) preserve generic type
- [ ] Deserialization errors are clear and actionable
- [ ] Backward compatibility maintained (Value still works)

### Quality
- [ ] > 90% test coverage
- [ ] Zero clippy warnings
- [ ] Complete rustdoc
- [ ] All examples compile and run
- [ ] Performance < 2% overhead

### Performance
- [ ] TypedJsonStream vs JsonStream: < 2% latency difference
- [ ] Deserialization per-item (lazy)
- [ ] Memory impact negligible

### Documentation
- [ ] Full API documentation
- [ ] Example program with comments
- [ ] User guide with patterns
- [ ] README update
- [ ] FAQ section

---

## Timeline Estimate

| Phase | Duration | Notes |
|-------|----------|-------|
| 8.2.1 | 2-3 days | Core type system |
| 8.2.2 | 1 day | Client integration |
| 8.2.3 | 1 day | Stream enhancement |
| 8.2.4 | 2-3 days | Comprehensive tests |
| 8.2.5 | 1 day | Example program |
| 8.2.6 | 2-3 days | Documentation |
| 8.2.7 | 1-2 days | Performance & QA |
| **Total** | **10-15 days** | **1-2 weeks** |

---

## Risk & Mitigation

| Risk | Impact | Mitigation |
|------|--------|-----------|
| Generic bounds too complex | Medium | Careful API design, comprehensive testing |
| Deserialization overhead | Low | Should be < 1% overhead, benchmark to verify |
| Error messages unclear | Medium | Include type name and serde details in error |
| Breaking backward compat | High | Default type parameter preserves existing API |
| Performance regression | Medium | Benchmark before/after, set < 2% threshold |

---

## Dependencies

**No new dependencies** - uses existing:
- `serde` (already in Cargo.toml)
- `serde_json` (already in Cargo.toml)
- `futures` (already in Cargo.toml)

Users must add `serde` derive to their types, which is standard practice.

---

## Relationship to Other Features

### Phase 8.1: TLS Support (Completed ✅)
- Independent feature
- No interaction with typed streaming
- Both can coexist

### Phase 8.3: Connection Config (Planned)
- Independent feature
- No interaction with typed streaming

### Phase 8.5: Query Metrics (Planned)
- Can work together
- Metrics collected regardless of stream type

---

## Design Decisions

### 1. Generic vs Separate Method ✅
**Chosen**: Generic `query::<T>()` not separate `query_typed::<T>()`
- Cleaner, idiomatic Rust
- One API rather than two
- Type inference works from context

### 2. Deserialization Timing ✅
**Chosen**: Lazy (per-item in poll_next)
- Efficient: don't deserialize filtered rows
- Natural error propagation
- Works with existing stream architecture

### 3. Rust Predicates Approach ✅
**Chosen**: Keep JSON-based, applied before deserialization
- Simpler mental model
- Optimization: skip deserializing filtered rows
- Flexibility: use JSON accessors if needed

### 4. Error Representation ✅
**Chosen**: New Error::Deserialization variant with type_name + details
- Clear error categorization
- Includes serde error details
- Type name aids debugging

---

## Example Usage

### Simple Typed Query
```rust
#[derive(Deserialize)]
struct Project { id: String, name: String }

let mut stream = client.query::<Project>("projects").execute().await?;
while let Some(result) = stream.next().await {
    let project = result?;
    println!("Project: {}", project.name);
}
```

### With Filtering
```rust
let mut stream = client
    .query::<Project>("projects")
    .where_sql("status='active'")
    .where_rust(|json| json["cost"].as_f64().unwrap_or(0.0) > 10_000.0)
    .order_by("name ASC")
    .execute()
    .await?;
```

### Error Handling
```rust
while let Some(result) = stream.next().await {
    match result {
        Ok(project) => println!("Project: {}", project.name),
        Err(e) => eprintln!("Error deserializing Project: {}", e),
    }
}
```

---

## Testing Strategy

### Unit Tests
- Generic type parameter handling
- Error construction with type info
- PhantomData has zero size

### Integration Tests
- Real Postgres with custom types
- Nested structs, Optional, Collections
- Filtering + typing
- Error messages with real data

### Benchmarks
- Typed vs JSON throughput
- Deserialization overhead measurement
- Memory impact analysis

---

## Documentation Plan

### API Documentation (rustdoc)
- QueryBuilder<T>
- TypedJsonStream<T>
- Error::Deserialization
- FraiseClient::query::<T>()

### User Guide
- Introduction to typed streaming
- Common patterns (filtering, ordering, error handling)
- Advanced types (nested, optional, collections)
- Performance considerations
- FAQ and troubleshooting

### Examples
- Basic typed query
- SQL + Rust filtering
- Nested types
- Error handling

---

## Next Steps

1. **Code Implementation** (Days 1-5)
   - Phase 8.2.1: Core type system
   - Phase 8.2.2: Client integration
   - Phase 8.2.3: Stream enhancement

2. **Testing** (Days 6-8)
   - Phase 8.2.4: Comprehensive tests
   - Achieve 90%+ coverage

3. **Documentation & Examples** (Days 9-11)
   - Phase 8.2.5: Example program
   - Phase 8.2.6: Documentation

4. **Performance & QA** (Days 12-14)
   - Phase 8.2.7: Benchmarking
   - Code review
   - Final cleanup

5. **Merge & Release**
   - PR to main
   - v0.2.0 release notes
   - Announce typed streaming feature

---

## Phase 8.2 vs Other Priority Features

| Feature | Priority | Effort | Status |
|---------|----------|--------|--------|
| Phase 8.1: TLS | 🔴 Critical | 1-2 weeks | ✅ Complete |
| Phase 8.3: Config | 🟢 High | 3-5 days | 📋 Planned |
| Phase 8.2: Typed | 🟡 Medium | 1-2 weeks | 📋 Ready |
| Phase 8.5: Metrics | 🟢 High | 1 week | 📋 Planned |
| Phase 8.4: SCRAM | 🟡 Medium | 2 weeks | ⏳ If needed |
| Phase 8.6: Pooling | 🟡 Medium | 4-6 weeks | 📅 Defer to separate crate |

**Recommendation**: Start Phase 8.3 (Config) first as quick win, then 8.2 (Typed) for type safety.

---

## Related Documentation

- **Detailed Plan**: `.phases/phase-8-2-typed-streaming.md` (created)
- **Phase 8 Overview**: `PHASE_8_PLAN.md`
- **Phase 8.1 Plan**: `.phases/phase-8-1-tls-support.md`
- **Architecture**: `.claude/CLAUDE.md`
- **Performance**: `PERFORMANCE_TUNING.md`

---

**Status**: ✅ Planning Complete, Ready for Implementation
**Next Action**: Begin Phase 8.2.1 - Core Type System Implementation
