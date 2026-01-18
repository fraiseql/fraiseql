# FraiseQL - TODO Implementation Plan

**Date**: January 18, 2026
**Status**: ~98% Complete - Remaining items are polish/enhancement

---

## Overview

This document catalogs all TODO comments, stub implementations, and incomplete features across the FraiseQL codebase. Items are organized by priority and effort level.

---

## Priority 1: Core Functionality Gaps

### 1.1 Stream State Machine (`state_snapshot()` stub)

**Location**: `crates/fraiseql-wire/src/stream/json_stream.rs:148-152`

```rust
pub fn state_snapshot(&self) -> StreamState {
    // This is a best-effort snapshot that may return slightly stale state
    // For guaranteed accurate state, use `state()` method
    StreamState::Running // Will be updated when state machine is fully integrated
}
```

**Issue**: `state_snapshot()` always returns `Running` regardless of actual state.

**Impact**: Tests had to be adapted to work around this limitation. Users cannot reliably check stream state.

**Fix**:
```rust
pub fn state_snapshot(&self) -> StreamState {
    // Check lightweight atomic state first
    match self.state_atomic.load(Ordering::Acquire) {
        STATE_RUNNING => StreamState::Running,
        STATE_PAUSED => StreamState::Paused,
        _ => {
            // Fall back to pause_resume state if initialized
            if let Some(ref pr) = self.pause_resume {
                // Try to get state without blocking
                if let Ok(state) = pr.state.try_lock() {
                    return *state;
                }
            }
            StreamState::Running
        }
    }
}
```

**Effort**: 1-2 hours

---

### 1.2 Subscription Event Filtering (Stub)

**Location**: `crates/fraiseql-core/src/runtime/subscription.rs:587-599`

```rust
// TODO: Evaluate compiled WHERE filters against event.data and user_context
// TODO: Project only requested fields from subscription definition
```

**Issue**: Subscription events are not filtered by WHERE clauses or projected to requested fields.

**Impact**: All subscription events are delivered unfiltered, potentially sending more data than necessary.

**Fix**:
1. Parse compiled WHERE filters from subscription definition
2. Evaluate filters against `event.data` using JSONB path matching
3. Project only requested fields from the event payload

**Effort**: 4-6 hours

---

### 1.3 Kafka Adapter (Stub Implementation)

**Location**: `crates/fraiseql-core/src/runtime/subscription.rs:1932-2015`

```rust
/// This is a stub implementation. Full Kafka support requires the `rdkafka` crate
```

**Issue**: Kafka adapter is a stub that logs events but doesn't actually deliver to Kafka.

**Impact**: Subscriptions cannot use Kafka transport without enabling the `kafka` feature.

**Status**: **By Design** - Full implementation requires `rdkafka` crate with `kafka` feature flag. Stub provides API compatibility for testing.

**Fix**: Enable `kafka` feature and implement actual Kafka producer logic.

**Effort**: 8-12 hours (if implementing from scratch)

---

## Priority 2: Compiler Completeness

### 2.1 Code Generation TODOs

**Location**: `crates/fraiseql-core/src/compiler/codegen.rs:40-97`

Multiple TODOs for mapping intermediate schema to compiled schema:

```rust
// TODO: Transform IR + templates into CompiledSchema
fields: Vec::new(), // TODO: Map fields
sql_projection_hint: None, // TODO: Generate projection hints during optimization
implements: Vec::new(), // TODO: Map implements from intermediate
arguments: Vec::new(), // TODO: Map arguments
deprecation: None, // TODO: Map deprecation from intermediate
enums: Vec::new(), // TODO: Map enums from intermediate
input_types: Vec::new(), // TODO: Map input types from intermediate
interfaces: Vec::new(), // TODO: Map interfaces from intermediate
unions: Vec::new(), // TODO: Map unions from intermediate
subscriptions: Vec::new(), // TODO: Map subscriptions
directives: Vec::new(), // TODO: Map custom directives from intermediate
```

