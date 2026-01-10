# Changelog

All notable changes to FraiseQL will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.9.7] - 2025-01-10

**Entity Field Selection for Mutations + IDFilter for Where Clauses**

This release adds two major features:
1. GraphQL field selection support for nested entity objects in mutation responses
2. IDPolicy-aware ID filtering in where clauses (from v1.9.3-v1.9.6)

Both features improve developer experience and reduce payload sizes.

### Added

#### Nested Entity Field Filtering

Mutations now respect GraphQL field selections for nested entity objects:

**Before (v1.9.6)**: Mutations returned ALL entity fields regardless of query selection
```graphql
mutation {
  createLocation(input: {name: "Warehouse"}) {
    ... on CreateLocationSuccess {
      location { id name }  # ❌ Returned ALL 20 fields
    }
  }
}
```

**After (v1.9.7)**: Mutations return ONLY requested fields
```graphql
mutation {
  createLocation(input: {name: "Warehouse"}) {
    ... on CreateLocationSuccess {
      location { id name }  # ✅ Returns only id and name
    }
  }
}
```

#### Implementation Details

**Python Layer** (`mutation_decorator.py`):
- `_extract_nested_selections()`: Recursively extracts nested field selections from GraphQL AST
- `_extract_entity_field_selections()`: Parses inline fragments to find entity field selections
- Automatically passes selections to Rust pipeline as JSON

**Rust Layer** (`fraiseql_rs/src/mutation/entity_filter.rs`):
- `filter_entity_fields()`: Recursive filtering algorithm for nested objects
- Handles objects, arrays, primitives, and null values
- Zero overhead when no selections provided (backward compatible)

#### IDFilter for Where Clauses (from v1.9.3-v1.9.6)

New `IDFilter` class for filtering ID fields in where clauses with IDPolicy awareness:

```python
@fraise_input
class IDFilter:
    eq: ID | None = None
    neq: ID | None = None
    in_: list[ID] | None = None
    nin: list[ID] | None = None
    isnull: bool | None = None
```

**Key Features:**
- ID type **always** uses `IDFilter` regardless of IDPolicy configuration
- GraphQL schema stays consistent (`$id: ID!`)
- UUID validation (if `IDPolicy.UUID`) happens at runtime, not schema level
- No frontend query changes needed when switching policies

```python
from fraiseql.config.schema_config import SchemaConfig, IDPolicy

# Both policies use ID scalar in GraphQL schema
SchemaConfig.set_config(id_policy=IDPolicy.UUID)  # Validates UUID format at runtime
SchemaConfig.set_config(id_policy=IDPolicy.OPAQUE)  # Accepts any string
```

### Performance Impact

- **Payload Reduction**: 30-90% smaller responses (depends on entity size)
- **Filtering Overhead**: <1ms per mutation (negligible)
- **Network Savings**: Significant for large entities (e.g., Location: 20+ fields)

### Testing

- **10 Python unit tests**: Entity field selection extraction
- **16+ Rust unit tests**: Filtering logic for nested objects, arrays, edge cases
- **4 integration tests**: End-to-end with PostgreSQL database
- **Backward compatibility**: All 97 existing mutation tests pass unchanged

### Files Modified

| File | Lines | Change |
|------|-------|--------|
| `src/fraiseql/mutations/mutation_decorator.py` | +95 | Entity field extraction from GraphQL AST |
| `src/fraiseql/mutations/rust_executor.py` | +3 | Pass entity_selections to Rust |
| `fraiseql_rs/src/mutation/entity_filter.rs` | +250 | Recursive filtering algorithm (NEW) |
| `fraiseql_rs/src/mutation/response_builder.rs` | +13 | Apply filtering in response builder |
| `fraiseql_rs/src/mutation/mod.rs` | +3 | Updated API signature |
| `fraiseql_rs/src/lib.rs` | +2 | PyO3 binding update |
| `tests/unit/mutations/test_entity_field_extraction.py` | +430 | Python unit tests (NEW) |
| `fraiseql_rs/src/mutation/tests/entity_field_filtering.rs` | +450 | Rust unit tests (NEW) |
| `tests/integration/graphql/mutations/test_entity_field_selection_integration.py` | +490 | Integration tests (NEW) |

**Total**: 10 files changed, +1,830 lines added

## [1.9.2] - 2025-01-05

**Security & ID Policy Release - APQ Fixes + Configurable ID Behavior**

This release fixes critical security vulnerabilities in APQ response caching and introduces configurable ID policy for GraphQL scalar behavior.

### Added

#### Configurable ID Policy

New `IDPolicy` configuration for GraphQL ID scalar behavior:

- **`IDPolicy.UUID`** (default): IDs must be valid UUIDs - FraiseQL's opinionated approach
- **`IDPolicy.OPAQUE`**: IDs accept any string - GraphQL spec-compliant mode

```python
from fraiseql.config.schema_config import SchemaConfig, IDPolicy

# Default: UUID enforcement (recommended)
SchemaConfig.set_config(id_policy=IDPolicy.UUID)

# GraphQL spec-compliant: accepts any string
SchemaConfig.set_config(id_policy=IDPolicy.OPAQUE)
```

#### Semantic Type Mapping Fix

- `uuid.UUID` now always maps to `UUIDScalar` (GraphQL name: "UUID")
- Only the `ID` type annotation is affected by the ID policy
- Clearer distinction between entity identifiers (`ID`) and generic UUIDs (`uuid.UUID`)

### Changed

#### Examples Updated to Use ID Type

- 44 example files updated to use `ID` type for entity identifiers
- Consistent with Trinity pattern: `id: ID` for external identifiers
- Added `from fraiseql.types import ID` imports

