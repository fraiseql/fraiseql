# FraiseQL - TODO Implementation Plan

**Date**: January 18, 2026
**Status**: ~99% Complete - Most items done, remaining are polish/optional

---

## Overview

This document catalogs all TODO comments, stub implementations, and incomplete features across the FraiseQL codebase. Items are organized by priority and effort level.

**Last Review**: January 18, 2026 - Architecture assessment completed.

---

## Completed Items (Phase A + B)

The following items have been fully implemented:

| Item | Location | Status |
|------|----------|--------|
| 1.1 `state_snapshot()` | fraiseql-wire | ✅ Implemented with atomic state tracking |
| 1.2 Subscription filtering | fraiseql-core | ✅ Event filtering and field projection done |
| 2.1 Codegen mappings | fraiseql-core | ✅ Full IR-to-CompiledSchema mapping |
| 2.2 Validator | fraiseql-core | ✅ Type, query, mutation, subscription validation |
| 2.3 Aggregate types | fraiseql-core | ✅ Dimension fields and temporal buckets |
| 2.4 SQL lowering | fraiseql-core | ✅ Multi-database SQL template generation |
| 2.5 Fact table paths | fraiseql-core | ✅ JSONB path extraction implemented |
| 3.3 Converter TODOs | fraiseql-cli | ✅ Subscriptions, directives, deprecation |
| 4.1 Cache key views | fraiseql-core | N/A - Architecture mismatch (no JOINs) |
| 4.3 Query planner | fraiseql-core | N/A - Uses compiled templates, not planning |
| 4.4 Aggregate parser | fraiseql-core | ✅ COUNT DISTINCT implemented |

---

## Architecture Mismatches (Removed)

### Cache Key View Extraction (4.1)

**Status**: **N/A - Removed**

FraiseQL uses single-table compiled SQL templates (no JOINs or resolver chains). The `sql_source` field is the complete set of accessed views for cache invalidation. No changes needed.

### Query Planner (4.3)

**Status**: **N/A - Removed**

FraiseQL is a **compiled execution engine**:
- Queries are matched to pre-compiled templates
- SQL comes from `sql_source` (not dynamically planned)
- No runtime query optimization needed

The current planner implementation is complete for FraiseQL's architecture:
- Generates ExecutionPlan from QueryMatch
- Extracts projection fields
- Provides cost estimates

The TODO for "full query planning logic" is an artifact from a traditional GraphQL architecture that doesn't apply.

---

## Remaining Items

### Priority 1: By Design (Optional)

#### 1.3 Kafka Adapter (Stub Implementation)

**Location**: `crates/fraiseql-core/src/runtime/subscription.rs`

**Status**: **By Design** - Full implementation requires `rdkafka` crate with `kafka` feature flag. Stub provides API compatibility for testing.

**Action**: Keep as-is unless Kafka support is explicitly needed.

---

### Priority 3: CLI Completeness

#### 3.1 Introspect Facts (Stub)

**Location**: `crates/fraiseql-cli/src/commands/introspect_facts.rs:52-118`

```rust
// TODO: Implement actual database introspection
// TODO: Implement actual database query
```

**Issue**: `introspect-facts` command is a stub that doesn't actually introspect the database.

**Impact**: Cannot auto-discover fact tables from database schema.

**Fix**: Implement database introspection using tokio-postgres to query `information_schema`.

**Effort**: 4-6 hours

---

#### 3.2 Validate Facts (Stub)

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

### Priority 4: Cache & Runtime

#### 4.2 Aggregation Query Caching

**Location**: `crates/fraiseql-core/src/cache/adapter.rs:391`

```rust
// TODO: Add caching support for aggregation queries in future phase
```

**Issue**: Aggregation queries are not cached.

**Impact**: Repeated aggregation queries always hit the database.

**Effort**: 3-4 hours

---

### Priority 5: SDK Polish (Optional)

#### 5.1 TypeScript Decorator Metadata

**Location**: `fraiseql-typescript/src/decorators.ts`

**Status**: **By Design** - TypeScript runtime type erasure limitation. Workaround exists via manual `registerTypeFields()` calls.

**Action**: Document the limitation and workaround.

---

#### 5.2 PHP StaticAPI GraphQLType

**Location**: `fraiseql-php/src/StaticAPI.php:91`

**Status**: Minimal impact - field definitions are stored separately.

**Effort**: 1-2 hours if needed

---

#### 5.3 Fraisier Status Commands

**Location**: `fraisier/fraisier/cli.py:200-226`

```python
# TODO: Add actual version/health checking once deployers are complete
# TODO: Implement actual status checking
```

**Issue**: `status` and `status_all` commands show placeholder values.

**Impact**: Can't monitor deployed fraises.

**Effort**: 2-4 hours

---

### Priority 6: Testing & Documentation

#### 6.1 Server Tests

**Location**: `crates/fraiseql-server/src/server.rs:218`

```rust
// TODO: Add server tests
```

**Issue**: Server module lacks integration tests.

**Impact**: Server functionality not tested end-to-end.

**Effort**: 4-6 hours

---

#### 6.2 Database Benchmarks

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

#### 6.3 TLS Integration Tests

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

| Priority | Item | Location | Effort | Status |
|----------|------|----------|--------|--------|
| P1 | Kafka adapter | fraiseql-core | 8-12h | By Design (optional) |
| P3 | Introspect facts | fraiseql-cli | 4-6h | Pending |
| P3 | Validate facts | fraiseql-cli | 4-6h | Pending |
| P4 | Aggregation caching | fraiseql-core | 3-4h | Pending |
| P5 | TypeScript metadata | fraiseql-ts | - | By Design |
| P5 | PHP GraphQLType | fraiseql-php | 1-2h | Low priority |
| P5 | Fraisier status | fraisier | 2-4h | Pending |
| P6 | Server tests | fraiseql-server | 4-6h | Pending |
| P6 | DB benchmarks | fraiseql-core | 4-6h | Pending |
| P6 | TLS tests | fraiseql-wire | 4-6h | Pending |

**Total Remaining Effort**: ~35-50 hours (mostly optional/polish)

---

## Recommended Implementation Order

### Phase C: Cache & CLI (8-12 hours)
1. Aggregation query caching (P4) - 4h
2. Introspect facts implementation (P3) - 4h
3. Validate facts implementation (P3) - 4h

### Phase D: Testing (12-18 hours)
1. Server integration tests (P6) - 6h
2. Database benchmarks (P6) - 6h
3. TLS test infrastructure (P6) - 6h

### Phase E: SDK Polish (Optional, 4-8 hours)
1. Fraisier status commands (P5) - 4h
2. PHP GraphQLType (P5) - 2h

---

## Notes

1. **Kafka adapter** is intentionally a stub - full implementation requires the `kafka` feature flag.
2. **TypeScript metadata** is a design limitation of TypeScript runtime, not a bug.
3. **Query planner** and **cache view extraction** were removed as they don't match FraiseQL's compiled template architecture.
4. Most remaining items are polish/optional rather than critical functionality.
5. The core engine is feature-complete for the compiled GraphQL execution model.