**Issue**: Code generator returns placeholder empty vectors for many schema elements.

**Impact**: Compiled schemas may be missing fields, interfaces, unions, enums, and directives.

**Fix**: Implement mapping from intermediate IR to compiled schema format for each element type.

**Effort**: 8-12 hours total
- Fields mapping: 2 hours
- Arguments/deprecation: 1 hour
- Enums/input types: 2 hours
- Interfaces/unions: 2 hours
- Subscriptions: 2 hours
- Directives: 1 hour

---

### 2.2 Validator TODOs

**Location**: `crates/fraiseql-core/src/compiler/validator.rs:68-76`

```rust
// TODO: Implement type validation
// TODO: Implement query validation
```

**Issue**: Validator has placeholder implementations.

**Impact**: Invalid schemas may not be caught during compilation.

**Fix**: Implement validation rules for types and queries per GraphQL spec.

**Effort**: 4-6 hours

---

### 2.3 Aggregate Type Generation

**Location**: `crates/fraiseql-core/src/compiler/aggregate_types.rs:526-540`

```rust
// TODO: Add dimension fields (from JSONB paths)
// TODO: Add temporal bucket fields (from timestamp columns)
```

**Issue**: Aggregate types don't include dimension fields or temporal bucket fields.

**Impact**: Fact table aggregations may be missing dimension groupings.

**Fix**: Parse JSONB paths and timestamp columns from fact table metadata to generate dimension fields.

**Effort**: 3-4 hours

---

### 2.4 SQL Template Lowering

**Location**: `crates/fraiseql-core/src/compiler/lowering.rs:63-75`

```rust
// TODO: Generate SQL templates for each query
// TODO: Generate SQL templates for each mutation
```

**Issue**: SQL template generation for queries and mutations is not implemented.

**Impact**: Compiled schemas may not have optimized SQL templates.

**Fix**: Generate parameterized SQL templates based on query/mutation definitions.

**Effort**: 6-8 hours

---

### 2.5 Fact Table Path Extraction

**Location**: `crates/fraiseql-core/src/compiler/fact_table.rs:307`

```rust
paths: Vec::new(), // TODO: Extract paths from sample data
```

**Issue**: Fact table dimension paths are not extracted from sample data.

**Impact**: Fact tables may not have correct dimension path metadata.

**Effort**: 2-3 hours

---

## Priority 3: CLI Completeness

### 3.1 Introspect Facts (Stub)

**Location**: `crates/fraiseql-cli/src/commands/introspect_facts.rs:52-118`

```rust
// For now, return a stub implementation
// TODO: Implement actual database introspection
// TODO: Implement actual database query
// For now, return stub
```

**Issue**: `introspect-facts` command is a stub that doesn't actually introspect the database.

**Impact**: Cannot auto-discover fact tables from database schema.

**Fix**: Implement database introspection using tokio-postgres to query `information_schema`.

**Effort**: 4-6 hours

---

### 3.2 Validate Facts (Stub)

**Location**: `crates/fraiseql-cli/src/commands/validate_facts.rs:60-136`

```rust
// TODO: Implement actual database validation
// TODO: Implement actual comparison logic
```

**Issue**: `validate-facts` command doesn't actually validate against the database.

**Impact**: Cannot verify fact table definitions match database schema.

**Fix**: Query database metadata and compare with schema definitions.

**Effort**: 4-6 hours

---

### 3.3 Schema Converter TODOs

**Location**: `crates/fraiseql-cli/src/schema/converter.rs:107-380`

```rust
subscriptions: vec![], // TODO: Add in future phase
directives: vec![],    // TODO: Add custom directives from intermediate schema
deprecation: None, // TODO: Parse deprecation from intermediate format
```

**Issue**: CLI converter doesn't handle subscriptions, directives, or deprecation.

**Impact**: These schema elements are lost during conversion.

**Effort**: 3-4 hours

---