### Documentation

- Updated `docs/core/id-type.md` with ID Policy documentation
- Added `SchemaConfig` section to `docs/core/configuration.md`
- Updated `docs/getting-started/quickstart.md` to use `ID` type

### Security

#### APQ Response Cache Data Leakage (CRITICAL)

Fixed a critical vulnerability where APQ cached responses could be served to the wrong users due to missing variable consideration in cache keys.

**Vulnerability Details**:
- Cache keys were computed using only query hash, ignoring GraphQL variables
- Query `{ user(id: $id) { name } }` with `{id: 1}` would cache User 1's data
- Same query with `{id: 2}` would incorrectly return User 1's cached data
- **Impact**: Cross-user data leakage in multi-tenant applications

**Fix**: Implemented `compute_response_cache_key()` that combines query hash with normalized JSON variables, ensuring different variable values produce different cache entries.

### Fixed

#### APQ Field Selection Not Respected

Fixed bug where cached APQ responses returned full payloads instead of only the requested fields.

**Bug Details**:
- Query `{ user { name } }` should return only `{ "user": { "name": "John" } }`
- Instead returned full object: `{ "user": { "id": 1, "name": "John", "email": "...", ... } }`
- Cached responses ignored the GraphQL field selection from the query

**Fix**: Added `apq_selection` module that parses GraphQL queries and filters responses:
- `extract_selection_set()`: Parses query to extract requested fields
- `filter_response_by_selection()`: Filters response to match selection
- Defense in depth: Filtering applied on both cache store AND retrieve
- Full support for fragments, aliases, nested objects, and lists

#### Full Response Cached for Partial Requests

Fixed bug where the full resolver response was cached even when only partial fields were requested.

**Bug Details**:
- Resolver returns `{ id, name, email, metadata }` for ORM object
- Query requests only `{ id, name }`
- Full response was cached, wasting memory and potentially exposing unrequested data

**Fix**: Response is now filtered by field selection BEFORE storing in cache.

### Added

#### APQ Selection Module

New module `src/fraiseql/middleware/apq_selection.py` for GraphQL field selection:
- Parse GraphQL queries using `graphql-core` library
- Extract selection sets with operation name support
- Filter responses based on field selection
- Handle fragments (named and inline)
- Support field aliases and deeply nested structures

### Testing

- 22 new unit tests for APQ selection extraction and filtering
- Updated regression tests (removed 3 xfail markers)
- All 47 APQ-related tests passing

### Files Modified

| File | Change |
|------|--------|
| `src/fraiseql/middleware/apq_selection.py` | New module for selection parsing and filtering |
| `src/fraiseql/middleware/apq_caching.py` | Added variable-aware cache keys and response filtering |
| `src/fraiseql/fastapi/routers.py` | Pass query_text and operation_name for filtering |
| `tests/middleware/test_apq_selection.py` | 22 unit tests for new module |
| `tests/regression/test_apq_field_selection_bug.py` | Removed xfail markers, tests now pass |

## [1.9.1] - 2025-12-31

**Stable Release - GraphQL Info Auto-Injection + Security Updates**

This release adds automatic GraphQL info injection middleware that enables the Rust zero-copy pipeline without requiring developers to manually pass `info=info` to repository methods, plus critical security updates.

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

### Security

#### CVE Patches (Issue #202)

Applied security updates to Docker base images, resolving 3 CVEs:

**Fixed CVEs:**
- **CVE-2025-14104** (util-linux) - Heap buffer overread vulnerability
- **CVE-2025-6141** (ncurses) - Stack buffer overflow vulnerability
- **CVE-2024-56433** (shadow-utils) - Subordinate ID configuration issue

**Docker Updates:**
- Updated `python:3.13-slim` base image to latest
- Added `apt-get upgrade -y` to both builder and runtime stages
- Updated version labels to 1.9.1 across all Dockerfiles
- Added CVE fix documentation in image labels

**Impact:**
- ✅ Government-grade security compliance restored
- ✅ Zero CRITICAL/HIGH vulnerabilities in production images
- ✅ Meets FedRAMP, NIS2, and ISO 27001 requirements

**Files Updated:**
- `deploy/docker/Dockerfile` - Standard production image
- `deploy/docker/Dockerfile.hardened` - Government-grade security

**Unfixed (Monitoring):**
- CVE-2025-9820 (GnuTLS) - No patch available yet, under active monitoring

### Fixed

#### Rust Benchmarks (core_benchmark.rs)

Fixed compilation errors preventing Rust performance benchmarks from running:

**Issues Resolved**:
- Missing `max_depth` field in TransformConfig initializations (3 occurrences)
- Transformer mutability errors (added `mut` keyword for transform_bytes calls)
- Broken `byte_reader_parsing` test (incorrect JSON parsing logic)

**Performance CI**:
- Added Python 3.13 setup for PyO3 C API linking
- Changed from `cargo bench --all` to `cargo bench --bench core_benchmark`
- Focus on PyO3-independent benchmarks for reliable CI execution

**Benchmark Results** (all passing):
- ✅ zero_copy_small: 2.7µs (327 MiB/s throughput)
- ✅ zero_copy_medium: 62.7µs (408 MiB/s throughput)
- ✅ zero_copy_large: 5.56ms (481 MiB/s throughput)
- ✅ components/arena_allocation: 4.5ns
- ✅ components/byte_reader_parsing: 6.4ns
- ✅ components/snake_to_camel: 21.7ns

**Files Modified**:
- `fraiseql_rs/benches/core_benchmark.rs` - Fixed 3 compilation errors
- `.github/workflows/performance.yml` - Added Python setup, simplified workflow

#### Middleware Testing

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
