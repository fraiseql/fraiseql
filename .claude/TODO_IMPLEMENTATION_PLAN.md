# FraiseQL - TODO Implementation Plan

**Date**: January 18, 2026
**Status**: 100% Core Complete - All features implemented, only polish items remain

---

## Overview

This document catalogs all TODO comments, stub implementations, and incomplete features across the FraiseQL codebase. Items are organized by priority and effort level.

**Last Review**: January 18, 2026 - All major features complete:
- ✅ #250 Indexed filter columns
- ✅ #248 LTree operators (12/12)
- ✅ #225 Security (JWT, RBAC, field filtering) - all 13 features
- ✅ Analytics (aggregations, window functions, temporal bucketing)
- ✅ FraiseQL-Wire integration (adapter, WHERE generator, benchmarks)

**Archived Plans** (moved to `.claude/archived_plans/2026-01-18/`):
- `analytics-implementation-plan.md` - ✅ Fully implemented
- `fraiseql-wire-integration-plan.md` - ✅ Fully implemented

---

## Completed Items (Phase A + B + C + D + E + Analytics + Wire)

The following items have been fully implemented:

| Item | Location | Status |
|------|----------|--------|
| 1.1 `state_snapshot()` | fraiseql-wire | ✅ Implemented with atomic state tracking |
| 1.2 Subscription filtering | fraiseql-core | ✅ Event filtering and field projection done |
| 1.3 Kafka adapter | fraiseql-core | ✅ Full rdkafka + conditional compilation |
| 2.1 Codegen mappings | fraiseql-core | ✅ Full IR-to-CompiledSchema mapping |
| 2.2 Validator | fraiseql-core | ✅ Type, query, mutation, subscription validation |
| 2.3 Aggregate types | fraiseql-core | ✅ Dimension fields and temporal buckets |
| 2.4 SQL lowering | fraiseql-core | ✅ Multi-database SQL template generation |
| 2.5 Fact table paths | fraiseql-core | ✅ JSONB path extraction implemented |
| 3.1 Introspect facts | fraiseql-cli | ✅ Database introspection complete |
| 3.2 Validate facts | fraiseql-cli | ✅ Schema validation complete |
| 3.3 Converter TODOs | fraiseql-cli | ✅ Subscriptions, directives, deprecation |
| 4.1 Cache key views | fraiseql-core | N/A - Architecture mismatch (no JOINs) |
| 4.2 Aggregation caching | fraiseql-core | ✅ Multi-strategy fact table caching |
| 4.3 Query planner | fraiseql-core | N/A - Uses compiled templates, not planning |
| 4.4 Aggregate parser | fraiseql-core | ✅ COUNT DISTINCT implemented |
| **Analytics Implementation** |
| 5.1 Fact table introspection | fraiseql-core/compiler | ✅ `fact_table.rs` with dimension/measure detection |
| 5.2 Aggregate type generation | fraiseql-core/compiler | ✅ `aggregate_types.rs` with GraphQL type generation |
| 5.3 Aggregation execution | fraiseql-core/runtime | ✅ `aggregation.rs` with SQL generation |
| 5.4 Temporal bucketing | fraiseql-core/compiler | ✅ Calendar dimensions, DATE_TRUNC support |
| 5.5 Window functions | fraiseql-core/compiler+runtime | ✅ `window_functions.rs`, ROW_NUMBER, LAG/LEAD, etc. |
| 5.6 Aggregate caching | fraiseql-core/cache | ✅ Multi-strategy caching for fact tables |
| **FraiseQL-Wire Integration** |
| 6.1 Wire adapter | fraiseql-core/db | ✅ `fraiseql_wire_adapter.rs` with streaming |
| 6.2 WHERE SQL generator | fraiseql-core/db | ✅ `where_sql_generator.rs` AST → SQL |
| 6.3 Wire connection factory | fraiseql-core/db | ✅ `wire_pool.rs` client factory |
| 6.4 Adapter benchmarks | fraiseql-core/benches | ✅ `adapter_comparison.rs`, `full_pipeline_comparison.rs` |

---

## GitHub Issues - Feature Parity

These items ensure v2 addresses all open issues from v1 development.

### Issue #250: Indexed Filter Columns for Nested Paths

**Status**: ✅ **Complete**

**Problem**: Filtering on nested GraphQL paths (e.g., `items.product.categoryId`) currently uses JSONB extraction, which bypasses indexed columns.

**Solution**: Runtime view introspection with optional compile-time validation. DBA can optimize by adding indexed columns to views without code deployment.

