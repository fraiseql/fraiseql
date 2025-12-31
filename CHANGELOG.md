# Changelog

All notable changes to FraiseQL will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.9.1] - 2025-12-31

**Stable Release - GraphQL Info Auto-Injection + Test Coverage Improvements**

This release adds automatic GraphQL info injection middleware that enables the Rust zero-copy pipeline without requiring developers to manually pass `info=info` to repository methods.

### Added

#### GraphQL Info Auto-Injection (Issue #199)

Added `GraphQLInfoInjector` middleware that automatically injects GraphQL info into resolver contexts, enabling optimal Rust pipeline performance without manual info parameter passing.

**Benefits**:
- 7-10x faster serialization (automatic Rust pipeline activation)
- 60-80% smaller payloads (field selection works automatically)
- Improved developer experience (no need to remember `info=info`)
- Backwards compatible with existing code

**Implementation**:
- `src/fraiseql/middleware/graphql_info_injector.py` - Auto-injection middleware
- `tests/unit/middleware/test_graphql_info_injector.py` - Comprehensive test coverage (24 tests)

**Testing**:
- 24 unit tests (async + sync resolvers)
- 80%+ code coverage
- Edge cases: positional args, kwargs, missing context, None values
- Backwards compatibility verified

**Credit**: Middleware implementation and tests by @purvanshjoshi (PR #201)

### Fixed

- Improved test coverage for GraphQLInfoInjector middleware (54% → 80%+)
- Added sync resolver test coverage (11 additional tests)
- Added positional argument handling tests
- Added edge case tests for robustness

## [1.9.0b1] - 2024-12-30

**Beta Release - Nested JSONB Field Fix + Rust Performance Optimizations**

This beta release includes a critical bug fix for nested JSONB field filtering and significant Rust pipeline optimizations reviewed by a Rust specialist.

### Fixed

#### Nested JSONB Field Filtering (Underscore+Number Patterns)

Fixed critical bug where nested JSONB fields with underscore+number patterns (e.g., `dns_1.ip_address`) were being filtered out, returning only `__typename` instead of requested fields.

**Root Cause**: Materialized path mismatch between database (snake_case), Python (partial camelCase), and Rust (checking wrong variants)
- Database: `dns_1.ip_address` (snake_case)
- Python: `dns1.ip_address` (parent camelCase, child snake_case)
- Rust: Only checking `dns_1.ip_address` and `dns1.ipAddress` (fully camelCase)

**Solution**: Added partial camelCase path matching to handle Python's format

**Files Modified**:
- `fraiseql_rs/src/json_transform.rs` - Added path matching logic
- `fraiseql_rs/src/lib.rs` - Updated PyO3 bindings
- `src/fraiseql/core/rust_pipeline.py` - Pass field_selections as list

**Testing**: 20+ WHERE clause tests, 6103 functional tests pass, zero regressions

### Performance

#### Rust Pipeline Optimizations (Rust Specialist Reviewed)

Implemented comprehensive performance optimizations based on specialist code review:

1. **Pre-compute path variants** (50-80% overhead reduction)
   - Cache all three path formats (snake_case, camelCase, partial) upfront in `build_alias_map()`
   - Trade memory (3x HashSet size) for speed (O(1) lookups vs O(N) conversions)
   - Eliminates repeated string allocations during field filtering

2. **Optimize string allocations** in path construction
   - Use `String::with_capacity()` and `push_str()` instead of Vec collect + join
   - Reduces allocations in `to_camel_case_path()` hot path
   - Pre-allocate string buffers based on input length

3. **Improve PyO3 type conversion**
   - Add comprehensive `python_to_json()` helper supporting all Python types
   - Handle None, bool, int, float, str, dict, list with recursive conversion
   - Simplify `build_graphql_response()` from 33 lines to 5 lines
   - More robust error handling for invalid float values

**Current Performance** (after optimizations):
- Rust Pipeline: 0.03ms (4% of total request time)
- Total Request: 0.62ms
- PostgreSQL: 0.54ms (86%)
- 18 performance tests pass in 2.06 seconds

### Code Quality

#### Rust Code Improvements

1. **Extract path matching logic** to helper function
   - Move complex matching logic to `path_matches_selection()`
   - Improves readability and testability
   - Centralizes prefix/exact match logic

2. **Add inline documentation**
   - Document all three path variants (snake_case, camelCase, partial)
   - Explain byte-level checks for prefix matching
   - Clarify performance trade-offs in comments

**Files Modified**:
- `fraiseql_rs/src/json_transform.rs` - Path optimization + helper functions
- `fraiseql_rs/src/lib.rs` - PyO3 type conversion improvements
- `uv.lock` - Dependency updates from maturin build

**Testing**: All pre-commit hooks pass (rustfmt, clippy, ruff)

### Beta Testing

This is a **beta release** for real-world validation before the stable 1.9.0 release.

**Beta Duration**: 1-2 weeks
**Monitoring Focus**:
- Nested JSONB field filtering in production scenarios
- Performance validation in high-load environments
- Path matching edge cases with unusual field names
- Memory usage patterns with pre-computed path variants

**Promote to 1.9.0 Stable**: Mid-January 2025 (if no critical issues found)

## [1.9.0] - 2025-12-30

**Major Release - Native Tokio-Postgres Driver + Rust Performance**

This release represents a complete architectural shift to native Rust async database operations using tokio-postgres, delivering 7-10x performance improvements for database operations.

### Added

#### Native Tokio-Postgres Database Driver (Rust)

Complete rewrite of the database layer using tokio-postgres for native async Rust performance:

**Core Features:**
- **Native Rust async driver**: tokio-postgres with deadpool connection pooling
- **Zero-copy query execution**: Streaming results with minimal allocations
- **Connection pooling**: Configurable pool with health checks and timeouts
- **ACID transactions**: Full transaction support with savepoints
- **WHERE clause builder**: Type-safe query construction in Rust
- **Prepared statements**: Automatic statement caching and reuse

**Performance Improvements:**
- **7-10x faster** than psycopg3 for large result sets (>1000 rows)
- **Zero-copy deserialization**: Direct JSON transformation without Python overhead
- **Efficient connection reuse**: Pooling reduces connection overhead
- **Streaming results**: Memory-efficient processing of large datasets
- **Concurrent queries**: True parallelism with Rust async runtime

**Architecture:**
- `fraiseql_rs/src/db/pool.rs` - Connection pool management (deadpool-postgres)
- `fraiseql_rs/src/db/query.rs` - Query execution with streaming
- `fraiseql_rs/src/db/transaction.rs` - ACID transaction handling
- `fraiseql_rs/src/db/where_builder.rs` - Type-safe WHERE clause construction
- `fraiseql_rs/src/db/types.rs` - Type definitions and configurations

**Dependencies:**
```toml
tokio-postgres = "0.7"  # Async PostgreSQL driver
deadpool-postgres = "0.14"  # Connection pooling
```

**Testing:**
- Full integration test suite with real PostgreSQL
- Chaos engineering tests (145/145 passing)
- Connection pool stress testing
- Transaction isolation verification
- Performance benchmarks

**Migration Notes:**
- Fully backwards compatible with existing FraiseQL code
- Python API unchanged (database layer abstracted)
- Automatic fallback to psycopg3 if Rust extension unavailable
- No code changes required for existing applications

**Performance Comparison** (1000 row query):
- psycopg3 (Python): ~15ms
- tokio-postgres (Rust): ~1.5ms (10x faster)

#### Operational Runbooks

Added comprehensive operational runbooks for production incident response (~4,000 lines of documentation):

- **Database Performance Degradation** - Diagnose and resolve slow queries, connection pool issues, and query timeouts
- **High Memory Usage** - Handle memory leaks, OOM events, and resource exhaustion
- **Rate Limiting Triggered** - Investigate rate limit violations and distinguish legitimate traffic from abuse
- **GraphQL Query DoS** - Detect and mitigate expensive queries and DoS attacks
- **Authentication Failures** - Troubleshoot auth failures, token issues, and brute force attacks

**Features:**
- MTTR targets (10-20 minutes per incident)
- Prometheus alert rules and Grafana dashboard panels
- Structured log parsing with jq examples
- PostgreSQL diagnostic queries
- Step-by-step resolution procedures (immediate, short-term, long-term)
- Post-incident review templates
- Escalation paths and emergency contacts

**Location:** `docs/production/runbooks/`

#### ID Type for GraphQL-Standard Identifiers

Added ID type as the GraphQL-standard scalar for all identifiers:

- **ID Type**: `from fraiseql.types import ID` (replaces UUID in examples)
- **IDScalar**: GraphQL scalar type (backed by UUID in PostgreSQL)
- **CLI Integration**: Generated code now uses ID type by default
- **Migration**: All documentation examples updated from UUID to ID

**Rationale:**
- GraphQL standard compliance (ID is the standard scalar for identifiers)
- Better developer experience (shorter, clearer than UUID)
- Future-proof (opaque identifiers)

#### Rust Safety Improvements

**Memory Safety:**
- Arena allocator memory bounds (10MB limit)
- JSON recursion depth limits (64 levels)
- Panic elimination in production hot paths

**Code Quality:**
- Zero Clippy strict warnings (`cargo clippy -- -D warnings`)
- Property-based testing for Arena allocator
- Comprehensive SAFETY comments (50+ lines of documentation)

**Testing:**
- Chaos test stability: 145/145 passing (100%)
- Property-based fuzz tests for memory safety
- Reduced panic risks from 337 to 328

### Fixed

#### Complex AND/OR WHERE Clause Filtering (Issue #124 Edge Cases)

Fixed critical bugs in WHERE clause normalization that caused complex nested AND/OR filter combinations to be incorrectly flattened, resulting in lost or improperly combined filter conditions.

**Root Causes:**
1. **OR Handler Bug**: When processing OR clauses with nested AND conditions, the handler was flattening nested structures, losing AND grouping within OR branches
2. **AND Handler Bug**: When processing AND clauses with nested OR conditions, the handler was similarly flattening, losing OR clauses nested within AND
3. **Empty Set Check**: FK detection used truthiness check that failed on empty sets

**Impact:**
- Complex queries like `(device=X AND status=Y) OR (device=Z AND status=W)` returned incorrect results
- Queries like `(device=X OR device=Y) AND status=Z` completely lost the OR clause
- Affected any query combining multiple levels of boolean logic

**Solution:**
- OR Handler: Now preserves entire `WhereClause` structures as `nested_clauses`
- AND Handler: Checks for complex nested structures and preserves them
- Empty Set Check: Fixed to use `len(set) == 0` instead of truthiness

**Files Changed:**
- `src/fraiseql/where_normalization.py` - Fixed OR, AND, and empty set checks
- `src/fraiseql/db.py` - Added metadata fallback for hybrid tables
- `src/fraiseql/gql/builders/registry.py` - Preserve metadata during re-registration
- `tests/regression/issue_124/` - Added 12 comprehensive edge case tests

**Testing:** All 6076+ tests passing (100% success rate), zero regressions

#### Python Builtin Shadowing Prevention

Fixed potential issues where 'type' and 'input' could shadow Python builtins:

- Removed `type` and `input` from `__all__` exports
- Added `__getattr__` support for `fraiseql.type` and `fraiseql.input`
- Users should use `from fraiseql import fraise_type` or access via `fraiseql.type`

### Changed

#### Documentation Improvements

- **Architecture Documentation**: Added 4 comprehensive Mermaid diagrams
  - Request Flow (query → database → response)
  - Trinity Pattern (UUID identifiers across schemas)
  - Type System (Python → GraphQL → PostgreSQL)
  - CQRS Design (command/query separation)

- **Field Documentation Guide**: Added comprehensive guide for docstring styles
  - Google style (recommended)
  - Sphinx style
  - NumPy style
  - Best practices and examples

- **Link Quality**: Fixed 100+ broken internal documentation links

#### Rust Code Quality

- Clippy strict mode: Zero warnings with `-D warnings`
- Reduced excessive nesting (14+ warnings fixed)
- Implemented `Default` trait idiomatically
- Replaced `Unknown` type with `Option<SchemaType>`

---

## [1.8.9] - 2025-12-20