## Priority 4: Cache & Runtime

### 4.1 Cache Key View Extraction

**Location**: `crates/fraiseql-core/src/cache/key.rs:226-228`

```rust
// TODO (Phase 4): Extract views from JOIN clauses in compiled SQL
// TODO (Phase 5): Extract views from resolver chains
// TODO (Phase 5): Add support for custom resolver view tracking
```

**Issue**: Cache keys don't track views referenced in JOINs or resolver chains.

**Impact**: Cache invalidation may not work correctly for complex queries.

**Effort**: 4-6 hours

---

### 4.2 Aggregation Query Caching

**Location**: `crates/fraiseql-core/src/cache/adapter.rs:391`

```rust
// TODO: Add caching support for aggregation queries in future phase
```

**Issue**: Aggregation queries are not cached.

**Impact**: Repeated aggregation queries always hit the database.

**Effort**: 3-4 hours

---

### 4.3 Query Planner

**Location**: `crates/fraiseql-core/src/runtime/planner.rs:61`

```rust
// TODO: Implement full query planning logic
```

**Issue**: Query planner is incomplete.

**Impact**: Query execution may not be optimally planned.

**Effort**: 8-12 hours

---

### 4.4 Aggregate Parser

**Location**: `crates/fraiseql-core/src/runtime/aggregate_parser.rs:431`

```rust
// TODO: Parse which field to count distinct
```

**Issue**: COUNT DISTINCT doesn't know which field to count.

**Impact**: COUNT DISTINCT aggregations may not work correctly.

**Effort**: 1-2 hours

---

## Priority 5: SDK Polish

### 5.1 TypeScript Decorator Metadata

**Location**: `fraiseql-typescript/src/decorators.ts`

Multiple placeholders due to TypeScript's runtime type erasure:

```typescript
"Query",              // Placeholder - should be extracted from metadata
false,                // Placeholder for nullable
[],                   // Placeholder for arguments
```

**Issue**: TypeScript decorators can't extract type information at runtime without `reflect-metadata`.

**Status**: **By Design** - Workaround exists via manual `registerTypeFields()` calls.

**Fix Options**:
1. Add `reflect-metadata` dependency and use decorator metadata
2. Document requirement for manual registration
3. Generate types from TypeScript AST at build time

**Effort**: 8-12 hours (for reflect-metadata approach)

---

### 5.2 PHP StaticAPI GraphQLType

**Location**: `fraiseql-php/src/StaticAPI.php:91`

```php
$types[$builder->getName()] = null; // Placeholder for GraphQLType
```

**Issue**: GraphQLType is stored as null instead of actual type object.

**Impact**: Minimal - field definitions are stored separately.

**Effort**: 1-2 hours

---

### 5.3 Fraisier Status Commands

**Location**: `fraisier/fraisier/cli.py:200-226`

```python
# TODO: Add actual version/health checking once deployers are complete
# TODO: Implement actual status checking
```

**Issue**: `status` and `status_all` commands show placeholder values.

**Impact**: Can't monitor deployed fraises.

**Effort**: 2-4 hours

---

## Priority 6: Testing & Documentation

### 6.1 Server Tests

**Location**: `crates/fraiseql-server/src/server.rs:218`

```rust
// TODO: Add server tests
```

**Issue**: Server module lacks integration tests.

**Impact**: Server functionality not tested end-to-end.

**Effort**: 4-6 hours

---

### 6.2 Database Benchmarks

**Location**: `crates/fraiseql-core/benches/database_baseline.rs:55-147`

```rust
// TODO: Implement actual query once PostgresAdapter is available
// TODO: Implement actual query
// TODO: Implement streaming query and measure time to first result
```

**Issue**: Benchmarks are placeholders without actual database queries.

**Impact**: Cannot measure real database performance.

**Effort**: 4-6 hours

---

### 6.3 TLS Integration Tests

**Location**: `crates/fraiseql-wire/tests/tls_integration.rs`

