# FraiseQL Versioning Strategy

**Date:** January 2026
**Status:** Complete System Specification
**Audience:** Framework architects, platform engineers, enterprise operators, SDK maintainers

---

## Executive Summary

FraiseQL uses **semantic versioning (MAJOR.MINOR.PATCH)** with explicit breaking change policies to balance innovation with stability. The versioning strategy covers five distinct versioning dimensions:

1. **Framework versioning** (FraiseQL runtime version)
2. **Schema versioning** (user-defined schema evolution)
3. **Compiled schema versioning** (internal IR evolution)
4. **Query API versioning** (GraphQL schema evolution)
5. **Error code versioning** (stable error taxonomy)

**Core principle**: FraiseQL commits to **3-year stability windows** for MAJOR versions. Framework versions can change freely; user schemas are backward-compatible within a MAJOR version.

---

## 1. Semantic Versioning (SemVer 2.0.0)

FraiseQL adheres to semantic versioning with three-component version numbers:

### 1.1 Version Format

```
MAJOR.MINOR.PATCH
  |      |      |
  |      |      └── Bug fixes and patches (no breaking changes)
  |      └────────── Features and improvements (backward-compatible)
  └──────────────── Breaking changes (incompatible with prior MAJOR version)
```

### 1.2 Version Examples

```
2.0.0   → Framework v2, first release
2.1.0   → Add new feature, backward-compatible with 2.0.x
2.1.1   → Bug fix, backward-compatible with 2.1.0
3.0.0   → Incompatible changes, requires migration from 2.x
```

### 1.3 Pre-release Versions

For beta testing and early access:

```
2.0.0-beta.1    → Beta version, may have breaking changes
2.0.0-rc.1      → Release candidate, likely stable
2.0.0-rc.2      → Second RC before GA
2.0.0            → General Availability (stable)
```

**Stability commitment:**
- ❌ **Never** use pre-release versions in production
- ✅ **Can** use for testing and feedback
- ✅ **Will** provide migration guide before GA
- ✅ **Will** announce breaking changes in pre-release changelog

---

## 2. Breaking Change Policy

### 2.1 What Constitutes a Breaking Change

A breaking change is **any modification that requires code changes in user schemas or client applications**. These changes trigger a MAJOR version bump.

#### 2.1.1 GraphQL Schema Breaking Changes

**Removals** (require MAJOR bump):
```graphql
# ❌ BREAKING: Remove a field
# 1.x
type User {
  id: ID!
  name: String!
  email: String   # Removing this field
}

# Would require 2.0.0
```

**Behavioral changes** (require MAJOR bump):
```graphql
# ❌ BREAKING: Change return type
# 1.x
type Query {
  user(id: ID!): User
}

# 2.x would return User | null → User! (non-null)
# Clients that didn't handle null must update code
```

**Argument changes** (require MAJOR bump):
```graphql
# ❌ BREAKING: Add required argument
# 1.x
type Query {
  posts: [Post!]!
}

# 2.x
type Query {
  posts(limit: Int!): [Post!]!  # New required argument
}
```

**Input type changes** (require MAJOR bump):
```graphql
# ❌ BREAKING: Add required field to input
# 1.x
input CreateUserInput {
  name: String!
  email: String
}

# 2.x
input CreateUserInput {
  name: String!
  email: String!  # Now required
  roles: [String!]!  # New required field
}
```

**Enum value removal** (require MAJOR bump):
```graphql
# ❌ BREAKING: Remove enum value
# 1.x
enum Role {
  ADMIN
  USER
  GUEST
}

# 2.x removes GUEST
```

#### 2.1.2 Operator Changes Breaking Changes

**Removing operators** (require MAJOR bump):
```python
# ❌ BREAKING: Remove an operator
# 1.x supports: eq, ne, gt, gte, lt, lte, in, nin, contains, regex
# 2.x removes: regex (for performance reasons)
# Queries using 'regex' operator fail

# Users must rewrite queries using contains or migrate to database functions
```

**Changing operator semantics** (require MAJOR bump):
```python
# ❌ BREAKING: Change operator behavior
# 1.x: in operator is case-sensitive
# 2.x: in operator is case-insensitive (SQL ILIKE)
# Queries that relied on case-sensitivity break
```

#### 2.1.3 Authorization Changes Breaking Changes

**Removing authorization rules** (require MAJOR bump):
```python
# ❌ BREAKING: Remove field-level masking
# 1.x: User.ssn field masked for non-admins
# 2.x: Remove masking (now exposed to everyone)
# Security expectations break; clients may violate compliance

# This is a MAJOR version change with security implications
```

**Adding required authorization rules** (require MAJOR bump):
```python
# ❌ BREAKING: Add row-level security that filters results
# 1.x: Query returns all posts
# 2.x: Only return posts by current user
# Queries that expected all posts now get fewer results
```

#### 2.1.4 Error Code Changes Breaking Changes

**Removing error codes** (require MAJOR bump):
```python
# ❌ BREAKING: Error code E_VALIDATION_EMAIL_001 removed
# 1.x: query fails with E_VALIDATION_EMAIL_001
# 2.x: Different error code or different error format
# Client error handling breaks
```