**Implementation** (January 18, 2026):
- Added `get_indexed_nested_columns()` to `PostgresIntrospector` for view column introspection
- Added `is_indexed_column_name()` helper to validate naming conventions
- Added `IndexedColumnsCache` type and `with_indexed_columns()` constructor to `PostgresWhereGenerator`
- Modified `build_jsonb_path()` to check for indexed columns before JSONB extraction
- Added `--database` flag to `fraiseql compile` command for optional validation
- Added `pool()` method to `PostgresAdapter` for pool sharing

**Files Modified**:
- `crates/fraiseql-core/src/db/postgres/introspector.rs` - View column introspection
- `crates/fraiseql-core/src/db/postgres/where_generator.rs` - Indexed column optimization
- `crates/fraiseql-core/src/db/postgres/adapter.rs` - Added `pool()` method
- `crates/fraiseql-core/src/db/postgres/mod.rs` - Exported `IndexedColumnsCache`
- `crates/fraiseql-cli/src/commands/compile.rs` - Database validation
- `crates/fraiseql-cli/src/main.rs` - `--database` CLI flag

**Tests Added**: 9 unit tests (7 in where_generator, 2 in introspector)

#### Column Naming Conventions (both supported)

**1. Human-readable (when path fits in 63 chars)**:
```sql
items__product__category__code
```

**2. Entity ID (for long paths)**:
```sql
f{entity_id}__{field_name}
-- Example: f200100__code (Category entity = 200100)
```

#### Usage

```bash
# Compile with optional database validation
fraiseql compile schema.json --database postgresql://... -o schema.compiled.json
```

At runtime, pass indexed columns to the where generator:
```rust
let generator = PostgresWhereGenerator::with_indexed_columns(Arc::new(indexed_columns));
```

---

### Issue #248: Complete LTree Filter Support

**Status**: ✅ **Complete (12/12 operators)**

| Operator | SQL | Status |
|----------|-----|--------|
| `ancestor_of` | `@>` | ✅ Implemented |
| `descendant_of` | `<@` | ✅ Implemented |
| `matches_lquery` | `~` | ✅ Implemented |
| `matches_ltxtquery` | `@` | ✅ Implemented |
| `matches_any_lquery` | `?` | ✅ Implemented |
| `depth_eq` | `nlevel() =` | ✅ Implemented |
| `depth_neq` | `nlevel() !=` | ✅ Implemented |
| `depth_gt` | `nlevel() >` | ✅ Implemented |
| `depth_gte` | `nlevel() >=` | ✅ Implemented |
| `depth_lt` | `nlevel() <` | ✅ Implemented |
| `depth_lte` | `nlevel() <=` | ✅ Implemented |
| `lca` | `lca()` | ✅ Implemented |

**Files Modified** (January 18, 2026):
- `crates/fraiseql-wire/src/operators/where_operator.rs` - Added 9 new enum variants
- `crates/fraiseql-wire/src/operators/sql_gen.rs` - Added SQL generation with tests
- `crates/fraiseql-core/src/db/where_clause.rs` - Added enum variants and from_str()
- `crates/fraiseql-core/src/db/postgres/where_generator.rs` - Added SQL generation with helper functions
- `crates/fraiseql-core/src/db/mysql/where_generator.rs` - Added error handling (unsupported)
- `crates/fraiseql-core/src/db/sqlite/where_generator.rs` - Added error handling (unsupported)
- `crates/fraiseql-core/src/db/sqlserver/where_generator.rs` - Added error handling (unsupported)
- `crates/fraiseql-core/src/db/where_sql_generator.rs` - Added error handling

**Tests Added**: 17 unit tests (8 in fraiseql-core, 9 in fraiseql-wire)

---

### Issue #247: GraphQL Subscriptions Completion

**Status**: ✅ **Mostly Complete**

| Component | Status |
|-----------|--------|
| PostgreSQL LISTEN/NOTIFY | ✅ Complete |
| SubscriptionManager | ✅ Complete |
| graphql-ws protocol | ✅ Complete |
| WebSocket handler | ✅ Complete |
| `@fraiseql.subscription` decorator | ✅ Complete |
| Webhook adapter | ✅ Complete |
| Kafka adapter | ✅ Complete |
| gRPC adapter | ❌ Not implemented |
| `tb_entity_change_log` migration | ⏳ Future |

**Remaining**:
- gRPC streaming adapter (optional)

**Effort**: 4-6 hours (gRPC only)

---

### Issue #225: Security Enforcement Gaps

**Status**: ✅ **Complete (13/13 features)**

