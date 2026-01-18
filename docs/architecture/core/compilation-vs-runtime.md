# Compilation vs Runtime: Decision Authority Matrix

**Version:** 1.0
**Date:** January 11, 2026
**Status:** Complete
**Audience:** All FraiseQL developers, especially those working on compiler or runtime

---

## Purpose

This document establishes **when decisions are made** in FraiseQL: compile-time (static) vs runtime (dynamic).

Understanding this boundary is critical because:

- **Compile-time decisions** are fixed and cannot change during execution
- **Runtime decisions** are dynamic and determined per request
- **Incorrect placement** leads to architectural violations

---

## Core Principle

> **FraiseQL maximizes compile-time decisions to enable deterministic runtime behavior.**

**Why?**

- Compile-time errors are better than runtime errors
- Static analysis enables optimization
- Deterministic execution enables predictable caching and invalidation
- No user code means no runtime surprises

---

## Decision Authority Matrix

| Decision | When | Who Decides | Where Specified | Can Change at Runtime? |
|----------|------|-------------|-----------------|----------------------|
| **SCHEMA & TYPES** |
| What types exist in schema | Compile-time | Authoring schema | `authoring-contract.md` | ❌ No |
| What fields each type has | Compile-time | Authoring schema | `authoring-contract.md` | ❌ No |
| Field types (String, Int, etc.) | Compile-time | Authoring schema | `authoring-contract.md` | ❌ No |
| Custom scalar validation rules | Compile-time | Authoring schema | `scalars.md` | ❌ No |
| Type relationships (nested fields) | Compile-time | Authoring schema | `authoring-contract.md` | ❌ No |
| Interface implementations | Compile-time | Authoring schema | `authoring-contract.md` | ❌ No |
| Union member types | Compile-time | Authoring schema | `authoring-contract.md` | ❌ No |
| **OPERATORS & FILTERING** |
| What WHERE operators are available | Compile-time | Database capability manifest + target | `database-targeting.md` | ❌ No |
| Which operators work on which types | Compile-time | Database capability manifest | `where-operators.md` | ❌ No |
| Operator→SQL mapping | Compile-time | Compiler lowering rules | `database-targeting.md` Section 3 | ❌ No |
| Extension operator availability (pgvector, PostGIS) | Compile-time | Schema configuration + capability manifest | `database-targeting.md` Section 2 | ❌ No |
| Nested WHERE structure | Compile-time | Type graph introspection | `database-targeting.md` Section 4 | ❌ No |
| **QUERIES & MUTATIONS** |
| What queries exist | Compile-time | Authoring schema | `authoring-contract.md` | ❌ No |
| What mutations exist | Compile-time | Authoring schema | `authoring-contract.md` | ❌ No |
| Query arguments (names, types) | Compile-time | Authoring schema | `authoring-contract.md` | ❌ No |
| Mutation input types | Compile-time | Authoring schema | `authoring-contract.md` | ❌ No |
| Return types | Compile-time | Authoring schema | `authoring-contract.md` | ❌ No |
| **BINDINGS** |
| Type → database view binding | Compile-time | Authoring schema | `authoring-contract.md` Section 4 | ❌ No |
| Mutation → stored procedure binding | Compile-time | Authoring schema | `authoring-contract.md` Section 4 | ❌ No |
| Projection rules (field → JSONB path) | Compile-time | Binding definitions | `execution-model.md` Phase 5 | ❌ No |
| Whether binding is valid | Compile-time | Compiler validates against DB schema | `compilation-pipeline.md` Phase 3 | ❌ No |
| **VALIDATION** |
| GraphQL syntax validity | Parse-time | GraphQL parser | GraphQL spec | ❌ No |
| Query semantic validity (fields exist) | Compile-time | Schema validator | `authoring-contract.md` | ❌ No (schema fixed) |
| Query semantic validity (field selection) | Runtime | Rust runtime validates request | `execution-model.md` Phase 1 | ✅ Yes (per request) |
| Argument types match schema | Compile-time (schema) + Runtime (values) | Compiler + Runtime | `execution-model.md` Phase 1 | ✅ Yes (values) |
| Required arguments provided | Runtime | Rust runtime | `execution-model.md` Phase 1 | ✅ Yes (per request) |
| Custom scalar value validation | Runtime | Rust runtime | `scalars.md` | ✅ Yes (per value) |
| **AUTHORIZATION** |
| Authorization rule syntax | Compile-time | Schema validator | `authoring-contract.md` Section 5 | ❌ No |
| What auth context fields are required | Compile-time | Auth context schema declaration | `PRD.md` Section 4.2 | ❌ No |
| Whether user is authenticated | Runtime | External auth provider | `PRD.md` Section 4.2 | ✅ Yes (per request) |
| AuthContext structure | Runtime | Auth provider produces | `PRD.md` Section 4.2 | ✅ Yes (per user) |
| Whether user is authorized for query | Runtime | Compiled auth metadata + AuthContext | `execution-model.md` Phase 2 | ✅ Yes (per request) |
| Whether user can see specific field | Runtime | Compiled field-level auth + AuthContext | `execution-model.md` Phase 5 | ✅ Yes (per field) |
| Role hierarchy rules | Compile-time | RBAC schema declaration | `rbac.md` | ❌ No |
| User's roles | Runtime | AuthContext from provider | `rbac.md` | ✅ Yes (per user) |
| **EXECUTION** |
| Query execution plan | Compile-time | Compiler planning phase | `compilation-pipeline.md` Phase 5 | ❌ No |
| SQL to execute for each query | Compile-time | Compiler SQL generation | `compilation-pipeline.md` Phase 5 | ❌ No |
| SQL parameters (runtime values) | Runtime | Request arguments | `execution-model.md` Phase 4 | ✅ Yes (per request) |
| Which database to query | Compile-time | Database target configuration | `database-targeting.md` | ❌ No |
| Connection string / credentials | Runtime | Environment configuration | `production-deployment.md` | ✅ Yes (deployment) |
| **CACHING** |
| Whether caching is enabled | Compile-time | Schema configuration | `caching.md` | ❌ No (config fixed) |
| Cache backend (Memory, PostgreSQL, Custom) | Compile-time | Schema configuration | `caching.md` | ❌ No (config fixed) |
| Cache key generation strategy | Compile-time | Compiler cache key rules | `caching.md` Section 2 | ❌ No |
| Specific cache key for this query | Runtime | Query + arguments + AuthContext | `caching.md` Section 2 | ✅ Yes (per request) |
| Cache hit/miss | Runtime | Cache lookup | `execution-model.md` Phase 0 | ✅ Yes (per request) |
| Cache TTL policy | Compile-time | Configuration | `caching.md` Section 3 | ❌ No (config fixed) |
| Cache invalidation cascade | Runtime | Mutation response metadata | `caching.md` Section 4 | ✅ Yes (per mutation) |
| **APQ (AUTOMATIC PERSISTED QUERIES)** |
| APQ security mode (OPTIONAL, REQUIRED, DISABLED) | Compile-time | Configuration | `persisted-queries.md` Section 3 | ❌ No (config fixed) |
| APQ storage backend | Compile-time | Configuration | `persisted-queries.md` Section 2 | ❌ No (config fixed) |
| Query hash → query text mapping | Runtime | APQ storage | `persisted-queries.md` Section 2 | ✅ Yes (queries registered) |
| Whether this query is registered | Runtime | APQ lookup | `execution-model.md` Phase 0 | ✅ Yes (per query) |
| **PERFORMANCE** |
| Connection pool size | Compile-time | Configuration | `production-deployment.md` | ❌ No (config fixed) |
| Query complexity limits | Compile-time | Configuration | `execution-model.md` Phase 1 | ❌ No (config fixed) |
| Rate limiting rules | Compile-time | Configuration | `security-compliance.md` | ❌ No (config fixed) |
| Rate limiting enforcement | Runtime | Request rate tracking | `security-compliance.md` | ✅ Yes (per user) |
| **MONITORING** |
| What metrics are collected | Compile-time | Configuration | `monitoring.md` | ❌ No (config fixed) |
| Metric values | Runtime | Execution tracking | `monitoring.md` | ✅ Yes (per request) |
| Tracing enabled/disabled | Compile-time | Configuration | `monitoring.md` Section 3 | ❌ No (config fixed) |
| Trace spans | Runtime | Request execution | `monitoring.md` Section 3 | ✅ Yes (per request) |