**Changing error code semantics** (require MAJOR bump):
```python
# ❌ BREAKING: Change what E_DB_POSTGRES_DEADLOCK_303 means
# 1.x: Means database deadlock (retry with exponential backoff)
# 2.x: Now means connection timeout (retry with circuit breaker)
# Client retry logic becomes ineffective
```

**Note**: Error codes are part of the contract and **never** change within a MAJOR version.

#### 2.1.5 Compilation-Time Changes Breaking Changes

**Changing compiled schema structure** (require MAJOR bump):
```
# ❌ BREAKING: Compiled schema JSON structure changes
# 1.x CompiledSchema:
{
  "version": "2.0.0",
  "types": {...},
  "operations": {...}
}

# 2.x CompiledSchema changes structure:
{
  "framework_version": "2.0.0",
  "schema_version": 1,
  "entities": {...},  # Renamed from "types"
  "queries": {...}    # Renamed from "operations"
}

# Runtime cannot load 2.x schemas if built for 1.x framework
```

#### 2.1.6 Type System Breaking Changes

**Removing a custom scalar** (require MAJOR bump):
```graphql
# ❌ BREAKING: Remove custom scalar
# 1.x
scalar DateTime
scalar JSON
type Event {
  timestamp: DateTime!
  metadata: JSON
}

# 2.x removes DateTime scalar
# Queries fail; schemas using DateTime cannot compile
```

**Changing scalar serialization** (require MAJOR bump):
```graphql
# ❌ BREAKING: Change how UUID is serialized
# 1.x: UUID serialized as "f47ac10b-58cc-4372-a567-0e02b2c3d479"
# 2.x: UUID serialized as "f47ac10b58cc4372a5670e02b2c3d479" (no hyphens)
# Clients parsing UUID strings break
```

### 2.2 Non-Breaking Changes

These changes are safe within the same MAJOR version:

#### 2.2.1 Safe Additions (MINOR version bump)

**Adding new fields** (backward-compatible):
```graphql
# ✅ SAFE: Add optional field
# 1.x
type User {
  id: ID!
  name: String!
  email: String
}

# 1.1 (MINOR bump)
type User {
  id: ID!
  name: String!
  email: String
  phone: String           # New optional field
  verified_at: DateTime   # New optional field
}
```

**Adding new types** (backward-compatible):
```graphql
# ✅ SAFE: Add new type and query
# 1.x types: User, Post, Comment

# 1.1 adds: Product type and products query
# Existing clients unaffected
```

**Adding new enum values** (backward-compatible):
```graphql
# ✅ SAFE: Add enum value (if clients ignore unknown values)
# 1.x
enum Role {
  ADMIN
  USER
}

# 1.1
enum Role {
  ADMIN
  USER
  SUPER_ADMIN  # New enum value
}

# Clients that don't use SUPER_ADMIN are unaffected
```

**Adding optional arguments** (backward-compatible):
```graphql
# ✅ SAFE: Add optional argument
# 1.x
type Query {
  posts: [Post!]!
}

# 1.1
type Query {
  posts(limit: Int, offset: Int): [Post!]!
}

# Existing queries without arguments still work
```

**Adding new operators** (backward-compatible):
```python
# ✅ SAFE: Add new operator
# 1.x supports: eq, ne, gt, gte, lt, lte, in, nin, contains

# 1.1 adds: startsWith, endsWith, regex
# Existing queries work unchanged
```

**Adding field-level masking** (backward-compatible):
```python
# ✅ SAFE: Add masking that previously wasn't masked (restricts data, not expands it)
# 1.x: User.ssn visible to everyone
# 1.1: User.ssn now masked for non-admins (returns null for regular users)
# Admin clients still see ssn; regular clients see null (which is safe)
```

**Expanding authorization** (backward-compatible):
```python
# ✅ SAFE: Make row-level security more restrictive (fewer results is safe)
# 1.x: Query returns posts from all users
# 1.1: Query now only returns current user's posts
# Results are filtered but authorization is stricter (more secure)
```

**Adding new error codes** (backward-compatible):
```python
# ✅ SAFE: Add new error codes (clients ignore codes they don't recognize)
# 1.x error codes: E_VALIDATION_*, E_AUTH_*, E_DB_*
# 1.1 adds: E_RATE_LIMIT_* (new category)
# Existing error handling still works; clients can add handling for new codes
```

#### 2.2.2 Safe Modifications (PATCH version bump)

**Performance improvements** (patch):
```
# ✅ SAFE: Query execution faster, same semantics
# 1.0.0 → 1.0.1: Database query optimized from 100ms to 50ms
# Behavior unchanged; only performance changes
```

**Bug fixes** (patch):
```
# ✅ SAFE: Fix incorrect behavior to match specification
# 1.0.0 had a bug: "in" operator case-sensitive despite spec saying case-insensitive
# 1.0.1: Fix bug, "in" operator now case-insensitive per spec
# Note: This is a bug fix (behavior was wrong), not a breaking change
```

**Documentation updates** (patch):
```
# ✅ SAFE: Documentation corrections, no code changes
# 1.0.0 → 1.0.1: Update docs for clarity
```

