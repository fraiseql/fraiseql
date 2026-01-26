# Phase 15, Cycle 1 - RED: API Stability & Backward Compatibility Requirements

**Date**: March 17-21, 2026
**Phase Lead**: API Lead + Product Lead
**Status**: RED (Defining API Stability Requirements)

---

## Objective

Define comprehensive API stability and backward compatibility requirements for FraiseQL v2, establishing semantic versioning, deprecation procedures, breaking change policy, and long-term support strategy to provide users with confidence and predictability.

---

## Background: Current State

From Phase 13-14:
- ✅ Security: Enterprise-grade implementation
- ✅ Operations: Production-ready procedures
- ✅ Framework: Core engine complete

**Critical Need**: API stability guarantees
- Users need to know: Will my code break in the next version?
- Users need to know: How long is this version supported?
- Users need to know: What's the deprecation timeline?

---

## Semantic Versioning (SemVer)

### Version Format: MAJOR.MINOR.PATCH

**Current**: v2.0.0

**Rules**:

```
MAJOR (2.x.x): Breaking changes
  - Changed API signature
  - Removed public function/trait
  - Changed behavior in incompatible way
  - Example: 2.0.0 → 3.0.0

MINOR (2.X.x): New features, backward compatible
  - New public API
  - Deprecated API (with warning)
  - Performance improvements
  - Example: 2.0.0 → 2.1.0

PATCH (2.x.X): Bug fixes, backward compatible
  - Security fixes
  - Bug fixes
  - Documentation fixes
  - Example: 2.0.0 → 2.0.1
```

### Examples

**Breaking Changes (MAJOR bump)**:
```rust
// v2.0.0
pub async fn execute_query(query: &str) -> Result<QueryResult, Error>

// v3.0.0 (breaking - signature changed)
pub async fn execute_query(
    query: &str,
    timeout_ms: u64  // NEW REQUIRED PARAMETER
) -> Result<QueryResult, ExecutionError>  // DIFFERENT ERROR TYPE

// Users must update their code
```

**New Feature (MINOR bump)**:
```rust
// v2.0.0
pub async fn execute_query(query: &str) -> Result<QueryResult, Error>

// v2.1.0 (new API, backward compatible)
pub async fn execute_query(query: &str) -> Result<QueryResult, Error>  // unchanged

pub async fn execute_query_with_timeout(
    query: &str,
    timeout_ms: u64
) -> Result<QueryResult, Error>  // NEW, users can opt-in

// Old code continues to work
```

**Bug Fix (PATCH bump)**:
```rust
// v2.0.0
pub fn validate_query(query: &str) -> bool {
    // BUG: Returns true for invalid queries in edge case
}

// v2.0.1 (bug fix, backward compatible)
pub fn validate_query(query: &str) -> bool {
    // FIXED: Now correctly rejects invalid queries
}

// Users update automatically for fix
```

---

## API Stability Guarantees

### Stable APIs (Won't Break)

**Rust Crate APIs** (public functions, traits, structs):
- Guarantee: No breaking changes within MAJOR version
- Timeline: 2-3 years per MAJOR version (see support policy)
- Example: `fraiseql::schema::CompiledSchema` in v2.x is stable

**GraphQL Execution** (query language):
- Guarantee: GraphQL syntax won't break (GraphQL spec is stable)
- Can add new directives, but won't remove old ones
- Performance improvements without behavior changes

**Configuration Format**:
- Guarantee: Config file format won't break
- Can add optional fields, but required fields stay same
- Major changes get migration guide

### Unstable APIs (May Break)

**Marked `#[unstable]`**:
```rust
#[unstable(feature = "experimental_caching", issue = "123")]
pub async fn with_cache(schema: &CompiledSchema) -> CompiledSchema {
    // This may break in minor versions
    // Users must opt-in to experimental features
    // Get advance notice 1-2 releases before removal
}
```

**Beta Features**:
- Clearly labeled in docs
- May break in minor versions
- Document in CHANGELOG
- Provide migration guide

**Internal APIs** (not public):
- Can break without warning
- Examples: private functions, internal modules

---

## Deprecation Procedure

### Timeline: 3-Release Deprecation Window

```
Release N: Feature X introduced (e.g., v2.1.0)

Release N+1: Feature Y introduced (e.g., v2.2.0)
            Feature X marked deprecated
            Warning added: "X will be removed in v2.4.0"
            Docs updated with migration path

Release N+2: Feature Z introduced (e.g., v2.3.0)
            Feature X still works (but warns on use)
            Advance notice: "Removing in next release"

Release N+3: Feature removed (e.g., v2.4.0)
            Feature X no longer available
            Users must have migrated by now
```

### Deprecation Warnings

**In Code**:
```rust
#[deprecated(
    since = "2.1.0",
    note = "Use `new_function()` instead"
)]
pub fn old_function() -> String {
    // Still works, but Rust compiler warns users
}
```