---

## Key Patterns

### Pattern 1: Schema Structure is Compile-Time

**Everything about the GraphQL schema** is determined at compile time:

- What types exist
- What fields each type has
- What arguments queries accept
- What operators are available

**Why?** Schema is the contract. Changing it at runtime breaks that contract.

**Example:**

```python
# Compile-time: Define schema
@fraiseql.type
class User:
    id: ID
    name: str
    email: str

# Runtime: Can't add fields or change types
# This is impossible:
# user.add_field("age", Int)  # ❌ NOT ALLOWED
```

---

### Pattern 2: Values are Runtime, Structure is Compile-Time

**The structure of data** is compile-time.
**The values of data** are runtime.

**Example:**

```graphql
# Compile-time: Query structure
query {
  users(where: { name: { _eq: $name } }) {  # Structure fixed
    id
    name
  }
}

# Runtime: Variable values change per request
{ "name": "Alice" }  # ✅ Value is runtime
{ "name": "Bob" }    # ✅ Different value, same structure
```

---

### Pattern 3: Authorization Rules are Compile-Time, Enforcement is Runtime

**Authorization rules** (what's required) are declared at compile time.
**Authorization enforcement** (whether user meets requirements) happens at runtime.

**Example:**

```python
# Compile-time: Declare rule
@fraiseql.query
@requires_role("admin")  # Rule declared
def users():
    pass

# Runtime: Check if user has "admin" role
# AuthContext.roles includes "admin"? ✅ Allow : ❌ Deny
```

---

### Pattern 4: Configuration is Compile-Time, Behavior is Runtime

**Configuration** (what features are enabled) is compile-time.
**Behavior** (how features execute) is runtime.

**Example:**

```python
# Compile-time: Enable caching
config = CompilerConfig(
    caching_enabled=True,         # Compile-time decision
    cache_backend="postgresql",   # Compile-time decision
    cache_ttl=300                 # Compile-time decision
)

# Runtime: Cache lookup and storage
# Is result in cache? ✅ Return cached : ❌ Execute query
```

---

## Common Misconceptions

### Misconception 1: "Authorization happens at compile time"

**Wrong:** Authorization **rules** are declared at compile time, but **enforcement** happens at runtime.

**Why?** The user's identity (AuthContext) isn't known until runtime.

**Correct understanding:**

- **Compile-time:** Validate authorization rule syntax, check auth context schema
- **Runtime:** Check if user's AuthContext satisfies authorization rules

---

### Misconception 2: "Queries are validated at runtime"

**Partially wrong:** Query **structure** is validated at compile time (schema). Query **field selection** is validated at runtime (request).

**Why?** Schema is known at compile time. Specific query text comes from client at runtime.

**Correct understanding:**

- **Compile-time:** Schema defines valid fields, types, arguments
- **Runtime:** Request validates that client selected fields that exist in schema

---

### Misconception 3: "WHERE operators can be added at runtime"

**Wrong:** WHERE operators are determined at compile time based on database target.

**Why?** Compiler reads capability manifest and generates WHERE types. Can't change at runtime.

**Correct understanding:**

- **Compile-time:** Capability manifest + database target → WHERE types generated
- **Runtime:** Runtime only executes operators that exist in compiled schema

---

### Misconception 4: "CompiledSchema can be modified at runtime"

**Wrong:** CompiledSchema is immutable.

**Why?** Runtime executes compiled plans. Changing CompiledSchema would invalidate all execution logic.

**Correct understanding:**

- **Compile-time:** Produce immutable CompiledSchema artifact
- **Runtime:** Load CompiledSchema once at startup, never modify

---

## Boundary Violations (Anti-Patterns)

### ❌ Anti-Pattern 1: Runtime Schema Modification

**Wrong:**

```rust
// Runtime tries to add field to type
fn add_field_to_user(schema: &mut CompiledSchema, field_name: &str) {
    schema.types["User"].fields.insert(field_name, ...);  // ❌ FORBIDDEN
}
```

**Why wrong?** Schema is compile-time contract. Runtime modification breaks determinism.

**Correct:** Recompile schema with new field at build time.

---

### ❌ Anti-Pattern 2: Dynamic Operator Translation

**Wrong:**

```rust
// Runtime tries to emulate unavailable operator
fn execute_regex_on_mysql(col: &str, pattern: &str) -> String {
    // Fake regex with LIKE + stored procedures
    format!("CALL emulate_regex({}, {})", col, pattern)  // ❌ FORBIDDEN
}
```

**Why wrong?** GraphQL schema lies about operator availability. Clients get runtime surprises.

**Correct:** Don't expose `_regex` in MySQL-targeted schema (compile-time decision).

---

### ❌ Anti-Pattern 3: Runtime Authorization Logic

**Wrong:**

```python
# Runtime resolver executes auth logic
def resolve_user_email(user, context):
    if context.user.role == "admin":  # ❌ Runtime logic
        return user.email
    else:
        return None
```

**Why wrong?** Authorization should be declarative (compile-time), not imperative (runtime).

**Correct:** Declare field-level auth at compile time:

```python
@fraiseql.type
class User:
    id: ID
    name: str
    email: str = fraiseql.field(requires_role="admin")  # ✅ Compile-time
```

---

### ❌ Anti-Pattern 4: Runtime Query Planning

**Wrong:**

```rust
// Runtime tries to change execution plan
fn execute_query(query: &Query) {
    if query.is_complex() {
        // Use different execution strategy  // ❌ Non-deterministic
        execute_with_batching(query);
    } else {
        execute_normally(query);
    }
}
```

**Why wrong?** Execution should be deterministic. Same query always uses same plan.

**Correct:** Compiler produces optimal execution plan at compile time. Runtime executes it.

---

## Validation Checklist

### Compile-Time Validation Checklist

When implementing compiler features, ensure:

- [ ] Schema structure validated (types, fields, arguments)
- [ ] Bindings validated against database schema
- [ ] Authorization rule syntax validated
- [ ] WHERE operators validated against capability manifest
- [ ] Type relationships validated (no circular references)
- [ ] Scalar types validated
- [ ] Extension operators checked against declared extensions
- [ ] Configuration values validated (TTL > 0, pool size > 0, etc.)
- [ ] No runtime-only values required for compilation

### Runtime Validation Checklist

When implementing runtime features, ensure:

- [ ] Request field selection validated against schema
- [ ] Request argument values validated against types
- [ ] Required arguments validated as present
- [ ] Custom scalar values validated
- [ ] AuthContext structure validated
- [ ] Authorization enforcement checked
- [ ] Cache keys generated deterministically
- [ ] SQL parameters bound safely
- [ ] No schema modification attempted
- [ ] No dynamic operator generation

---

## Decision Flow: "When is This Decided?"

Use this decision tree when unclear about timing:

```
Does the decision depend on request-specific data?
├─ YES → Runtime
│   Examples: AuthContext, cache hit/miss, SQL parameter values
└─ NO → Compile-time
    ↓
    Does the decision depend on configuration?
    ├─ YES → Compile-time (config is static)
    │   Examples: database target, caching enabled, security profile
    └─ NO → Compile-time (schema structure)
        Examples: types, fields, operators available
```

---

## Related Specifications

- **`docs/prd/PRD.md` Section 2.3-2.4** — Compile-time vs runtime responsibilities
- **`docs/architecture/core/compilation-pipeline.md`** — What compiler does (compile-time)
- **`docs/architecture/core/execution-model.md`** — What runtime does (runtime)
- **`docs/architecture/database/database-targeting.md`** — Database target (compile-time decision)
- **`docs/specs/authoring-contract.md`** — Schema declarations (compile-time)

---

## Summary Table: Compile-Time vs Runtime

| Aspect | Compile-Time | Runtime |
|--------|--------------|---------|
| **Schema** | Type definitions, fields, arguments | — |
| **Operators** | Which operators available | — |
| **Bindings** | Type→view, mutation→procedure | — |
| **Validation** | Syntax, schema structure | Field selection, argument values |
| **Authorization** | Rule declarations | Enforcement |
| **Execution** | Query plans, SQL generation | SQL parameter binding, execution |
| **Caching** | Configuration, key generation strategy | Cache lookup, cache storage |
| **APQ** | Configuration, security mode | Query hash lookup |
| **Performance** | Limits, pool size, configuration | Rate limiting enforcement, metrics |
| **Monitoring** | Configuration, what to collect | Metric values, trace spans |
| **Database** | Target database, capability manifest | Connection, credentials |

---

## Key Takeaway

> **If you can determine it without seeing a request, it's compile-time.**
> **If you need request data to determine it, it's runtime.**

**Examples:**

- "What fields does User have?" — Compile-time (no request needed)
- "What is the user's email?" — Runtime (need to execute query)
- "Can this field use _regex?" — Compile-time (capability manifest + database target)
- "Is this query result cached?" — Runtime (need cache key from request)
- "Does this mutation require admin role?" — Compile-time (declared in schema)
- "Does this user have admin role?" — Runtime (need AuthContext from request)

---

*End of Compilation vs Runtime Decision Matrix*