**Internal refactoring** (patch):
```
# ✅ SAFE: Rewrite internals without changing external behavior
# 1.0.0 → 1.0.1: Rewrite Rust pipeline for performance
# Compiled schema output identical; only internals change
```

---

## 3. Deprecation Policy

### 3.1 Deprecation Lifecycle

FraiseQL follows a **three-phase deprecation lifecycle** before removal:

```
ANNOUNCEMENT (Minor Version N)
     ↓
DEPRECATION (Minor Versions N to N+3)
     ↓
REMOVAL (Major Version M+1)
```

### 3.2 Deprecation Timeline (3-Year Stability Window)

FraiseQL commits to a **3-year support window** for each MAJOR version:

```
v2.0.0 Released (Year 0)
       ├─ v2.1.0 (Year 0, Q2) - Add new feature, announce deprecation
       ├─ v2.2.0 (Year 0, Q4) - Feature fully deprecated
       ├─ v2.3.0 (Year 1, Q2) - Still deprecated, but working
       ├─ v2.4.0 (Year 1, Q4) - Still deprecated, but working
       ├─ v2.5.0 (Year 2, Q2) - Last MINOR version of v2.x
       ├─ v2.6.0 (Year 2, Q4) - Still working
       └─ v2.x.x (Year 3)      - Last day of v2 support (Dec 31, Year 2)

v3.0.0 Released (Year 3)
       └─ v2.x.x no longer supported (Jan 1, Year 3)
```

### 3.3 Deprecation Announcement Format

When a feature is deprecated, the changelog includes:

```markdown
### v2.1.0 (Deprecation Announcement)

#### Deprecated
- **`regex` operator**: Use `contains` operator instead. Will be removed in v2.5.0.
  - **Reason**: Regex performance overhead; most use cases better served by `contains`
  - **Migration**: Replace `{name: {regex: "/pattern/"}}` with `{name: {contains: "pattern"}}`
  - **Timeline**: Deprecated v2.1, removal in v3.0 (3-year window)
  - **Help**: See migration guide: https://docs.fraiseql.io/migration/v2.1-regex-deprecation

- **Field-level masking via `@mask` decorator**: Use row-level security via `@authorize` instead.
  - **Reason**: @authorize more expressive; @mask conflates field-level with row-level
  - **Timeline**: Deprecated v2.1, removal in v3.0
  - **Help**: See migration guide: https://docs.fraiseql.io/migration/v2.1-mask-deprecation
```

### 3.4 Deprecation Warning in Runtime

When deprecated features are used:

```graphql
# Query uses deprecated regex operator
query GetPosts {
  posts(where: { title: { regex: "/draft/" } }) {
    id
    title
  }
}
```

**Response includes deprecation warning:**

```json
{
  "data": {
    "posts": [...]
  },
  "extensions": {
    "deprecations": [
      {
        "message": "Operator 'regex' is deprecated. Use 'contains' instead. Will be removed in v3.0.",
        "code": "W_DEPRECATED_OPERATOR_REGEX",
        "removal_version": "3.0.0",
        "migration_url": "https://docs.fraiseql.io/migration/v2.1-regex-deprecation",
        "location": {
          "line": 2,
          "column": 45
        }
      }
    ]
  }
}
```

### 3.5 Migration Guides

For each deprecation, FraiseQL provides:

1. **Why** — Business/technical reason for deprecation
2. **Impact** — Which features/users affected
3. **How to migrate** — Step-by-step migration instructions
4. **Timeline** — When removed
5. **Help** — Link to detailed guide and support

**Example migration guide structure:**
```
docs/migration/
├── v2.1-regex-deprecation.md
├── v2.2-field-masking.md
├── v2.3-custom-scalars.md
└── v3.0-breaking-changes.md
```

---

## 4. Schema Versioning

### 4.1 User-Defined Schema Evolution

Users write schemas in Python (or YAML/TypeScript). These schemas are **versioned separately** from the FraiseQL framework.

#### 4.1.1 User Schema Version Format

```python
@fraiseql.version("1.0.0")
@fraiseql.type
class User:
    """Version 1.0.0 of User type"""
    id: ID
    name: str
```

**User schema versions are independent of framework version:**

```
Framework v2.0.0
└─ User schema v1.0.0
   └─ Posts schema v2.5.0
   └─ Comments schema v1.3.0

Framework v2.1.0 (backward-compatible)
└─ User schema v1.0.0 (unchanged)
   └─ Posts schema v2.5.0 (unchanged)
   └─ Comments schema v1.3.0 (unchanged)

Framework v3.0.0 (breaking changes)
└─ User schema v2.0.0 (may need updates for new framework)
   └─ Posts schema v3.0.0
   └─ Comments schema v2.0.0
```

#### 4.1.2 Schema Backward Compatibility

**Within the same framework MAJOR version:**

FraiseQL enforces backward-compatible schema changes:

```python
# v1.0.0
@fraiseql.type
class User:
    id: ID
    name: str
    email: str | None = None

# v1.1.0 (backward-compatible with 1.0.0)
@fraiseql.type
class User:
    id: ID
    name: str
    email: str | None = None
    phone: str | None = None       # New optional field
    created_at: datetime | None = None  # New optional field
```