**In Docs**:
```markdown
### execute_query_sync() - DEPRECATED

⚠️ Deprecated since v2.1.0. Removed in v2.4.0.

**Use instead**: [`execute_query()`] (async version)

**Migration**:
```rust
// Before (v2.0.x)
let result = schema.execute_query_sync(query);

// After (v2.1.0+)
let result = schema.execute_query(query).await;
```
```

**In CHANGELOG**:
```markdown
## v2.1.0 (2026-04-15)

### Deprecated
- ⚠️ `execute_query_sync()` - Deprecated, will be removed in v2.4.0
  Use `execute_query()` (async) instead
```

---

## Breaking Change Policy

### When Breaking Changes Are Allowed

**Only in MAJOR version releases** (v2.0 → v3.0):
- Major architectural improvements
- Security fixes that require API changes
- Simplifying complex APIs
- Example: GraphQL spec upgrades

### How to Handle Breaking Changes

**1. Announce Early** (3+ months before release):
- Post RFC (Request for Comments) in GitHub Discussions
- Get community feedback
- Document rationale for breaking change
- Example: "We're changing error handling in v3.0 because X"

**2. Provide Migration Guide**:
```markdown
# Migration Guide: v2.x → v3.0

## Breaking Changes

### 1. Error Types Changed
**v2.x:**
```rust
pub enum QueryError {
    Parse(String),
    Database(String),
}

**v3.0:**
```rust
pub enum QueryError {
    Parse { message: String, location: Option<Location> },
    Database { message: String, code: String },
    Validation { message: String, path: Vec<String> },
}

**Migration:**
Change all error handling to use new types:
```rust
// Before
match error {
    QueryError::Parse(msg) => println!("{}", msg),
    QueryError::Database(msg) => println!("{}", msg),
}

// After
match error {
    QueryError::Parse { message, .. } => println!("{}", message),
    QueryError::Database { message, .. } => println!("{}", message),
    QueryError::Validation { message, .. } => println!("{}", message),
}
```

### 2. Async Requirement
...
```

**3. Provide Upgrade Path**:
- Clear migration steps
- Code examples for each breaking change
- Expected effort (small/medium/large)
- Tools to help (if applicable)

**4. Timing**:
- Release MAJOR versions infrequently (2-3 years apart)
- Give 6-12 months notice before breaking change
- Provide 2-3 months of support overlap (v2.x and v3.0 supported simultaneously)

---

## Long-Term Support (LTS) Policy

### Version Support Matrix

```
Version  | Release    | LTS Start | LTS End    | Status
---------|------------|-----------|------------|--------
v1.x     | 2025-06    | 2025-10   | 2027-06    | Maintenance
v2.0-2.x | 2026-03    | 2027-03   | 2029-03    | Current
v3.0+    | 2028-06    | 2029-06   | 2031-06    | Future
```

### Support Levels

**Current Release** (v2.0-2.x):
- All bug fixes
- All security fixes
- All performance improvements
- New features (MINOR releases)
- Support duration: 3 years

**LTS Release** (Long-Term Support):
- Critical bug fixes only
- All security fixes
- No new features
- Maintenance releases (PATCH only)
- Support duration: Extended 2+ years after Current ends

**End of Life** (EOL):
- No support
- Users should upgrade

### Example: v2.x Timeline

```
2026-03: v2.0.0 released (Current)
2026-09: v2.1.0 released (Current)
2027-03: v2.x becomes LTS (3-year current support starts)
2029-03: v2.x becomes EOL (LTS support phase ends)
```

---

## API Compatibility Testing

### What Won't Break

**Public API Surface**:
```rust
// These signatures won't change in v2.x
pub struct CompiledSchema { ... }
pub impl CompiledSchema {
    pub fn new(path: &str) -> Result<Self, Error> { ... }
    pub async fn execute(&self, query: &str) -> Result<String, Error> { ... }
}

pub enum QueryError { ... }
pub type Result<T> = std::result::Result<T, QueryError>;
```

**Behavior Contracts**:
```rust
// These contracts are guaranteed:
// - execute() will return same result for same input
// - Error types will be consistent
// - No silent data loss
// - Performance improvements only (not regressions)
```

### Test Strategy

**Backward Compatibility Test Suite**:
```rust
#[test]
fn test_v2_0_schema_still_works_in_v2_1() {
    // Load a schema created in v2.0.0
    let schema = CompiledSchema::from_file("test_data/v2.0.0/schema.json");

    // Execute query from v2.0.0
    let result = schema.execute("query { users { id name } }");

    // Result should match v2.0.0 behavior
    assert!(result.is_ok());
}

#[test]
fn test_error_types_match_contract() {
    // Verify error types match documented contract
    let error = schema.execute("invalid query");

    match error {
        Err(QueryError::Parse { .. }) => {},
        _ => panic!("Expected Parse error"),
    }
}
```

