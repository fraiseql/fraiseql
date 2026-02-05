# FraiseQL Design Decisions: Architecture Decision Records (ADRs)

**Date:** January 2026
**Status:** Complete System Specification
**Audience:** Architecture reviewers, framework contributors, stakeholders, technical leaders

---

## Executive Summary

This document records the fundamental architectural decisions behind FraiseQL, including the rationale, alternatives considered, and trade-offs made. Each decision explains **why** FraiseQL is designed the way it is.

**Core philosophy**: "Compiled, deterministic, database-centric GraphQL execution"

---

## ADR-001: Compile-Time Determinism Over Runtime Interpretation

### Decision

**All semantic decisions are made at compile-time, not runtime.** The compiled schema fully specifies execution plans, authorization rules, optimization strategies, and error handling.

### Rationale

**Problem**: Traditional GraphQL servers interpret queries at runtime:

- No pre-validation (errors discovered during execution)
- No optimization opportunity (query plan created fresh each time)
- No security pre-flight (authorization checked during execution)
- Unpredictable performance (same query can vary by 10x based on conditions)

**FraiseQL approach**: Compile everything ahead of time:

```text
Schema (Python)
    ↓ Compile
CompiledSchema (optimized IR)
    ↓ Runtime
Execute deterministically
```text

### Trade-offs

**Pros:**

- ✅ Predictable performance (same query always ~same latency)
- ✅ Zero-cost abstractions (compilation overhead paid once)
- ✅ Superior security (authorization checked at compile-time)
- ✅ Better errors (caught before deployment)
- ✅ Easier optimization (full query knowledge available)

**Cons:**