**Between framework MAJOR versions:**

Schema changes may require migration:

```python
# Framework v2.x
@fraiseql.type
class User:
    id: ID
    name: str
    email: str | None = None

# Framework v3.0 (breaking changes)
@fraiseql.type
class User:
    id: ID
    name: str
    email: str  # Now required (breaking change in User schema)
    profile: UserProfile  # New required nested type
```

### 4.2 Compiled Schema Versioning

The **compiled schema** (internal IR) is versioned separately from user schemas:

```json
{
  "framework_version": "2.0.0",
  "compiled_schema_version": 1,
  "types": {...},
  "operations": {...},
  "federation": {...},
  "subscriptions": {...}
}
```

#### 4.2.1 Compiled Schema Format Evolution

Changes to compiled schema JSON format trigger framework MAJOR version bump:

```json
# Framework 2.x compiled schema
{
  "version": "2.0.0",
  "types": {...}
}

# Framework 3.0 compiled schema (different format)
{
  "framework_version": "3.0.0",
  "compiled_schema_version": 2,
  "entities": {...}  # Renamed from "types"
}
```

**Impact**: Compiled schemas are not portable across major framework versions.

#### 4.2.2 Runtime Schema Loading

The runtime includes the compiled schema **at build time**:

```
User defines schema.py
       ↓
Compiler compiles to compiled-schema.json
       ↓
Runtime packages compiled-schema.json with code
       ↓
Runtime starts with built-in schema (no compilation at runtime)
```

**Schema is immutable at runtime** — no runtime recompilation.

---

## 5. Query API Versioning

### 5.1 GraphQL Schema Versioning

The **GraphQL schema** (what clients see) is version-locked with the framework:

```graphql
# FraiseQL v2.0.0
type Query {
  user(id: ID!): User
}

# All queries compile against THIS schema
# Schema is part of framework, not separately versioned
```

### 5.2 Query Compatibility

**Within same framework version (e.g., v2.0 to v2.4):**

Queries remain compatible:

```graphql
# Query written for v2.0.0
query GetUser {
  user(id: "123") {
    id
    name
    email
  }
}

# Same query works on v2.1.0, v2.2.0, v2.3.0, v2.4.0
# New optional fields added in v2.1+, but this query unchanged
```

**Across framework MAJOR versions (e.g., v2.x to v3.0):**

Queries may break and require migration:

```graphql
# Query written for v2.x
query GetPosts {
  posts(filter: { author_id: "123" }) {
    id
    title
  }
}

# v3.0 changes filter syntax
query GetPosts {
  posts(where: { author: { id: "123" } }) {  # Changed syntax
    id
    title
  }
}
```

### 5.3 Query Validation

FraiseQL validates all queries at **compile time**, not runtime:

```python
# Compile-time: Query validation
schema = fraiseql.compile(schema_definition)

# Invalid queries caught during compilation, not at runtime
query = """
  query GetUser {
    user(id: "123") {
      nonexistent_field
    }
  }
"""

schema.validate(query)  # Raises CompilationError immediately
```

---

## 6. Error Code Versioning

### 6.1 Error Code Stability Guarantee

**Error codes are part of the contract and never change** within a MAJOR version.

#### 6.1.1 Error Code Format

Error codes follow deterministic format:

```
E_CATEGORY_SUBCATEGORY_NUMBER

E_VALIDATION_EMAIL_001      → Category: VALIDATION, Subcategory: EMAIL, Number: 001
E_DB_POSTGRES_DEADLOCK_303  → Category: DB, Subcategory: POSTGRES_DEADLOCK, Number: 303
E_AUTH_PERMISSION_401       → Category: AUTH, Subcategory: PERMISSION, Number: 401
E_FED_SUBGRAPH_TIMEOUT_502  → Category: FED, Subcategory: SUBGRAPH_TIMEOUT, Number: 502
```

#### 6.1.2 Error Code Stability Rules

**Within same MAJOR version:**

```python
# v2.0.0: Validation error for empty email
error_code = "E_VALIDATION_EMAIL_001"
message = "Email cannot be empty"

# v2.1.0: Same error has same code
error_code = "E_VALIDATION_EMAIL_001"
message = "Email cannot be empty"

# v2.5.0: Still same error code
error_code = "E_VALIDATION_EMAIL_001"
message = "Email cannot be empty"
```

**Error messages can change** (provide more details), but codes and semantics are locked.

#### 6.1.3 Adding New Error Codes

New error codes are safe to add in MINOR versions:

```python
# v2.0.0 error codes
E_VALIDATION_EMAIL_001    # Email cannot be empty
E_VALIDATION_EMAIL_002    # Email format invalid

# v2.1.0 adds new error code
E_VALIDATION_EMAIL_003    # Email already exists (new in v2.1)
```

**Client impact**: Clients that don't handle `E_VALIDATION_EMAIL_003` still work; they receive an error they didn't explicitly handle, which is backward-compatible.

#### 6.1.4 Removing Error Codes

Removing error codes is a **BREAKING CHANGE** requiring MAJOR version bump:

```python
# v2.x
E_VALIDATION_EMAIL_002    # Email format invalid

# Cannot be removed in v2.5 (would break clients handling this code)
# Can only be removed in v3.0 (MAJOR version)
```

**Migration**: Deprecate error code in v2.x, remove in v3.0.

#### 6.1.5 Changing Error Code Semantics

Changing what an error code means is a **BREAKING CHANGE**:

```python
# v2.0: E_DB_POSTGRES_TIMEOUT_304 = Query execution timeout
#       Client retries with exponential backoff

# v2.5: Change E_DB_POSTGRES_TIMEOUT_304 to mean Connection timeout
#       (different retry strategy needed)
# This breaks client error handling

# Solution: Create new error code E_DB_POSTGRES_CONN_TIMEOUT_305
#           Keep 304 for query timeout
```

---

## 7. Multi-Version Runtime Support

### 7.1 Can a Runtime Load Multiple Schema Versions?

**Short answer**: No, not at the same time.

**Long answer**: The runtime is **single-schema**. You select which compiled schema to load at startup:

```rust
// Rust runtime initialization
let compiled_schema = load_compiled_schema("v2.0.0");
let runtime = FraiseQLRuntime::new(compiled_schema);

// All queries execute against v2.0.0 schema
// Cannot mix v2.0 and v2.1 schemas in same runtime
```

### 7.2 Multiple Versions Across Multiple Instances

To run multiple versions simultaneously, deploy multiple runtime instances:

```
┌─────────────────────────────────┐
│ API Gateway                     │
└──────────┬──────────────────────┘
           │
           ├─→ Runtime Instance A (Framework v2.0.0)
           │   └─ Schema v1.0.0
           │
           ├─→ Runtime Instance B (Framework v2.1.0)
           │   └─ Schema v1.1.0
           │
           └─→ Runtime Instance C (Framework v2.4.0)
               └─ Schema v1.2.0
```

**Client routing**:
- Requests for v1.0.0 schema → Route to Instance A
- Requests for v1.1.0 schema → Route to Instance B
- Requests for v1.2.0 schema → Route to Instance C

### 7.3 Schema Migration Between Versions

To migrate from Framework v2.0 to v2.1:

```
1. Deploy new runtime instance with Framework v2.1
2. Route new requests to v2.1 instance
3. Keep v2.0 instance running for existing clients
4. Gradually migrate clients to v2.1
5. Once all clients migrated, shut down v2.0 instance
```

---

## 8. MAJOR Version Upgrade Path

### 8.1 Pre-Upgrade Checklist

Before upgrading from v2.x to v3.0:

```markdown
### v2.x → v3.0 Pre-Upgrade Checklist

- [ ] Review v3.0 breaking changes guide
  https://docs.fraiseql.io/migration/v3.0-breaking-changes

- [ ] Check which v2.x features are deprecated in v3.0
  [ ] No deprecated operators used
  [ ] No deprecated decorators used
  [ ] No deprecated error codes expected in error handling

- [ ] Update schemas for v3.0 requirements
  [ ] Update type definitions
  [ ] Update authorization rules
  [ ] Update error handling code

- [ ] Run schema validation
  $ fraiseql validate schema.py --target v3.0

- [ ] Test on staging environment
  $ fraiseql compile schema.py --target v3.0
  $ fraiseql test --schema compiled-schema-v3.json

- [ ] Review compiled schema changes
  $ fraiseql diff compiled-schema-v2.json compiled-schema-v3.json

- [ ] Update client code
  [ ] Handle new error codes
  [ ] Adjust for removed fields/operators
  [ ] Test error handling

- [ ] Plan deployment
  [ ] Deploy v3.0 runtime instance first (canary)
  [ ] Route test traffic to v3.0
  [ ] Monitor for errors
  [ ] Gradually migrate production traffic
  [ ] Keep v2.x instance for rollback (1-2 weeks)

- [ ] Performance testing
  [ ] Benchmark queries on v3.0
  [ ] Check for regressions
  [ ] Verify error handling performance
```

### 8.2 Version Coexistence Window

During migration from v2.x to v3.0:

```
Timeline:
  Week 0: v3.0 released
  Week 1: Deploy v3.0 canary, route 5% traffic
  Week 2: Increase to 25% traffic
  Week 3: Increase to 50% traffic
  Week 4: Increase to 75% traffic
  Week 5: Increase to 100% traffic
  Week 6: Keep v2.x for rollback (if needed)
  Week 7: Decommission v2.x instance

Duration: ~7 weeks for full migration
```

### 8.3 Rollback Window

If issues arise post-upgrade:

```
If critical issue detected within 48 hours of upgrade:
  → Can rollback to v2.x (keep v2.x instance running during migration)

If critical issue detected after 7 days:
  → Cannot safely rollback (data may have changed)
  → Must fix forward in v3.x
  → Will release v3.0.1 hotfix quickly
```

---

## 9. Client Versioning & Compatibility

### 9.1 Client Version Lock

Clients lock to a specific FraiseQL framework version:

```python
# Client code targeting FraiseQL v2.x
from fraiseql import Client

client = Client(
    endpoint="https://api.example.com",
    framework_version="2.0.0"  # Explicit version lock
)

# Query uses v2.0.0 semantics
response = client.execute("""
  query GetPosts {
    posts(filter: { published: true }) {
      id
      title
    }
  }
""")
```

### 9.2 Client Upgrade Path

When server upgrades framework:

```
Server runs v2.0.0
└─ Client sends queries
   └─ Server executes against v2.0.0 schema
   └─ Client receives responses

Server upgrades to v2.1.0 (backward-compatible)
└─ Same client queries work unchanged
   └─ New optional fields available (client ignores if not needed)

Server upgrades to v3.0.0 (breaking changes)
└─ Client queries may break
   └─ Need to update client code
   └─ Recompile client code for v3.0.0
```

### 9.3 Version Negotiation

Runtime can optionally advertise its version:

```graphql
# GraphQL introspection
{
  __schema {
    description  # "FraiseQL v2.1.0"
  }
}

# Via HTTP header
GET /graphql
Host: api.example.com
Accept: application/json

Response:
200 OK
X-FraiseQL-Version: 2.1.0
Content-Type: application/json
```

**Client usage**:

```python
# Check server version before executing query
version = response.headers.get('X-FraiseQL-Version')
if version.startswith('2.0'):
    # Execute v2.0 query
    ...
elif version.startswith('2.1'):
    # Can use v2.1 features
    ...
else:
    # Unsupported version
    raise IncompatibleVersionError(version)
```

---

## 10. Compiler Version Management

### 10.1 Schema Compilation

When users compile schemas, they specify target framework version:

```bash
# Compile for specific framework version
fraiseql compile schema.py --target 2.0.0

# Compile for latest version
fraiseql compile schema.py --target latest

# Compile with version compatibility check
fraiseql compile schema.py --target 2.0.0 --strict
```

### 10.2 Compiled Schema Portability

Compiled schemas are **not portable across framework versions**:

```
Compiled schema built with FraiseQL v2.0.0
    ↓
Runtime v2.0.0: ✅ Works
Runtime v2.1.0: ✅ Works (backward-compatible)
Runtime v3.0.0: ❌ Incompatible (different format)
```

**To upgrade**:

```bash
# 1. Recompile schema with new framework version
fraiseql compile schema.py --target 3.0.0

# 2. Deploy compiled schema and runtime v3.0 together
docker build -t my-api:v3 --build-arg SCHEMA=compiled-schema-v3.json .

# 3. Deploy new runtime instance
docker run my-api:v3
```

---

## 11. Ecosystem Versioning

### 11.1 SDK Versions

Client SDKs (Python, TypeScript, Go, Rust) are versioned separately from framework:

```
FraiseQL Framework: v2.1.0
Python SDK:        v2.1.0  (matches framework)
TypeScript SDK:    v2.1.1  (patch ahead for bug fixes)
Go SDK:            v2.0.5  (behind, still testing)
Rust SDK:          v2.1.0  (matches framework)
```

**Compatibility matrix**:

```
SDK v2.1.0 can connect to:
  - Framework v2.0.x ✅ (backward-compatible)
  - Framework v2.1.x ✅ (same version)
  - Framework v2.2+ ✅ (forward-compatible for read)
  - Framework v3.0 ❌ (breaking changes)
```

### 11.2 Tool Versions

Development tools have independent versions:

```
Framework:           v2.1.0
Compiler:            v2.1.0  (same as framework)
Migration tool:      v1.3.2  (separate versioning)
Schema validator:    v2.1.0  (same as framework)
Performance profiler: v1.0.0 (separate versioning)
```

---

## 12. Version Communication

### 12.1 Changelog Format

Every release includes a detailed changelog:

```markdown
# v2.1.0 Changelog

**Release Date:** January 15, 2024
**Framework:** FraiseQL v2.1.0
**Compatibility:** Backward-compatible with v2.0.x

## ✨ New Features

### Keyset Pagination Support
- Added `@cursor` directive for keyset-based pagination
- Supports stateless pagination for large result sets
- Example: `posts(first: 20, after: "cursor123") { id title }`

### New Operators
- Added `startsWith` operator for string filtering
- Added `endsWith` operator for string filtering
- Operators work with both `String` and `Text` types

## 🐛 Bug Fixes

- Fixed WHERE clause filtering on hybrid tables (Issue #124)
- Fixed deadlock detection in concurrent mutations
- Fixed error code E_VALIDATION_EMAIL_001 message formatting

## 🚨 Breaking Changes

- **None** - This is a backward-compatible MINOR release

## ⚠️ Deprecations

- `regex` operator: Use `contains` or `startsWith` instead
  - Deprecated in v2.1.0
  - Will be removed in v3.0.0 (3-year window)
  - Migration guide: https://docs.fraiseql.io/migration/regex-deprecation

## 📊 Performance

- Query execution 15% faster on average
- Memory usage reduced by 8%
- Compiled schema size reduced by 12%

## 🔒 Security

- Fixed potential SQL injection vector in LIKE queries
- Enhanced rate limiting for federation queries
- Improved authorization caching

## 🛠️ For Operators

- Upgraded PostgreSQL FDW support to 15.x
- Added support for SQL Server 2022
- Improved connection pooling (pgbouncer 1.18+)

## 📚 Documentation

- Added keyset pagination guide
- Updated federation architecture guide
- New error code reference
```