---

## API Design Review Process

### Before Releasing New API

**Review Checklist**:

1. **Naming**
   - [ ] Function names use standard Rust conventions (snake_case)
   - [ ] Type names use PascalCase
   - [ ] Names match their purpose
   - [ ] No abbreviations (use full words)

2. **Signatures**
   - [ ] Take references where possible (avoid unnecessary copies)
   - [ ] Use Result<T, Error> for fallible operations
   - [ ] Document panic conditions (or use Result)
   - [ ] Generic parameters have meaningful bounds

3. **Documentation**
   - [ ] Every public item has doc comment
   - [ ] Doc comment includes examples
   - [ ] Error types documented
   - [ ] Performance characteristics documented

4. **Stability**
   - [ ] Is this truly stable? (will it change?)
   - [ ] Mark unstable if experimental
   - [ ] Document any future plans to change

5. **Consistency**
   - [ ] Matches patterns used elsewhere in crate
   - [ ] Follows Rust API guidelines
   - [ ] Consistent error handling
   - [ ] Consistent async/sync patterns

**Example Review**:

```rust
// ❌ Poor API design
pub fn execute(q: &str) -> Result<String, Box<dyn std::error::Error>> {
    // Too generic error, unclear what can fail
    // Abbreviation "q", not clear
    // String result not informative
}

// ✅ Good API design
/// Execute a GraphQL query against this schema.
///
/// # Arguments
/// * `query` - GraphQL query string
///
/// # Returns
/// A structured query result with metadata
///
/// # Errors
/// Returns `QueryError::Parse` if query syntax is invalid
/// Returns `QueryError::Validation` if query fails validation
/// Returns `QueryError::Execution` if execution fails
///
/// # Example
/// ```ignore
/// let schema = CompiledSchema::from_file("schema.json")?;
/// let result = schema.execute("query { users { id } }").await?;
/// println!("{}", result.to_json());
/// ```
pub async fn execute(&self, query: &str) -> Result<QueryResult, QueryError> {
    // Clear parameter names
    // Specific error types
    // Structured result type
    // Documented and exemplified
}
```

---

## Communication & Documentation

### Version Announcement

**For Each MAJOR Release**:
```markdown
# Announcing FraiseQL v3.0 (Est. 2028-06-15)

## What's New
- [Feature 1]
- [Feature 2]
- [Feature 3]

## Breaking Changes
- [Breaking change 1]
- [Breaking change 2]

## Migration Guide
See [MIGRATION_GUIDE.md](link)

## Timeline
- 2027-06: RFC period (get feedback)
- 2027-09: Beta release (test)
- 2028-03: Release candidate (final testing)
- 2028-06: v3.0.0 released
- 2028-06-2028-12: v2.x and v3.0 both supported
- 2028-12: v2.x end of life
```

**For Each MINOR Release**:
```markdown
# FraiseQL v2.3.0 Released

## New Features
- New API: `with_cache()`
- New: Query planning mode
- Improvement: 20% faster parsing

## Deprecated
- ⚠️ `old_api()` - will be removed in v2.5.0

## Bug Fixes
- Fixed: Query complexity calculation edge case
- Fixed: Memory leak in result caching

## Upgrade
`cargo update fraiseql`
```

**For Each PATCH Release**:
```markdown
# FraiseQL v2.2.1 Released (Security)

## Security
- Fixed: SQL injection in field validation (CVE-2026-XXXXX)

## Bug Fixes
- Fixed: Crash when executing empty query
- Fixed: Error message text encoding

## Upgrade (Recommended)
`cargo update fraiseql`
```

### Documentation Updates

**For New APIs**:
- [ ] Add to API reference
- [ ] Add example to getting started guide
- [ ] Add to changelog
- [ ] Add blog post (if major feature)

**For Deprecated APIs**:
- [ ] Mark deprecated in code
- [ ] Update API reference with deprecation notice
- [ ] Add migration guide
- [ ] Update changelog
- [ ] Consider blog post explaining why

**For Breaking Changes**:
- [ ] RFC discussion (3 months)
- [ ] Migration guide (detailed examples)
- [ ] Announcement blog post
- [ ] Email notification to users
- [ ] Update all documentation

---

## Success Criteria (Phase 15, Cycle 1 - RED)

- [x] Semantic versioning defined (MAJOR.MINOR.PATCH)
- [x] API stability guarantees documented
- [x] Deprecation procedure (3-release window)
- [x] Breaking change policy established
- [x] Long-term support timeline defined (3 years current, +2 LTS)
- [x] API design review checklist created
- [x] Backward compatibility testing strategy documented
- [x] Communication plan for releases documented
- [x] Version support matrix defined
- [x] Migration guide template created

---

**RED Phase Status**: ✅ READY FOR IMPLEMENTATION
**Ready for**: GREEN Phase (Implement Policies & First Release)
**Target Date**: March 17-21, 2026