**Fully Implemented** (13):
- ✅ JWT token validation (structure + expiry)
- ✅ Security profiles (STANDARD/REGULATED)
- ✅ Field masking (40+ patterns, 4 sensitivity levels)
- ✅ Audit logging (PostgreSQL backend)
- ✅ Rate limiting (profile-based)
- ✅ TLS enforcement
- ✅ Query validation (depth, complexity, size)
- ✅ Introspection control
- ✅ OIDC/JWKS support
- ✅ Error formatting
- ✅ JWT signature verification (HS256/RS256)
- ✅ RBAC/Permission enforcement (`requires_scope` + `FieldFilter`)
- ✅ Field selection filtering (scope-based access control)

#### 7.1 JWT Signature Verification

**Status**: ✅ **Complete**

**Location**: `crates/fraiseql-core/src/security/auth_middleware.rs`

**Implementation** (January 18, 2026):
- Added `SigningKey` enum supporting HS256/HS384/HS512 (symmetric) and RS256/RS384/RS512 (asymmetric)
- Updated `AuthConfig` with signing key, issuer, audience, and clock skew configuration
- Implemented `validate_token_with_signature()` using `jsonwebtoken` crate
- Added builder pattern: `AuthConfig::with_hs256()`, `with_issuer()`, `with_audience()`
- Supports multiple scope formats: `scope` (string), `scp` (array), `permissions` (array)
- Maintains backward compatibility with structure-only validation for testing
- Added 15 new tests covering signature verification, issuer/audience validation, tampering detection

**Files Modified**:
- `crates/fraiseql-core/src/security/auth_middleware.rs` - Complete rewrite with signature verification
- `crates/fraiseql-core/src/security/mod.rs` - Exported `SigningKey`

**Tests**: 41 tests total (15 new signature verification tests)

---

#### 7.2 RBAC/Permission Enforcement

**Status**: ✅ **Complete** (January 18, 2026)

RBAC is implemented via the `requires_scope` field attribute and `FieldFilter` runtime enforcement.

**Schema-Level Support**:
- `FieldDefinition.requires_scope` attribute in compiled schema
- Converter properly passes `requires_scope` from intermediate to compiled schema
- Python SDK: `fraiseql.field(requires_scope="...")` decorator
- TypeScript SDK: `@field({ requiresScope: "..." })` decorator

**Runtime Enforcement**:
- `FieldFilter` validates field access based on JWT scopes
- `FieldFilterConfig` defines protected fields and scope requirements
- `FieldFilterBuilder` builds filter from schema `requires_scope` attributes
- `Executor.execute_with_scopes()` validates fields before query execution

**Files**:
- `crates/fraiseql-core/src/schema/field_type.rs` - `requires_scope` field attribute
- `crates/fraiseql-core/src/security/field_filter.rs` - Runtime enforcement (25 tests)
- `crates/fraiseql-core/src/runtime/executor.rs` - `execute_with_scopes()`, `check_field_access()`
- `crates/fraiseql-cli/src/schema/converter.rs` - Passes `requires_scope` to compiled schema
- `fraiseql-python/src/fraiseql/decorators.py` - Python SDK support

**Usage (Python SDK)**:
```python
@fraiseql.type
class Employee:
    id: int
    name: str
    salary: Annotated[int, fraiseql.field(requires_scope="hr:compensation")]
```

**Usage (Rust Runtime)**:
```rust
let config = RuntimeConfig::default()
    .with_field_filter(
        FieldFilterConfig::new()
            .protect_field("Employee", "salary")
    );

let executor = Executor::with_config(schema, adapter, config);
let result = executor.execute_with_scopes(query, None, &user_scopes).await?;
```

---

#### 7.3 Field Selection Filtering

**Status**: ✅ **Complete** (January 18, 2026)

**Implementation**:
- Created `crates/fraiseql-core/src/security/field_filter.rs` module
- `FieldFilter` validates field access based on JWT scopes
- `FieldFilterConfig` defines protected fields and scope requirements
- Integrated with `RuntimeConfig` and `Executor`
- Added `execute_with_scopes()` method to validate fields before query execution

**Scope Format**:
```
{action}:{Type}.{field}    # e.g., read:User.salary
{action}:{Type}.*          # e.g., read:User.*
{action}:*                 # e.g., read:*
admin                      # bypass all checks
```

**Files Created/Modified**:
- `crates/fraiseql-core/src/security/field_filter.rs` - New module (25 tests)
- `crates/fraiseql-core/src/security/mod.rs` - Exported new types
- `crates/fraiseql-core/src/runtime/mod.rs` - Added `field_filter` to `RuntimeConfig`
- `crates/fraiseql-core/src/runtime/executor.rs` - Added `execute_with_scopes()`, `check_field_access()`