### 12.2 Migration Guides

For each MAJOR version upgrade, provide detailed guide:

```
docs/migration/
├── v2-to-v3.md                    # Main migration guide
├── v3-breaking-changes.md         # What breaks
├── v3-operator-changes.md         # Operator changes
├── v3-error-code-mapping.md       # How error codes changed
├── v3-schema-examples.md          # Before/after schema examples
├── v3-performance-guide.md        # Performance characteristics
└── v3-troubleshooting.md          # Common issues and solutions
```

### 12.3 Deprecation Announcements

Deprecations are announced prominently:

```markdown
# ⚠️ Deprecation Notice: `regex` Operator

**Announced in:** v2.1.0
**Removal planned:** v3.0.0
**Timeline:** 3 years from v2.1 release

## Why?
The `regex` operator provides functionality better served by `contains` with superior performance (10x faster on average). We're consolidating operators to reduce API surface and improve query performance.

## What to do?

### Before (v2.x)
```graphql
query SearchPosts {
  posts(where: { title: { regex: "/^draft/" } }) {
    id
    title
  }
}
```

### After (v2.1+)
```graphql
query SearchPosts {
  posts(where: { title: { startsWith: "draft" } }) {
    id
    title
  }
}
```

## Impact
- Affects ~3% of existing queries in telemetry
- No performance impact on other queries
- Removal affects only queries using `regex` operator

## Timeline
- **v2.1 (2024-01)**: Deprecation announced, `regex` still works, warnings enabled
- **v2.2-2.5 (2024-2025)**: `regex` still works, deprecation warnings continue
- **v3.0 (2027)**: `regex` operator removed, queries fail at compile time

## Help
- Migration guide: https://docs.fraiseql.io/migration/regex-deprecation
- Support: support@fraiseql.io
- Issues: https://github.com/fraiseql/fraiseql/issues
```

---

## 13. Support & Long-Term Maintenance

### 13.1 Support Window

FraiseQL commits to support windows for each MAJOR version:

```
v2.x (v2.0.0 released Date X)
  ├─ Active support: 2 years from release
  │  └─ Full features, bug fixes, security patches
  ├─ Maintenance support: 1 year after active support ends
  │  └─ Security patches only, no new features
  └─ End of life: 3 years from release
     └─ No support, no patches

v3.x (released 3 years after v2.0)
  ├─ Active support: 2 years from release
  └─ ...continues pattern...
```

### 13.2 Security Patches

Security vulnerabilities are backported to all supported versions:

```
Security vulnerability discovered in v3.0.0
  ├─ v3.0.1 released with patch (immediate)
  ├─ v2.5.3 released with patch (same day)
  ├─ v2.4.2 released with patch (same day)
  └─ v2.3.1 released with patch (same day)
```

### 13.3 End of Life (EOL) Handling

When a MAJOR version reaches end of life:

```markdown
# v2.x End of Life (December 31, 2026)

**Support ended:** January 1, 2027

Applications running v2.x will continue to function, but:
- ❌ No new security patches
- ❌ No bug fixes
- ❌ No technical support
- ✅ v2.x instances remain functional (no forced upgrades)

To continue receiving support:
1. Upgrade to v3.x or later
2. Follow upgrade guide: https://docs.fraiseql.io/migration/v2-to-v3

We recommend upgrading before December 31, 2026.
```

---

## 14. Version Decision Tree

Use this decision tree to determine if a change requires version bump:

```
Does the change modify user-facing behavior?
│
├─ NO
│  └─ Is it a code refactoring, performance improvement, or doc fix?
│     ├─ YES → PATCH version (v2.0.0 → v2.0.1)
│     └─ NO → No version bump (development/internal only)
│
└─ YES
   │
   └─ Does the change break existing queries, schemas, or code?
      │
      ├─ NO (addition, improvement, deprecation)
      │  └─ MINOR version (v2.0.0 → v2.1.0)
      │
      └─ YES (removal, incompatibility, behavior change)
         └─ MAJOR version (v2.0.0 → v3.0.0)
```

---

## 15. Versioning Best Practices

### 15.1 For Framework Maintainers

**DO:**
- ✅ Increment MAJOR version for breaking changes
- ✅ Increment MINOR version for new features (backward-compatible)
- ✅ Increment PATCH version for bug fixes
- ✅ Test against previous versions
- ✅ Document breaking changes prominently
- ✅ Provide migration guides
- ✅ Support for 3 years per MAJOR version
- ✅ Deprecate before removing (3 versions before removal)
- ✅ Lock error codes within MAJOR version

**DON'T:**
- ❌ Remove fields/operators without deprecation period
- ❌ Change error code semantics
- ❌ Use 0.x versioning forever (stabilize with 1.0)
- ❌ Break queries without MAJOR version bump
- ❌ Change compiled schema format within MAJOR version
- ❌ Support multiple schema versions in single runtime instance

### 15.2 For Application Developers

