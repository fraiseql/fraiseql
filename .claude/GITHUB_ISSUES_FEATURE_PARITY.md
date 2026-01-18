# FraiseQL v2 - GitHub Issues Feature Parity Analysis

**Date**: January 18, 2026
**Status**: Phase D Complete - All P1 Issues Resolved
**Purpose**: Ensure v2 addresses all open issues from v1 development

---

## Summary

| Issue | Title | Status in v2 | Action Required |
|-------|-------|--------------|-----------------|
| #250 | Indexed Filter Columns | ✅ Complete | None |
| #248 | LTree Filter Support | ✅ Complete (12/12) | None |
| #247 | GraphQL Subscriptions | ✅ Mostly Complete | Optional: gRPC adapter |
| #226 | v2.0 Rust-First Architecture | ✅ Complete | This IS v2 |
| #225 | Security Testing & Enforcement | ✅ JWT + Field Filtering Complete | Optional: RBAC |

---

## Issue #250: Indexed Filter Columns for Nested Paths

**Status**: ✅ **Complete** (January 18, 2026)

### Problem

When filtering on nested JSONB paths (e.g., `items.product.categoryId`), FraiseQL generates JSONB extraction SQL that bypasses indexed columns in views:

```sql
-- Without indexed columns (slow): JSONB extraction on every row
WHERE data->'items'->'product'->>'categoryId' = '123'

-- With indexed columns (fast): Use column directly
WHERE "items__product__category_id" = '123'
```

### Solution Implemented

**Runtime view introspection with optional compile-time validation.** DBAs can optimize by adding indexed columns to views without code deployment.

### Implementation Details

1. **View column introspection** - `PostgresIntrospector::get_indexed_nested_columns()`
2. **Indexed column detection** - `is_indexed_column_name()` validates naming conventions
3. **WHERE clause optimization** - `PostgresWhereGenerator::with_indexed_columns()`
4. **Compile-time validation** - `fraiseql compile --database URL` flag

### Column Naming Conventions

**1. Human-readable (when path fits in 63 chars)**:

```sql
items__product__category__code
```

**2. Entity ID format (for long paths)**:

```sql
f{entity_id}__field_name
-- Example: f200100__code (Category entity = 200100)
```

### Files Modified

- `crates/fraiseql-core/src/db/postgres/introspector.rs` - View column introspection
- `crates/fraiseql-core/src/db/postgres/where_generator.rs` - Indexed column optimization
- `crates/fraiseql-core/src/db/postgres/adapter.rs` - Added `pool()` method
- `crates/fraiseql-core/src/db/postgres/mod.rs` - Exported `IndexedColumnsCache`
- `crates/fraiseql-cli/src/commands/compile.rs` - Database validation
- `crates/fraiseql-cli/src/main.rs` - `--database` CLI flag

### Tests Added

9 unit tests (7 in where_generator, 2 in introspector)

---

## Issue #248: LTree Filter Support

**Status**: ✅ **Complete (12/12 operators)** (January 18, 2026)

### All Operators Implemented

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

### Files Modified

- `crates/fraiseql-wire/src/operators/where_operator.rs` - 9 new enum variants
- `crates/fraiseql-wire/src/operators/sql_gen.rs` - SQL generation + tests
- `crates/fraiseql-core/src/db/where_clause.rs` - Enum variants + from_str()
- `crates/fraiseql-core/src/db/postgres/where_generator.rs` - SQL generation
- `crates/fraiseql-core/src/db/mysql/where_generator.rs` - Error handling (unsupported)
- `crates/fraiseql-core/src/db/sqlite/where_generator.rs` - Error handling (unsupported)
- `crates/fraiseql-core/src/db/sqlserver/where_generator.rs` - Error handling (unsupported)

### Tests Added

17 unit tests (8 in fraiseql-core, 9 in fraiseql-wire)

---

## Issue #247: GraphQL Subscriptions

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
| gRPC adapter | ❌ Not implemented (optional) |
| `tb_entity_change_log` migration | ⏳ Future |

### Remaining (Optional)

- **gRPC streaming adapter** - 4-6 hours (if needed)

---

