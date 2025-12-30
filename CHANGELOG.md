# Changelog

All notable changes to FraiseQL will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.9.0] - 2025-12-30

**Major Release - WHERE Clause Fixes + Rust Safety Improvements**

### Added

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