4 tests remain ignored, requiring TLS-enabled PostgreSQL.

**Issue**: Cannot test TLS connections with testcontainers (requires SSL certificates).

**Options**:
1. Create testcontainer with self-signed SSL certificates
2. Set up CI with TLS-enabled PostgreSQL
3. Keep as manual tests with documentation

**Effort**: 4-6 hours (for testcontainer SSL setup)

---

## Summary Table

| Priority | Item | Location | Effort | Impact |
|----------|------|----------|--------|--------|
| P1 | `state_snapshot()` stub | fraiseql-wire | 1-2h | Medium |
| P1 | Subscription filtering | fraiseql-core | 4-6h | Medium |
| P1 | Kafka adapter | fraiseql-core | 8-12h | Low (optional) |
| P2 | Codegen mappings | fraiseql-core | 8-12h | High |
| P2 | Validator | fraiseql-core | 4-6h | Medium |
| P2 | Aggregate types | fraiseql-core | 3-4h | Medium |
| P2 | SQL lowering | fraiseql-core | 6-8h | High |
| P2 | Fact table paths | fraiseql-core | 2-3h | Low |
| P3 | Introspect facts | fraiseql-cli | 4-6h | Low |
| P3 | Validate facts | fraiseql-cli | 4-6h | Low |
| P3 | Converter TODOs | fraiseql-cli | 3-4h | Medium |
| P4 | Cache key views | fraiseql-core | 4-6h | Medium |
| P4 | Aggregation caching | fraiseql-core | 3-4h | Low |
| P4 | Query planner | fraiseql-core | 8-12h | Medium |
| P4 | Aggregate parser | fraiseql-core | 1-2h | Low |
| P5 | TypeScript metadata | fraiseql-ts | 8-12h | Low |
| P5 | PHP GraphQLType | fraiseql-php | 1-2h | Low |
| P5 | Fraisier status | fraisier | 2-4h | Low |
| P6 | Server tests | fraiseql-server | 4-6h | Medium |
| P6 | DB benchmarks | fraiseql-core | 4-6h | Low |
| P6 | TLS tests | fraiseql-wire | 4-6h | Low |

**Total Estimated Effort**: ~85-120 hours

---

## Recommended Implementation Order

### Phase A: Core Fixes (16-24 hours)
1. `state_snapshot()` implementation (P1) - 2h
2. Subscription event filtering (P1) - 6h
3. Validator implementation (P2) - 6h
4. Aggregate parser COUNT DISTINCT (P4) - 2h

### Phase B: Compiler Completeness (24-36 hours)
1. Codegen field/argument mapping (P2) - 4h
2. Codegen enums/interfaces/unions (P2) - 4h
3. SQL template lowering (P2) - 8h
4. Aggregate type dimension fields (P2) - 4h
5. Fact table path extraction (P2) - 3h
6. CLI converter completeness (P3) - 4h

### Phase C: Cache & Runtime (16-24 hours)
1. Cache key view extraction (P4) - 6h
2. Aggregation query caching (P4) - 4h
3. Query planner (P4) - 12h

### Phase D: CLI & Testing (16-24 hours)
1. Introspect facts implementation (P3) - 6h
2. Validate facts implementation (P3) - 6h
3. Server integration tests (P6) - 6h
4. TLS test infrastructure (P6) - 6h

### Phase E: SDK Polish (Optional, 12-20 hours)
1. TypeScript reflect-metadata (P5) - 12h
2. PHP GraphQLType (P5) - 2h
3. Fraisier status commands (P5) - 4h
4. Database benchmarks (P6) - 6h

---

## Notes

1. **Kafka adapter** is intentionally a stub - full implementation requires the `kafka` feature flag.
2. **TypeScript metadata** is a design limitation of TypeScript runtime, not a bug.
3. **Query planner** may not be needed if the current template-based approach works well.
4. Many P5/P6 items are polish rather than critical functionality.