## Issue #226: v2.0 Rust-First Architecture

**Status**: ✅ **Complete** - This IS the v2 implementation

The current codebase IS the Rust-first architecture described in #226:

- Python/TypeScript for authoring only (decorators → JSON)
- Rust core engine with compiled execution
- Pluggable HTTP servers (Axum)
- 100% type-safe security (Rust compiler guarantees)

---

## Issue #225: Security Testing & Enforcement

**Status**: ✅ **10/13 features complete, JWT signature verification added**

### Fully Implemented (11)

| Feature | Status | Location |
|---------|--------|----------|
| JWT token validation | ✅ Complete | `security/auth_middleware.rs` |
| JWT signature verification | ✅ **NEW** | HS256/RS256 with `jsonwebtoken` |
| Token expiry check | ✅ Complete | `security/auth_middleware.rs` |
| Security profiles | ✅ Complete | `security/profiles.rs` |
| Field masking | ✅ Complete | `security/field_masking.rs` |
| Audit logging | ✅ Complete | `security/audit.rs` |
| Rate limiting | ✅ Complete | Profile-based |
| TLS enforcement | ✅ Complete | `security/tls_enforcer.rs` |
| Query validation | ✅ Complete | `security/query_validator.rs` |
| Introspection control | ✅ Complete | `security/introspection_enforcer.rs` |
| OIDC/JWKS | ✅ Complete | `security/oidc.rs` |

### JWT Signature Verification (January 18, 2026)

- Added `SigningKey` enum supporting HS256/HS384/HS512 and RS256/RS384/RS512
- Updated `AuthConfig` with signing key, issuer, audience, clock skew
- Builder pattern: `AuthConfig::with_hs256()`, `with_issuer()`, `with_audience()`
- Supports multiple scope formats: `scope`, `scp`, `permissions`
- Backward compatible with structure-only validation for testing
- 15 new tests for signature verification

### Field Selection Filtering (January 18, 2026)

- Created `field_filter.rs` module with scope-based access control
- `FieldFilter` validates field access based on JWT scopes
- Integrated with `RuntimeConfig` and `Executor::execute_with_scopes()`
- Scope format: `{action}:{Type}.{field}` (e.g., `read:User.salary`)
- 25 unit tests covering all scenarios

### Remaining (Optional, Consider v2.1)

| Feature | Status | Effort |
|---------|--------|--------|
| RBAC/Permission enforcement | ❌ Not implemented | 12-16h |

---

## Phase D + E Completion Summary

**All P1 + P2 (security) items complete!**

| Priority | Issue | Status |
|----------|-------|--------|
| P1 | #250 Indexed filter columns | ✅ Complete |
| P1 | #248 LTree operators | ✅ Complete (12/12) |
| P1 | #225 JWT signature verification | ✅ Complete |
| P2 | #225 Field selection filtering | ✅ Complete |
| P2 | #247 gRPC subscription adapter | ❌ Optional |
| P3 | #225 RBAC enforcement | ❌ Consider v2.1 |

---

## Files Reference

### Indexed Columns (#250)

- `crates/fraiseql-core/src/db/postgres/introspector.rs`
- `crates/fraiseql-core/src/db/postgres/where_generator.rs`
- `crates/fraiseql-cli/src/commands/compile.rs`

### LTree (#248)

- `crates/fraiseql-core/src/db/where_clause.rs`
- `crates/fraiseql-core/src/db/postgres/where_generator.rs`
- `crates/fraiseql-wire/src/operators/where_operator.rs`
- `crates/fraiseql-wire/src/operators/sql_gen.rs`

### Security (#225)

- `crates/fraiseql-core/src/security/auth_middleware.rs` - JWT signature verification
- `crates/fraiseql-core/src/security/field_filter.rs` - Field selection filtering
- `crates/fraiseql-core/src/security/mod.rs` - Module exports
- `crates/fraiseql-core/src/runtime/mod.rs` - RuntimeConfig with field_filter
- `crates/fraiseql-core/src/runtime/executor.rs` - execute_with_scopes()

### Subscriptions (#247)

- `crates/fraiseql-core/src/runtime/subscription.rs`
