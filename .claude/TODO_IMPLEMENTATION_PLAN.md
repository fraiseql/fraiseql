# FraiseQL - TODO Implementation Plan

**Date**: January 18, 2026
**Status**: ~98% Complete - Phase D complete + Field Selection Filtering (P2)

---

## Overview

This document catalogs all TODO comments, stub implementations, and incomplete features across the FraiseQL codebase. Items are organized by priority and effort level.

**Last Review**: January 18, 2026 - Phase D complete. All P1 GitHub issues implemented:
- ✅ #250 Indexed filter columns
- ✅ #248 LTree operators (12/12)
- ✅ #225 JWT signature verification

---

## Completed Items (Phase A + B + C)

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

---

## GitHub Issues - Feature Parity (NEW)

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

**Status**: ⚠️ **Partial (10/13 features)**

**Fully Implemented** (10):
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

**Critical Gaps** (3):

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

**Location**: Documented in `docs/enterprise/rbac.md` (845 lines) but NOT in Rust

**Issue**: RBAC design exists but no runtime enforcement in `fraiseql-core`.

**Required**:
- Create `crates/fraiseql-core/src/security/permissions.rs`
- Implement permission resolver with caching
- Add field-level permission enforcement
- Integrate with query execution

**Effort**: 12-16 hours

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

**Location**: `fraiseql-php/src/StaticAPI.php:91`

**Status**: Minimal impact - field definitions are stored separately.

**Effort**: 1-2 hours if needed

---

#### 5.3 Fraisier Status Commands

**Location**: `fraisier/fraisier/cli.py:200-226`

**Issue**: `status` and `status_all` commands show placeholder values.

**Effort**: 2-4 hours

---

### Priority 6: Testing & Documentation

#### 6.1 Server Tests

**Location**: `crates/fraiseql-server/src/server.rs:218`

**Issue**: Server module lacks integration tests.

**Effort**: 4-6 hours

---

#### 6.2 Database Benchmarks

**Location**: `crates/fraiseql-core/benches/database_baseline.rs:55-147`

**Issue**: Benchmarks are placeholders without actual database queries.

**Effort**: 4-6 hours

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
| P2 | #225 Field selection filtering | fraiseql-core | 6-8h | ✅ Complete |
| P2 | #247 gRPC subscription adapter | fraiseql-core | 4-6h | ❌ Optional |
| P3 | #225 RBAC/Permission enforcement | fraiseql-core | 12-16h | ❌ Consider v2.1 |
| **Original Items** |
| P5 | TypeScript metadata | fraiseql-ts | - | By Design |
| P5 | PHP GraphQLType | fraiseql-php | 1-2h | Low priority |
| P5 | Fraisier status | fraisier | 2-4h | Pending |
| P6 | Server tests | fraiseql-server | 4-6h | Pending |
| P6 | DB benchmarks | fraiseql-core | 4-6h | Pending |
| P6 | TLS tests | fraiseql-wire | 4-6h | Pending |

**GitHub Issues Total**: 35-48 hours
**Original Items Total**: 15-25 hours
**Grand Total**: 50-73 hours

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

### Phase E: Security Completion - 4-6 hours

4. **Field selection filtering** (#225) - ✅ Complete
   - Created `field_filter.rs` module with 25 tests
   - Integrated with `RuntimeConfig` and `Executor`
   - Scope-based access control (`read:Type.field` pattern)

5. **gRPC subscription adapter** (#247) - 4-6h (optional)
   - Add tonic dependency
   - Implement streaming adapter

### Phase F: RBAC (Consider v2.1) - 12-16 hours

6. **RBAC/Permission enforcement** (#225) - 12-16h
   - Most complex feature
   - Port design from docs to Rust
   - May defer to v2.1 release

### Phase G: Testing & Polish - 15-25 hours

7. Server integration tests (P6) - 6h
8. Database benchmarks (P6) - 6h
9. TLS test infrastructure (P6) - 6h
10. Fraisier status commands (P5) - 4h
11. PHP GraphQLType (P5) - 2h

---

## Notes

1. **Kafka adapter** ✅ Complete with conditional compilation (`--features kafka`)
2. **TypeScript metadata** is a design limitation of TypeScript runtime, not a bug
3. **Query planner** and **cache view extraction** removed (architecture mismatch)
4. **RBAC** is the most complex remaining item - consider deferring to v2.1
5. **Denormalized columns** - the convention exists in docs, just needs Rust implementation
6. **LTree** - ✅ Complete (12/12 operators) - January 18, 2026

---

## References

- GitHub Issues: https://github.com/fraiseql/fraiseql/issues
- Feature Parity Analysis: `.claude/GITHUB_ISSUES_FEATURE_PARITY.md`
- Schema Conventions: `docs/specs/schema-conventions.md`
- Security Docs: `docs/enterprise/rbac.md`