**DO:**
- ✅ Lock to specific framework version in production
- ✅ Test upgrades on staging first
- ✅ Review changelog before upgrading
- ✅ Plan migrations for MAJOR version upgrades
- ✅ Handle deprecated warnings in development
- ✅ Update error handling code for new error codes

**DON'T:**
- ❌ Use `latest` version in production
- ❌ Upgrade MAJOR versions without planning
- ❌ Ignore deprecation warnings
- ❌ Assume backward compatibility between MAJOR versions
- ❌ Run unsupported versions in production

---

## 16. Examples

### 16.1 Example: Adding New Field (MINOR Version)

**Scenario**: Add optional `phone` field to User type.

```python
# v2.0.0
@fraiseql.type
class User:
    id: ID
    name: str
    email: str | None = None

# v2.1.0 (add optional field)
@fraiseql.type
class User:
    id: ID
    name: str
    email: str | None = None
    phone: str | None = None  # New in v2.1

# Queries written for v2.0.0 still work in v2.1.0
# Clients can optionally request 'phone' field
```

**Version bump**: `2.0.0` → `2.1.0` (MINOR)

### 16.2 Example: Removing Operator (MAJOR Version)

**Scenario**: Remove `regex` operator.

```python
# v2.x
# Query with regex operator (works)
query GetPosts {
  posts(where: { title: { regex: "/draft/" } }) {
    id
  }
}

# v3.0
# Query with regex operator (ERROR)
query GetPosts {
  posts(where: { title: { regex: "/draft/" } }) {
    id
  }
}
# Error: "regex operator removed in v3.0, use startsWith instead"

# Require client update:
query GetPosts {
  posts(where: { title: { startsWith: "draft" } }) {
    id
  }
}
```

**Version bump**: `2.x.x` → `3.0.0` (MAJOR)

### 16.3 Example: Bug Fix (PATCH Version)

**Scenario**: Fix WHERE clause filtering on hybrid tables (Issue #124).

```python
# v2.0.0
# WHERE clause not applied to SQL columns on hybrid tables (BUG)

# v2.0.1
# WHERE clause correctly applied to SQL columns (FIXED)
```

**Version bump**: `2.0.0` → `2.0.1` (PATCH)

### 16.4 Example: Deprecation then Removal

```
Timeline:
  v2.1.0 (Year 0): Announce deprecation of 'regex' operator
  v2.2.0 (Year 0): Still works, warnings shown
  v2.3.0 (Year 1): Still works, warnings shown
  v2.4.0 (Year 1): Still works, warnings shown
  v2.5.0 (Year 2): Still works, warnings shown
  v3.0.0 (Year 3): Operator removed, queries fail

Migration required during 3-year window (v2.1 to v3.0).
```

---

## 17. Summary & Quick Reference

### 17.1 Versioning at a Glance

| Version Type | Use Case | Example | Support |
|--------------|----------|---------|---------|
| **MAJOR** | Breaking changes | 2.0.0 → 3.0.0 | 3 years |
| **MINOR** | New features, backward-compatible | 2.0.0 → 2.1.0 | Until v3 released |
| **PATCH** | Bug fixes, performance | 2.0.0 → 2.0.1 | Until v3 released |

### 17.2 Breaking Change Examples

**Requires MAJOR version bump:**
- Remove field from type
- Change field return type
- Remove operator
- Remove error code
- Change error code semantics
- Change compiled schema format
- Remove custom scalar
- Add required argument/input field

**Backward-compatible (MINOR version bump):**
- Add optional field
- Add new operator
- Add new type
- Add new enum value
- Add new error code
- Add optional argument

**Bug fixes and performance (PATCH version bump):**
- Fix incorrect behavior
- Improve performance
- Update documentation
- Internal refactoring

### 17.3 Deprecation & Support

- **Deprecation window**: 3 years (MAJOR version)
- **Support window per MAJOR**: 3 years (active + maintenance)
- **Error codes**: Locked within MAJOR version (never change)
- **Compiled schemas**: Require recompilation for MAJOR version change

---

## 18. Appendix: Version Checking

### 18.1 Programmatic Version Checks

```python
import fraiseql

# Get framework version
version = fraiseql.__version__
# Returns: "2.1.0"

# Check version compatibility
if fraiseql.version_matches(">=2.0.0,<3.0.0"):
    print("Running on v2.x")
else:
    print("Running on different MAJOR version")

# Get compiled schema version
schema = fraiseql.compile(...)
print(schema.framework_version)
# Returns: "2.1.0"

print(schema.compiled_schema_version)
# Returns: 1
```

### 18.2 GraphQL Introspection

```graphql
{
  __schema {
    types {
      name
      description  # May include version info
    }
  }
}
```

### 18.3 Runtime Endpoints

```bash
# Check framework version
curl https://api.example.com/version
# Returns: {"version": "2.1.0"}

# Get schema version
curl https://api.example.com/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ __schema { description } }"}'
```

---

**Document Version**: 1.0.0
**Last Updated**: January 2026
**Status**: Complete and frozen for framework v2.x

FraiseQL versioning ensures stability for users while allowing framework innovation. Three-year support windows, explicit breaking change policies, and comprehensive migration guides make upgrades predictable and manageable.