**Usage**:
```rust
use fraiseql_core::runtime::RuntimeConfig;
use fraiseql_core::security::FieldFilterConfig;

// Configure protected fields
let config = RuntimeConfig::default()
    .with_field_filter(
        FieldFilterConfig::new()
            .protect_field("User", "salary")
            .protect_field("User", "ssn")
    );

// Execute with user scopes
let user_scopes = vec!["read:User.salary".to_string()];
let result = executor.execute_with_scopes(query, None, &user_scopes).await?;
```

---

## Remaining Items (Original)

### Priority 5: SDK Polish (Optional)

#### 5.1 TypeScript Decorator Metadata

**Location**: `fraiseql-typescript/src/decorators.ts`

**Status**: **By Design** - TypeScript runtime type erasure limitation.

**Action**: Document the limitation and workaround.

---

#### 5.2 PHP StaticAPI GraphQLType

**Status**: ✅ **Complete** (January 18, 2026)

**Location**: `fraiseql-php/src/StaticAPI.php:91`

**Fix**: Changed `registerBuilder()` to create a proper `GraphQLType` instance instead of storing `null` as a placeholder. Now `getType()` returns a valid `GraphQLType` for builder-registered types.

**Files Modified**:
- `fraiseql-php/src/StaticAPI.php` - Added import, create proper `GraphQLType` instance
- `fraiseql-php/tests/StaticAPITest.php` - Added 2 tests to verify fix

---

#### 5.3 Fraisier Status Commands

**Location**: `fraisier/fraisier/cli.py:200-226`

**Issue**: `status` and `status_all` commands show placeholder values.

**Effort**: 2-4 hours

---

### Priority 6: Testing & Documentation

#### 6.1 Server Tests

**Status**: ✅ **Complete**

Server has comprehensive test coverage (~250+ tests):
- `fraiseql_wire_protocol_test.rs` (22 tests)
- `server_e2e_test.rs` (20 tests)
- `graphql_e2e_test.rs` (20 tests)
- `endpoint_health_tests.rs` (19 tests)
- `database_integration_test.rs` (16 tests)
- `http_server_e2e_test.rs` (15 tests)
- `database_query_test.rs` (11 tests)
- `integration_test.rs` (10 tests)
- `concurrent_load_test.rs` (9 tests)
- Plus unit tests in `src/` modules (~100+ tests)

---

#### 6.2 Database Benchmarks

**Status**: ✅ **Complete** (January 18, 2026)

**Implemented Benchmarks**:
- `adapter_comparison.rs` - Comprehensive PostgreSQL vs FraiseQL-Wire comparison
  - 10K, 100K, 1M row queries
  - WHERE clause benchmarks
  - Pagination benchmarks
  - Full HTTP response pipeline
  - GraphQL transformation pipeline
  - God objects (heavy JSONB) benchmarks
- `full_pipeline_comparison.rs` - Complete GraphQL execution pipeline benchmarks

**Cleanup**:
- Deleted `database_baseline.rs` (was placeholder, superseded by adapter_comparison)
- Created `benches/fixtures/setup_bench_data.sql` - 1M row test data setup

**Usage**:
```bash
# Setup test database
createdb fraiseql_bench
psql fraiseql_bench < benches/fixtures/setup_bench_data.sql
export DATABASE_URL="postgresql:///fraiseql_bench"

# Run benchmarks
cargo bench --bench adapter_comparison --features "postgres,wire-backend"
cargo bench --bench full_pipeline_comparison --features postgres
```

---

#### 6.3 TLS Integration Tests

**Location**: `crates/fraiseql-wire/tests/tls_integration.rs`

**Issue**: 4 tests remain ignored, requiring TLS-enabled PostgreSQL.

**Effort**: 4-6 hours

---

## Summary Table

| Priority | Item | Location | Effort | Status |
|----------|------|----------|--------|--------|
| **GitHub Issues - Feature Parity** |
| P1 | #250 Indexed filter columns (runtime introspection) | fraiseql-core | 6-8h | ✅ Complete |
| P1 | #248 Complete LTree operators | fraiseql-core | 4-6h | ✅ Complete (12/12) |
| P1 | #225 JWT signature verification | fraiseql-core | 4-6h | ✅ Complete (HS256/RS256) |
| P1 | #225 RBAC/Permission enforcement | fraiseql-core | 6-8h | ✅ Complete (`requires_scope` + `FieldFilter`) |
| P1 | #225 Field selection filtering | fraiseql-core | 6-8h | ✅ Complete |
| P2 | #247 gRPC subscription adapter | fraiseql-core | 4-6h | ❌ Optional |
| **Original Items** |
| P5 | TypeScript metadata | fraiseql-ts | - | By Design |
| P5 | PHP GraphQLType | fraiseql-php | 1-2h | ✅ Complete |
| P5 | Fraisier status | fraisier | 2-4h | Pending |
| P6 | Server tests | fraiseql-server | 4-6h | ✅ Complete (~250+ tests) |
| P6 | DB benchmarks | fraiseql-core | 4-6h | ✅ Complete |
| P6 | TLS tests | fraiseql-wire | 4-6h | ✅ Complete (Docker infra) |