- ❌ Schema changes require recompilation
- ❌ No runtime query interpretation (can't do arbitrary queries)
- ❌ Larger deployment artifacts (compiled schema)

### Alternatives Considered

**1. Hybrid (compile-time + runtime fallback)**

- ❌ Rejected: Adds complexity without major benefit
- ❌ Still requires runtime interpretation fallback
- ❌ Harder to reason about (two execution paths)

**2. Pure runtime interpretation (traditional Apollo)**

- ❌ Rejected: Contradicts "predictable performance" goal
- ❌ Security harder to reason about

### Implications

- Schemas are versioned with framework
- Query APIs are stable (no ad-hoc queries)
- All queries pre-defined in schema
- Perfect for GraphQL use case (API contracts)
- Not suitable for dynamic query builders

---

## ADR-002: Database as Source of Truth (No Resolvers)

### Decision

**FraiseQL has no user-defined resolvers.** All data access is through compiled SQL against database views. Developers declaratively define schema; FraiseQL generates execution plans.

### Rationale

**Problem**: Traditional GraphQL resolvers:

- N+1 query problem (fetch user, then N queries for posts)
- Unpredictable latency (resolver implementation varies)
- Hard to reason about execution (multiple independent functions)
- Difficult to optimize (global query optimization impossible)

**FraiseQL approach**: All joins happen in database:

```text
Query: user.posts.comments.author
↓
Compiled to single SQL query with JOINs
↓
Database executes optimally
↓
Single round-trip
```text

### Trade-offs

**Pros:**

- ✅ No N+1 queries (impossible by design)
- ✅ Database optimizer handles everything
- ✅ Single round-trip per query
- ✅ Predicable latency (database timing, not code)
- ✅ Perfect composition (any nesting works)
- ✅ Simple mental model (it's just SQL)

**Cons:**

- ❌ Can't do non-database operations in resolution
- ❌ Cannot call external APIs during query
- ❌ Must pre-compute derived data
- ❌ Requires schema thinking in SQL terms

### Alternatives Considered

**1. Dataloader pattern (batching)**

- ✅ Good: Reduces N+1 to N (grouping requests)
- ❌ Still multiple round-trips
- ❌ Still unpredictable (depends on request batching)
- ❌ Complexity added

**2. GraphQL field resolver caching**

- ✅ Good: Caches resolved values
- ❌ Still doesn't prevent N+1
- ❌ Stale data problems

### Implications

- All data must be in database
- Derived fields must be pre-computed
- Transactions work reliably
- Query optimization works predictably
- Pure data layer (no business logic in resolvers)

---

## ADR-003: Three Orthogonal Execution Planes (JSON, Arrow, Delta)

### Decision

**FraiseQL provides three execution planes from single schema:**

1. **JSON Plane** — Interactive queries/mutations (GraphQL over HTTP)
2. **Arrow Plane** — Columnar analytics (Apache Arrow format)
3. **Delta Plane** — Change data streams (subscriptions, CDC)

All three are **first-class**, compiled from same schema, not afterthoughts.

### Rationale

**Problem**: Single-mode APIs force wrong tool for job:

- GraphQL for analytics is inefficient (wrong data format)
- REST for events doesn't exist (polling hack)
- Message queues for real-time don't have schema

**FraiseQL approach**: One schema, three optimal interfaces:

```text
Schema
├─ JSON Plane (GraphQL): Interactive queries (OLTP)
├─ Arrow Plane (columnar): Analytics (OLAP)
└─ Delta Plane (streams): Events (CDC)
```text

### Trade-offs

**Pros:**

- ✅ Single source of truth (one schema)
- ✅ Right tool for each job (JSON for OLTP, Arrow for OLAP, CDC for events)
- ✅ Consistent authorization/types across planes
- ✅ No duplication of schema definitions

**Cons:**

- ❌ More complex runtime (3 execution paths)
- ❌ Larger implementation (3x more code)
- ❌ Different formats require different client libraries
- ❌ Distributed transactions harder (across planes)

### Alternatives Considered

**1. GraphQL-only (ignoring OLAP/CDC)**

- ❌ Wrong format for analytics
- ❌ Wrong format for streaming
- ❌ Forces workarounds

**2. Separate schema per plane**

- ❌ Breaks single source of truth
- ❌ Schema drift between planes
- ❌ Authorization inconsistency

### Implications

- Three client libraries needed (JSON, Arrow, Delta)
- Same schema version covers all planes
- Plane selection at request time
- Different performance characteristics per plane
- All planes benefit from same optimization work

---

## ADR-004: Federation as First-Class, Database-Linked Where Possible

### Decision

**FraiseQL implements Apollo Federation v2 with database-level linking optimization:**

- Standard HTTP `_entities` endpoint (works with any subgraph)
- Optional database linking (PostgreSQL FDW, SQL Server Linked Servers) for same-database optimization
- Automatic fallback (uses HTTP if database linking unavailable)

### Rationale

**Problem**: Traditional federation has performance penalty:

- Every federated reference = HTTP round-trip
- 50-200ms per roundtrip (10x slower than local join)
- Limits federation to loosely-coupled services only

**FraiseQL approach**: Optimize federation for same-database cases:

```text
Different database: User → HTTP → remote_db → User (50ms)
Same database: User → FDW → same_db → User (5ms, 10x faster)
```text

### Trade-offs

**Pros:**

- ✅ Works with any GraphQL subgraph (HTTP standard)
- ✅ 10x performance for FraiseQL-to-FraiseQL federation
- ✅ Single codebase (automatic strategy selection)
- ✅ No lock-in (database linking is optimization, not requirement)
- ✅ Gradual adoption (start HTTP, add database linking later)

**Cons:**

- ❌ Requires database network access (security boundary concern)
- ❌ Adds complexity (multiple strategies)
- ❌ Database linking setup (FDW, Linked Servers, etc.)
- ❌ Only works for same database type (PostgreSQL-PostgreSQL, not PostgreSQL-MySQL)

### Alternatives Considered

**1. HTTP federation only**

- ✅ Simple, no database linking complexity
- ❌ 10x slower for FraiseQL-to-FraiseQL
- ❌ Doesn't match "unified database" philosophy

**2. Database linking only (no HTTP fallback)**

- ❌ Cannot work with non-FraiseQL subgraphs
- ❌ Only works for same database type

### Implications

- All subgraphs expose HTTP `_entities` endpoint (standard Federation v2)
- Each subgraph detects other subgraph type at compile-time
- Automatic strategy selection (HTTP vs database linking)
- Performance improvement for same-database federation (10x)
- Full compatibility with Apollo Federation ecosystem

---

## ADR-005: Strict Authorization (Deny by Default)

### Decision

**Authorization is strict by default: deny unless explicitly allowed.**

- Type-level, field-level, and query-level authorization required
- Missing authorization rule = deny access
- Authorization rules are declarative (not code)
- Evaluated at compile-time and runtime

### Rationale

**Problem**: Permissive authorization defaults:

- Easy to accidentally expose data (forgot @authorize decorator)
- Hard to audit (must check all resolvers)
- Security through obscurity (undocumented endpoints)

**FraiseQL approach**: Explicit, mandatory authorization:

```python
@FraiseQL.type
@FraiseQL.authorize(rule="authenticated")  # Explicit required
class Post:
    @FraiseQL.authorize(rule="published_or_author")  # Explicit per-field
    content: str
```text

### Trade-offs

**Pros:**

- ✅ Hard to accidentally expose data
- ✅ Audit trail (all auth rules in schema)
- ✅ Compile-time validation (missing rules caught early)
- ✅ Safe default (deny is safer than allow)

**Cons:**

- ❌ More boilerplate (must declare all rules)
- ❌ Slower initial development (more annotations)
- ❌ Non-obvious errors if rule missing (access denied, not found)

### Alternatives Considered

**1. Public by default**

- ❌ Too easy to accidentally expose data
- ❌ Security anti-pattern (implicit deny is industry standard)

**2. Permissions-based (instead of role-based)**

- ✅ More flexible for complex authorization
- ❌ Harder to reason about
- ✅ FraiseQL supports both (roles and custom rules)

### Implications

- All types must have authorization rule
- Developers forced to think about security
- Schema acts as security policy document
- "Security by default" not "security by configuration"

---

## ADR-006: Immutable Audit Logs (Append-Only)

### Decision

**Audit logs are append-only, immutable.** Cannot be modified or deleted (except by database administrator with filesystem access).

### Rationale

**Problem**: Mutable audit logs:

- Admin can delete evidence of their actions
- No compliance (auditors can't rely on logs)
- Insider threat (bad actor deletes audit trail)

**FraiseQL approach**: Technical immutability:

```sql
-- Cannot UPDATE or DELETE audit logs
CREATE TABLE tb_audit_log (
    id BIGSERIAL PRIMARY KEY,
    timestamp TIMESTAMP,
    ...
) WITH (
    autovacuum_vacuum_insert_scale_factor = 0
    -- Prevent auto-cleanup
);

-- Only INSERT allowed
-- Even database role can't DELETE
REVOKE DELETE ON tb_audit_log FROM audit_role;
```text

### Trade-offs

**Pros:**

- ✅ Compliance-friendly (GDPR, HIPAA, PCI-DSS)
- ✅ Insider threat detection (evidence preserved)
- ✅ Unmodified audit trail (trustworthy)

**Cons:**

- ❌ Disk space grows indefinitely (no retention cleanup)
- ❌ Must manage retention separately (archival)
- ❌ Cannot fix audit log errors (only INSERT new corrected entry)

### Alternatives Considered

**1. Mutable logs with admin audit trail**

- ❌ Still doesn't prevent admin deletion
- ❌ Compliance doesn't accept "who deleted audit logs"

**2. Third-party audit service (external immutability)**

- ✅ Better: External service provides immutability
- ✅ Works with this approach
- ✅ Recommended for paranoid deployments

### Implications

- Audit logs must be periodically archived (to prevent disk full)
- Retention policy determined per compliance requirement
- Immutability enforced at database schema level
- Compliance officer can trust audit trail

---

## ADR-007: Subscriptions via Database Change Capture (Not WebSocket Hacks)

### Decision

**Subscriptions use database change capture (CDC) as source of truth, not application-side event tracking.**

- PostgreSQL LISTEN/NOTIFY (built-in)
- Debezium/CDC for other databases
- Guaranteed event delivery (database-backed)
- Per-entity ordering (not global ordering)

### Rationale

**Problem**: Application-side event tracking:

- Events can be lost (if app crashes before sending)
- No source of truth (events live in app memory)
- No ordering guarantee (events may reorder in flight)
- Race conditions (event published before database commit)

**FraiseQL approach**: Database as event source:

```text
User mutation committed
    ↓
Trigger fires in database
    ↓
Event published via LISTEN/NOTIFY
    ↓
Runtime receives guaranteed event
    ↓
Send to subscribers
```text

### Trade-offs

**Pros:**

- ✅ Events guaranteed (sourced from database)
- ✅ No race conditions (after transaction commits)
- ✅ Ordered per entity (database transaction order)
- ✅ Durable (if app crashes, events not lost)
- ✅ Source of truth (database, not app)

**Cons:**

- ❌ Requires database trigger support
- ❌ Cannot generate synthetic events (only real data changes)
- ❌ Depends on database (if down, events stop)
- ❌ Not global ordering (only per-entity)

### Alternatives Considered

**1. Event bus (message queue)**

- ✅ Decoupled: App publishes events
- ❌ Events can be lost (if broker down)
- ❌ Race conditions (event before commit possible)
- ❌ Added complexity (broker management)

**2. WebSocket + in-memory events**

- ✅ Simple: Subscribe and push
- ❌ Events lost on app crash
- ❌ No ordering guarantee

### Implications

- All databases must have change capture mechanism
- Events tied to database transactions
- Event delivery is at-least-once (not exactly-once)
- Idempotent event handlers required (process same event twice)
- Throughput limited by database trigger performance

---

## ADR-008: Compile-Time Schema Specialization Per Database

### Decision

**Different compiled schemas for different database targets.** PostgreSQL-specific schema differs from MySQL-specific schema (same user schema, different compiled versions).

### Rationale

**Problem**: One-size-fits-all compiled schema:

- Cannot use PostgreSQL-specific features (JSONB)
- Cannot use MySQL-specific features (JSON path)
- Must use lowest-common-denominator SQL
- Suboptimal for each database

**FraiseQL approach**: Generate per-database schema:

```text
User schema (generic)
    ├─ Compile for PostgreSQL 15 → PostgreSQL-optimized schema
    ├─ Compile for MySQL 8 → MySQL-optimized schema
    └─ Compile for SQL Server 2022 → SQL Server-optimized schema
```text

Each uses database-specific features (JSONB, partitioning, etc.)

### Trade-offs

**Pros:**

- ✅ Uses each database's strengths
- ✅ Optimal SQL generation per target
- ✅ Better performance (database-specific optimization)
- ✅ Access to advanced features (JSONB, etc.)

**Cons:**

- ❌ Multiple compiled schemas to manage
- ❌ Deployment complexity (choose correct schema)
- ❌ Testing burden (test all database targets)
- ❌ Differences in behavior across databases

### Alternatives Considered

**1. Generic compiled schema (works everywhere)**

- ✅ Simple: One schema for all databases
- ❌ Suboptimal SQL (can't use database features)
- ❌ Lower performance

**2. Capability manifests (this approach)**

- ✅ Same as chosen design

### Implications

- Capability manifest per database type
- Schema compilation takes database parameter
- Deployment must use correct schema for target database
- Testing must validate all target databases
- Schema version tied to database version (somewhat)

---

## ADR-009: Caching at Query Level (Not Field Level)

### Decision

**Caching operates at query level (entire operation cached/busted together), not field level (individual fields cached separately).**

### Rationale

**Problem**: Field-level caching:

- Cache invalidation nightmare (which fields changed?)
- Partial stale data (field A fresh, field B stale)
- Complexity (track dependencies per field)

**FraiseQL approach**: Query-level cache:

```text
Query: GetUserPosts
    ├─ Cache key: {operation_name, variables, user_id}
    ├─ Cached result: {id, title, author, comments}
    └─ Invalidate all together

When data changes:
    └─ Invalidate entire "GetUserPosts" (all variants)
```text

### Trade-offs

**Pros:**

- ✅ Simple cache invalidation (all or nothing)
- ✅ No partial stale data
- ✅ Easy to reason about (one TTL per query)
- ✅ Atomic (fresh or stale, never mixed)

**Cons:**

- ❌ Less granular caching (all fields bust together)
- ❌ Might invalidate too much (one field change invalidates whole query)
- ❌ Cache misses if any field changed

### Alternatives Considered

**1. Field-level caching**

- ✅ More granular
- ❌ Invalidation complexity
- ❌ Partial stale data risk

**2. Hybrid (with dependency tracking)**

- ✅ Better: Tracks field dependencies
- ✅ Smart invalidation
- ❌ Complexity

### Implications

- Cache keys include operation name and user context
- Mutations invalidate related query caches
- Cache TTL per-operation (configurable)
- Clients should use normalized queries (better cache reuse)

---

## ADR-010: No Client-Side Authorization (Server Trusts Itself Only)

### Decision

**Authorization is server-side only.** Client provides no authorization info; server derives all from verified token + database.

### Rationale

**Problem**: Client-side authorization:

- Client can lie about their roles
- Client can forge authorization tokens
- Client can modify GraphQL query (enable disabled fields)

**FraiseQL approach**: Server derives authorization:

```text
Client: "I'm user-456 (don't trust)"
    ↓
Server verifies JWT/token signature
    ✓ Verified: User identity is user-456
    ↓
Server queries: SELECT roles FROM tb_user WHERE id = 'user-456'
    ✓ Trusted: User has roles [author, member]
    ↓
Server evaluates authorization rules
    ✓ Result: User can read this field
```text

### Trade-offs

**Pros:**

- ✅ Secure (cannot forge authorization)
- ✅ Cannot bypass (server controls all access)
- ✅ Consistent (same user always gets same permissions)

**Cons:**

- ❌ Extra database query per request (authorization check)
- ❌ Cannot pre-authorize on client (must hit server)
- ❌ Latency impact (2-5ms per authorization check)

### Alternatives Considered

**1. Client provides roles in JWT**

- ❌ Can be forged if client has JWT
- ❌ Roles can get stale (jwt not refreshed)

**2. Server-side but cached aggressively**

- ✅ Better: Cache authorization decisions
- ✅ Reduces database hit rate
- ✓ FraiseQL does this (300s cache default)

### Implications

- Token verification on every request
- Authorization check on every request (cached)
- No trust in client-provided authorization
- Database query required for first authorization check
- Subsequent checks hit cache (fast)

---

## ADR-011: Error Codes Are Stable (Never Change Within MAJOR Version)

### Decision

**Error codes are part of the API contract.** Once assigned, error codes **never change meaning** within a MAJOR version. Adding codes is fine; changing/removing is not.

### Rationale

**Problem**: Changing error codes:

- Client error handling breaks
- What was "not found" becomes "unauthorized" (different handler)
- Clients must update code
- Introduces bugs in client applications

**FraiseQL approach**: Lock error codes within version:

```text
v2.0.0: E_DB_QUERY_TIMEOUT_302 = "Query timeout"
v2.1.0: Same (cannot change)
v2.5.0: Same (still cannot change)
v3.0.0: Can change (new major version)
```text

### Trade-offs

**Pros:**

- ✅ Clients can rely on error codes
- ✅ No surprise error meaning changes
- ✅ Stable error handling across updates
- ✅ Better error handling in applications

**Cons:**

- ❌ Cannot "fix" a confusing error code (must add new one)
- ❌ Error codes accumulate
- ❌ Might accidentally assign same code twice (risk)

### Alternatives Considered

**1. Change error codes freely**

- ❌ Breaks client error handling
- ❌ Impossible to know error code meaning in production

**2. Use error messages for matching**

- ❌ Messages change (not stable)
- ❌ Localization breaks (messages translated)

### Implications

- Error code design must be careful (assign once, permanently)
- New error codes added via deprecation + new code
- Error codes documented in CHANGELOG
- Error handling in clients matches by code, not message

---

## ADR-012: Strict Consistency by Default (SERIALIZABLE Isolation)

### Decision

**Default isolation level is SERIALIZABLE (highest consistency).** Applications can opt-in to weaker isolation (READ_COMMITTED) if needed for performance.

### Rationale

**Problem**: Weak isolation by default:

- Hard to reason about (unexpected behavior)
- Race conditions not obvious
- Difficult to debug (timing-dependent bugs)

**FraiseQL approach**: Safe by default:

```text
Default: SERIALIZABLE (safest)
    ├─ No dirty reads
    ├─ No non-repeatable reads
    ├─ No phantom reads
    └─ Strongest consistency

Optional: READ_COMMITTED (faster, weaker)
    ├─ Possible non-repeatable reads
    ├─ Possible phantom reads
    └─ Use only if you understand risks
```text

### Trade-offs

**Pros:**

- ✅ Safe by default (most developers don't understand isolation)
- ✅ Easier to reason about (serialized)
- ✅ Fewer race conditions in practice

**Cons:**

- ❌ Performance cost (SERIALIZABLE slower)
- ❌ More deadlocks (SERIALIZABLE detects conflicts)
- ❌ May need tuning for performance

### Alternatives Considered

**1. READ_COMMITTED by default (faster)**

- ✅ Better performance
- ❌ Developers must understand isolation
- ❌ Race conditions possible

**2. Application-specific per-query**

- ✅ Fine-grained control
- ✓ FraiseQL supports this

### Implications

- All queries/mutations SERIALIZABLE by default
- Performance tuning may require lower isolation
- Deadlock detection and retry required
- Database must support SERIALIZABLE (PostgreSQL, SQL Server, etc.)

---

## ADR-013: No Schema Versioning in Database (Version in Code)

### Decision

**Database schema version is NOT stored in database.** Version is managed in compiled schema (code), not database tables.

### Rationale

**Problem**: Version in database:

- Circular dependency (schema version = part of schema)
- Harder to deploy (must migrate version before using new schema)
- Complicates schema evolution
- Storage overhead (one more table)

**FraiseQL approach**: Version in compiled schema only:

```text
CompiledSchema {
    framework_version: "2.0.0",
    compiled_schema_version: 1,
    types: { User, Post, ... }
}
```text

Runtime checks version at startup:

```text
Compiled schema v2.0.0 matches runtime v2.0.0 ✓
→ Load schema
```text

### Trade-offs

**Pros:**

- ✅ No circular dependency
- ✅ Simpler deployment (one atomic step)
- ✅ Schema is self-describing
- ✅ No database migration for versioning

**Cons:**

- ❌ Deployment must use matching schema+runtime
- ❌ Cannot query schema version from database
- ❌ Need external versioning system (git, deployment tool)

### Alternatives Considered

**1. Version in database**

- ❌ Circular dependency
- ❌ Harder to deploy

**2. Hybrid (version in both places)**

- ✅ Can query version
- ❌ Adds complexity (must stay in sync)

### Implications

- Deployment is atomic (schema + runtime together)
- Schema versioning managed in git/code
- No schema query from database
- Version stored in compiled artifact

---

## ADR-014: User Schema Separate from Compiled IR

### Decision

**User-defined schema (Python/YAML) is separate from compiled intermediate representation (IR).** Compilation transforms one to other; they are not the same.

### Rationale

**Problem**: Single schema format:

- User schema cluttered with compilation details
- Hard to optimize (transformation happens in-place)
- No clean separation of concerns
- Schema validation mixed with optimization

**FraiseQL approach**: Two schemas:

```text
User Schema (Python)
    @FraiseQL.type
    class User:
        id: ID
        posts: [Post]

    ↓ Compile

Intermediate Schema (IR)
    {
        "types": {"User": {...}},
        "bindings": {...},
        "authorization": {...}
    }

    ↓ Compile

Compiled Schema (executable)
    {
        "queries": {...},
        "mutations": {...},
        "subscriptions": {...},
        "sql_plans": {...}
    }
```text

### Trade-offs

**Pros:**

- ✅ Clean separation of concerns
- ✅ Each phase has clear input/output
- ✅ Easier to optimize (transform at each phase)
- ✅ Easier to debug (inspect intermediate state)
- ✅ Pluggable phases (can replace optimization phase)

**Cons:**

- ❌ More complexity (multiple schemas)
- ❌ Larger codebase (more transformation logic)
- ❌ Performance cost (multiple transformations)

### Alternatives Considered

**1. Single schema (user + compiled merged)**

- ✅ Simpler
- ❌ Harder to optimize
- ❌ Less modular

**2. Direct compilation (no IR)**

- ✅ Faster compilation
- ❌ Harder to optimize
- ❌ Harder to debug

### Implications

- Compiler has clear phases (can be extended)
- Can inspect intermediate schema (debugging)
- Can plugin custom optimization phases
- Compilation slower (multiple stages)

---

## ADR-015: Explicit Field-Level Federation vs Automatic

### Decision

**Federation references are explicit in schema.** Developer declares which fields are federated; system doesn't auto-detect.

### Rationale

**Problem**: Automatic federation detection:

- Non-obvious which fields are external (must read comments)
- Difficult to reason about (implicit behavior)
- Hard to debug (why is this federated?)

**FraiseQL approach**: Explicit in schema:

```python
@FraiseQL.type
class Post:
    id: ID
    title: str

    # Explicit: This field comes from Authors subgraph
    @FraiseQL.requires(fields=["author_id"])
    author: Author  # External type

    # Explicit: This field is ours
    comments: [Comment]
```text

### Trade-offs

**Pros:**

- ✅ Clear which fields are external
- ✅ No surprises (explicit is obvious)
- ✅ Easier to debug (know immediately)

**Cons:**

- ❌ More boilerplate (must declare externals)
- ❌ Developers must remember to mark them
- ❌ Could accidentally miss one

### Alternatives Considered

**1. Automatic detection (by type not in schema)**

- ✅ Less boilerplate
- ❌ Implicit behavior (hard to understand)
- ❌ Brittle (if type added locally, detection breaks)

**2. Config file (external types in separate file)**

- ✅ Centralized
- ❌ Separate from schema definition
- ❌ Must stay in sync

### Implications

- All external references must be decorated
- Federation relationship visible in code
- Compiler validates federation decorators
- Easy to audit federation dependencies

---

## Summary: Core Principles

FraiseQL's architectural decisions stem from these core principles:

```text

1. Predictability: Same inputs, same outputs (determinism)
2. Security: Deny by default, explicit allow
3. Database-centric: Database as source of truth, not app
4. Auditability: Everything is traceable and immutable
5. Simplicity: Fewer magic behaviors, explicit is better
6. Performance: Compiled execution, zero-cost abstractions
7. Consistency: Strong consistency by default
8. Compatibility: Follows standards (Apollo Federation v2, W3C Trace, etc.)
```text

---

**Document Version**: 1.0.0
**Last Updated**: January 2026
**Status**: Complete ADR documentation for framework v2.x

FraiseQL's architecture is intentional, reasoned, and documented. Each design decision reflects trade-offs consciously evaluated.