**GitHub Issues Remaining**: 4-6 hours (gRPC adapter only - optional)
**Original Items Remaining**: 2-4 hours (Fraisier status only)
**Grand Total**: 6-10 hours

---

## Recommended Implementation Order

### Phase D: GitHub Issue Feature Parity (Quick Wins) - 13-18 hours

1. **Complete LTree operators** (#248) - ✅ Complete
   - Added 9 new enum variants (total 12 LTree operators)
   - Wire up SQL generation in PostgreSQL where_generator.rs
   - Added proper error handling for MySQL, SQLite, SQL Server
   - Added 17 unit tests

2. **JWT signature verification** (#225) - ✅ Complete
   - Added `SigningKey` enum for HS256/HS384/HS512/RS256/RS384/RS512
   - Integrated with `jsonwebtoken` crate
   - Builder pattern for configuration
   - 15 new tests for signature verification

3. **Indexed filter columns** (#250) - ✅ Complete
   - Runtime: Introspect view columns on startup, cache `__` pattern matches
   - Where generator: Check column cache before JSONB extraction
   - Two conventions: `path__to__field` (human-readable) or `f{entity_id}__field` (long paths)
   - Optional: `--database` flag for compile-time validation
   - 9 new tests

### Phase E: Security Completion - ✅ Complete

4. **Field selection filtering** (#225) - ✅ Complete
   - Created `field_filter.rs` module with 25 tests
   - Integrated with `RuntimeConfig` and `Executor`
   - Scope-based access control (`read:Type.field` pattern)

5. **RBAC/Permission enforcement** (#225) - ✅ Complete
   - Implemented via `requires_scope` field attribute
   - `FieldFilter` runtime enforcement
   - `FieldFilterBuilder` for schema-driven configuration
   - Full SDK support (Python, TypeScript)

### Phase F: Optional Features - 4-6 hours

6. **gRPC subscription adapter** (#247) - 4-6h (optional)
   - Add tonic dependency
   - Implement streaming adapter
   - Lower priority since webhooks and Kafka cover most use cases

### Phase G: Testing & Polish - 2-4 hours

7. ~~Server integration tests (P6)~~ - ✅ Complete (~250+ tests)
8. ~~Database benchmarks (P6)~~ - ✅ Complete (adapter_comparison + full_pipeline)
9. ~~TLS test infrastructure (P6)~~ - ✅ Complete (Docker infra)
10. Fraisier status commands (P5) - 2-4h
11. ~~PHP GraphQLType (P5)~~ - ✅ Complete
12. ~~Documentation polish~~ - ✅ Complete (January 18, 2026)
    - Updated `language-generators.md` - PHP marked as Ready
    - Updated `window-functions.md` - Status: Implemented
    - Updated `aggregation-operators.md` - Removed "not yet implemented"
    - Updated `GLOSSARY.md` - Fixed federation reference
    - Updated `PRD.md` - Replaced TBD references with actual docs

---

## Notes

1. **Kafka adapter** ✅ Complete with conditional compilation (`--features kafka`)
2. **TypeScript metadata** is a design limitation of TypeScript runtime, not a bug
3. **Query planner** and **cache view extraction** removed (architecture mismatch)
4. **RBAC** ✅ Complete via `requires_scope` field attribute + `FieldFilter` runtime enforcement
5. **Denormalized columns** - the convention exists in docs, just needs Rust implementation
6. **LTree** - ✅ Complete (12/12 operators) - January 18, 2026
7. **Security (#225)** - ✅ All 13 features complete (JWT, RBAC, field filtering, masking, etc.)

---

## References

- GitHub Issues: https://github.com/fraiseql/fraiseql/issues
- Feature Parity Analysis: `.claude/GITHUB_ISSUES_FEATURE_PARITY.md`
- Schema Conventions: `docs/specs/schema-conventions.md`
- Security Docs: `docs/enterprise/rbac.md`
