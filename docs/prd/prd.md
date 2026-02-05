<!-- Skip to main content -->
---
title: FraiseQL v2 — Product Requirements Document
description: FraiseQL is a **compiled database execution layer** that provides a GraphQL API over transactional state managed by the database.
keywords: []
tags: ["documentation", "reference"]
---

# FraiseQL v2 — Product Requirements Document

**Version:** 2.0
**Status:** Released (v2.0.0-alpha.1)
**Date:** February 5, 2026
**Audience:** Core maintainers, contributors, design partners

---

## 1. Vision & Philosophy

### 1.1 Product Vision

FraiseQL is a **compiled database execution layer** that provides a GraphQL API over transactional state managed by the database.

Its core purpose is to enable:

- **Language-agnostic schema authoring** (Python, TypeScript, YAML, GraphQL SDL) that compiles to identical execution plans
- **Multi-database support** via compile-time schema specialization — the same schema produces database-specific GraphQL APIs (PostgreSQL gets 60+ operators, MySQL gets 20+, SQLite gets 15)
- **Deterministic query execution** with no user-provided code, resolvers, or dynamic logic
- **Type-safe, compile-time validated** GraphQL schemas with database-backed authorization
- **Maximum performance** through compilation to optimized SQL and a Rust runtime (7-10x JSON transformation speedup)
- **Client cache invalidation** with deterministic cascade metadata from mutations
- **Optional Arrow acceleration** for analytics workloads (columnar, typed, batch-oriented)
- **Pluggable authentication** with declarative authorization rules

FraiseQL prioritizes **correctness, predictability, and evolvability** over flexibility. The database is the source of truth; GraphQL is the client interface.

### 1.2 Core Design Principles

1. **Compilation over interpretation**
   All semantics are resolved at compile time. Runtime behavior is fixed.

2. **Declarative semantics only**
   GraphQL describes *what* data is accessed, never *how*.

3. **No executable user code at runtime**
   No resolvers, hooks, middleware, or dynamic logic.

4. **Database as the source of truth**
   All joins, filters, constraints, and derivations belong to the database.

5. **Strict separation of concerns**
   Authoring, compilation, execution, authentication, and storage are isolated.

6. **Minimal, stable contracts**
   Integration points are explicit, versioned, and intentionally small.

7. **Authentication is external; authorization is metadata**
   Auth logic does not participate in query execution.

8. **Rich, static type system**
   FraiseQL's type system is the foundation for compilation. All runtime behavior — query planning, authorization enforcement, data projection, cache invalidation — is derived statically from type information. The runtime executes compiled plans; it does not interpret types.

9. **Compile-time database specialization**
   The database target (PostgreSQL, MySQL, SQLite, etc.) specified at compile time drives the exact GraphQL schema generated. WHERE input types are specialized per database, exposing only operators that database supports. No runtime translation or emulation. Same schema source, different compiled outputs per database.

10. **Language-agnostic authoring via intermediate representation**
    Schemas can be authored in any language (Python, TypeScript, YAML, GraphQL SDL, CLI) and all compile to identical execution plans. Language choice is an organization-scale decision, not a system constraint. An intermediate representation (AuthoringIR) unifies all language-specific syntaxes before compilation.

### 1.3 One-Sentence Definition

> **FraiseQL is a compiled database execution layer that accepts schemas in any language, specializes them per database target, exposes a type-safe GraphQL API, and executes queries deterministically in Rust without user code.**

---

## 2. System Architecture

### 2.1 High-Level Architecture

```text
<!-- Code example in TEXT -->
Schema Authoring (Any Language)
 ├─ Python / TypeScript / Java / Kotlin / CLI
 │   └─ emits CompiledSchema
 ▼
CompiledSchema (JSON / Rust)
 ├─ Type system
 ├─ Query & mutation definitions
 ├─ Database bindings (JSON + Arrow)
 ├─ Authorization requirements
 ├─ Federation metadata
 ├─ Database capability manifest
 ├─ Feature flags & versioning
 └─ No executable code
 ▼
FraiseQL Runtime (Rust)
 ├─ GraphQL validation
 ├─ Deterministic query planning
 ├─ Authorization enforcement
 ├─ Database execution
 ├─ Cache invalidation emission
 └─ JSON / Arrow output
```text
<!-- Code example in TEXT -->

### 2.2 Compilation Model

The authoring layer (Python, TypeScript, etc.) produces a **CompiledSchema** at build time. This schema:

- Contains all type definitions, queries, mutations, and bindings
- Includes authorization requirements as static metadata
- Declares database capabilities and available operators
- Contains no executable code

The runtime consumes the CompiledSchema and executes all requests without further interpretation.

### 2.3 Compile-Time vs Runtime Responsibilities

**Compile-Time (Static Analysis):**

- Parse schema in any language (Python, TypeScript, YAML, GraphQL SDL)
- Validate types and bindings against database schema
- Generate WHERE operators based on database target capabilities
- Build deterministic execution plans
- Validate authorization rule syntax
- Create optimized SQL for each query
- Produce immutable CompiledSchema artifact

**Runtime (Deterministic Execution):**

The Rust runtime receives CompiledSchema and:

- Deserializes incoming GraphQL requests
- Validates requests against compiled schema (field names, types, required arguments)
- Applies request-level caching and APQ resolution if enabled
- Authenticates user and builds auth context from external provider
- Checks authorization metadata against auth context
- Executes pre-compiled database queries with runtime-provided values
- Projects results to JSON or Arrow format using compiled projections
- Emits cache invalidation signals with cascade metadata

**The runtime does NOT:**

- Interpret, analyze, or rewrite GraphQL queries
- Make authorization decisions (they are compile-time declarations)
- Execute user-provided code or resolvers
- Translate or emulate database operators
- Modify schema based on user input
- Call other services or invoke functions

### 2.4 Key Invariants: When Decisions Are Made

| Decision | When | Authority |
|----------|------|-----------|
| What types exist | Compile-time | Authoring schema |
| What fields each type has | Compile-time | Authoring schema |
| What WHERE operators are available | Compile-time | Database capability manifest + target |
| Whether a query is syntactically valid | Compile-time | GraphQL grammar |
| Whether a query is semantically valid | Compile-time | Schema validator |
| How queries map to SQL | Compile-time | Compiler lowering rules |
| Whether user is authenticated | Runtime | External auth provider |
| Whether user is authorized for this request | Runtime | Compiled authorization metadata + auth context |
| What SQL to execute | Runtime | Compiled execution plan + runtime parameter values |
| What results to return | Runtime | Compiled projection rules + database results |

---

## 3. Execution Semantics

### 3.1 Database Contract

#### 3.1.1 Read Model

Reads are expressed as **deterministic projections** over database-owned views.

Two planes are supported:

- **JSON plane**
  - Nested object-shaped projections
  - JSON / JSONB compatible

- **Arrow plane**
  - Typed, columnar projections
  - Batch-oriented and analytics-friendly

The database:

- Owns joins, filters, and derived state
- Exposes **logical views**, not raw tables
- May use any internal strategy or engine

The runtime does not interpret relational logic.

#### 3.1.2 Write Model

Writes are expressed as **commands**:

- Invoked via stored procedures or functions
- Input is a validated JSON payload
- Execution is transactional
- Side effects are handled via DB observers or triggers

Mutations may return explicitly declared output types. Returned values must be directly bound to database write results or post-commit reads defined in the CompiledSchema. The runtime does not compute or derive additional state.

#### 3.1.3 Custom Logic

All custom logic must live:

- Inside the database (constraints, triggers, observers)
- Or in external systems reacting to DB events

Custom logic **never** runs in the GraphQL runtime.

#### 3.1.4 Storage vs Projection Separation

FraiseQL enforces strict separation between **storage** (write model) and **projection** (read model).

**Storage** is defined in the database:

- Tables (`tb_*`) own normalized data
- Constraints, triggers, and procedures enforce integrity
- The authoring layer never defines storage schema

**Projections** are defined in the authoring layer:

- Types describe client-facing shapes
- A single table may have multiple projections
- A single projection may span multiple tables
- Bindings connect projections to database views

This separation allows:

- Independent evolution of storage and API
- Multiple API shapes over the same data
- Clear ownership boundaries (DBA vs API designer)

#### 3.1.5 Projection Composition

Nested types are expressed as **JSONB composition**, not ORM-style relationships.

Each read view (`v_*`) produces a `data` JSONB column containing the projection. Nested fields compose projections via aggregation:

| Field Declaration | Composition Strategy |
|-------------------|---------------------|
| `posts: list[Post]` | `array(v_posts_by_user.posts)` |
| `author: User` | `v_user.data` (single object) |
| `postCount: int` | Scalar aggregation |
| `latestPost: Post` | `v_post.data` with ORDER + LIMIT 1 |

**Pre-aggregated views** enable efficient composition:

```sql
<!-- Code example in SQL -->
-- Pre-aggregated: posts grouped by user
CREATE VIEW v_posts_by_user AS
SELECT
    fk_user,
    jsonb_agg(data) AS posts
FROM v_post
GROUP BY fk_user;

-- Composition becomes a simple join
CREATE VIEW v_user_with_posts AS
SELECT
    u.pk_user,
    u.id,
    u.data || jsonb_build_object(
        'posts', COALESCE(p.posts, '[]'::jsonb)
    ) AS data
FROM v_user u
LEFT JOIN v_posts_by_user p ON p.fk_user = u.pk_user;
```text
<!-- Code example in TEXT -->

The database performs all composition. The runtime receives fully-formed JSONB.

For the Arrow plane, composition produces **multiple flat batches** with explicit key references instead of nested JSONB.

### 3.2 Schema Conventions

FraiseQL enforces opinionated schema conventions that unlock powerful features across the ecosystem:

- **Naming (`tb_*`, `v_*`, `fn_*`, etc.)** — Separates write tables from read views, enables automatic CQRS routing
- **Dual-key strategy (`pk_*`, `id`, `identifier`)** — Integer keys for join performance, UUIDs for external identity, slugs for URLs
- **Column structure (`data` JSONB, `{entity}_id` natives)** — Pre-aggregated projections composed from database views
- **Audit columns (`created_at`, `updated_at`, `deleted_at`)** — Enables soft deletes, cache invalidation, CDC
- **View composition** — Pre-aggregated views enable O(1) relationship composition without ORM overhead

**Analytical tables** use specialized naming conventions:

- **Fact tables (`tf_*`)** — Transactional data with measures (SQL columns) + dimensions (JSONB), any granularity
- **Dimension tables (`td_*`)** — Reference data for ETL denormalization (not joined at query time)

These conventions are **not optional** — they are required by FraiseQL's compilation and execution model. For complete schema conventions reference, see **`docs/specs/schema-conventions.md`** and **`docs/specs/analytical-schema-conventions.md`**.

### 3.3 Compile-Time Database Specialization

WHERE input types are automatically generated based on the target database's capabilities. This ensures that the GraphQL schema exposes only operators the target database supports, preventing runtime surprises and eliminating the need for runtime operator fallbacks.

**How it works:**

1. Compiler configuration specifies `database_target` (PostgreSQL, MySQL, SQLite, SQL Server, etc.)
2. A database capability manifest declares which operators each database supports
3. Phase 4 of compilation (WHERE type generation) reads the manifest for the target database
4. Only supported operators appear in the generated WHERE input types
5. Client queries cannot express unsupported operations (compile-time error)
6. Runtime backend lowering translates WHERE filters to database-specific SQL

**Example:** PostgreSQL compilation includes 60+ operators (`_regex`, `_cosine_distance`, `_jsonb_contains`, etc.). MySQL compilation includes 20+ operators (`_like`, `_json_extract`). SQLite compilation includes 15 basic operators (comparison and text).

For complete details on multi-database support architecture, see **`docs/architecture/database/database-targeting.md`**.

### 3.4 Data Planes

#### 3.4.1 JSON Data Plane (Default)

- `application/json`
- Nested GraphQL response shape
- Frontend-oriented

#### 3.4.2 Arrow Data Plane (Optional)

- `application/x-arrow`
- Columnar, typed, batch-oriented
- Suitable for analytics, BI, and ML workloads

Arrow projections are relational and columnar. Nested GraphQL selections compile into **multiple Arrow batches**, each representing a single logical entity or relationship, with explicit key references between batches.

The runtime does not materialize nested object graphs in the Arrow plane. Clients are responsible for joining or materializing relationships if required.

Single Arrow batches are limited to single-level projections.

Arrow support is optional and feature-gated.

**Implementation Status:** ✅ Implemented in v2.0.0-alpha.1 (feature-gated in cargo features)

**Detailed specification:** See `docs/architecture/database/arrow-plane.md` for complete Arrow architecture, authoring syntax, performance characteristics, and implementation phases.

#### 3.4.3 Delta Data Plane (Event Streams)

- **Protocol**: GraphQL Subscriptions (graphql-ws), webhooks, Kafka, gRPC
- **Event format**: Debezium-compatible CDC envelope
- **Source**: Database transactions (LISTEN/NOTIFY for PostgreSQL, CDC for others)
- **Durability**: Persisted in `tb_entity_change_log` with replay capability
- **Use cases**: Real-time UI updates, event-driven data replication, audit trails, external system integration

Delta plane provides **database-driven change event streaming** integrated with compilation and execution. Unlike JSON and Arrow planes which respond to explicit queries, Delta plane pushes events based on database changes.

**Key characteristics:**

- Events originate from database transactions, not application logic
- Subscription schemas compiled at build time, no dynamic schema introspection
- Multiple transport adapters (graphql-ws, webhooks, Kafka) consume same event stream
- Row-level authorization enforced at event capture time
- Events include full before/after data for audit and replay

**Detailed specification:** See `docs/architecture/realtime/subscriptions.md` for complete Delta plane architecture, event capture mechanisms, transport adapters, and implementation phases.

### 3.5 Analytical Execution Semantics

FraiseQL v2 supports database-native analytical queries through compile-time schema analysis and runtime SQL generation.

#### 3.5.1 Fact Table Pattern

Analytical workloads use **fact tables** following a standardized pattern:

- **Naming**: `tf_*` prefix (table fact)
- **Measures**: SQL columns with numeric types (INT, DECIMAL, FLOAT) for fast aggregation
- **Dimensions**: JSONB `dimensions` column for flexible GROUP BY grouping
- **Denormalized filters**: Indexed SQL columns (customer_id, occurred_at) for fast WHERE filtering

**No Joins**: FraiseQL does not support joins. All dimensional data must be denormalized into the `dimensions` JSONB column at ETL time (managed by DBA/data team, not FraiseQL).

**Example fact table**:

```sql
<!-- Code example in SQL -->
CREATE TABLE tf_sales (
    id BIGSERIAL PRIMARY KEY,
    -- Measures (SQL columns)
    revenue DECIMAL(10,2) NOT NULL,
    quantity INT NOT NULL,
    -- Dimensions (JSONB)
    dimensions JSONB NOT NULL,
    -- Denormalized filters (indexed)
    customer_id UUID NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL
);
```text
<!-- Code example in TEXT -->

**Pre-aggregated fact tables** (e.g., `tf_sales_daily`, `tf_events_monthly`) follow the same structure as raw fact tables, just at different granularity. Use descriptive suffixes to indicate granularity.

**Dimension tables** (`td_*` prefix) are reference data used at ETL time to denormalize into fact tables. They are never joined at query time.

#### 3.5.2 GROUP BY Compilation

When a type is marked as a fact table, the compiler:

1. **Introspects** table structure to identify measures, dimensions, and filters
2. **Generates** GraphQL aggregate types:
   - `{Type}Aggregate` - Result type with grouped dimensions + aggregated measures
   - `{Type}GroupByInput` - Dimension paths + temporal buckets
   - `{Type}HavingInput` - Post-aggregation filters
3. **Validates** that measures are numeric types
4. **Emits** SQL templates with GROUP BY, aggregate functions, and HAVING clauses

#### 3.5.3 Aggregate Functions

Supported aggregate functions vary by database target (from capability manifest):

**Basic (all databases)**:

- COUNT(*), COUNT(DISTINCT field)
- SUM(field), AVG(field), MIN(field), MAX(field)

**Statistical (PostgreSQL, SQL Server)**:

- STDDEV(field), VARIANCE(field)
- PERCENTILE_CONT(field, percentile)

**Conditional Aggregates**:

- PostgreSQL: `SUM(revenue) FILTER (WHERE status = 'completed')`
- MySQL/SQLite/SQL Server: `SUM(CASE WHEN status = 'completed' THEN revenue ELSE 0 END)`

#### 3.5.4 Temporal Bucketing

Time-series dimensions are compiled using database-specific temporal functions:

| Database | Function | Supported Buckets |
|----------|----------|-------------------|
| PostgreSQL | DATE_TRUNC | second, minute, hour, day, week, month, quarter, year |
| MySQL | DATE_FORMAT | day, week, month, year |
| SQLite | strftime | day, week, month, year |
| SQL Server | DATEPART | day, week, month, quarter, year, hour, minute |

**Example**: `groupBy: { occurred_at_day: true }` compiles to `DATE_TRUNC('day', occurred_at)` on PostgreSQL.

#### 3.5.5 HAVING Clause

Post-aggregation filters are compiled into HAVING clauses with compile-time validation:

- Validate that HAVING references aggregated measures (not raw columns)
- Generate database-specific HAVING SQL
- Support comparisons: _gt,_gte, _lt,_lte, _eq,_neq

**Example**: `having: { revenue_sum_gt: 10000 }` compiles to `HAVING SUM(revenue) > $1`.

#### 3.5.6 Performance Characteristics

- **SQL column aggregation**: 10-100x faster than JSONB aggregation
- **Indexed filters**: B-tree index access on denormalized columns (customer_id, occurred_at)
- **JSONB dimensions**: Slower than SQL columns but more flexible (use GIN indexes)
- **Pre-aggregated tables**: Query pre-computed fact tables (e.g., `tf_sales_daily`) for common rollups

#### 3.5.7 ETL Responsibility

FraiseQL provides the **GraphQL query interface** over existing tables. The DBA/data team is responsible for:

- Creating and populating `tf_*` (fact) tables with denormalized dimensions
- Creating and refreshing pre-aggregated fact tables (e.g., `tf_sales_daily`) via scheduled jobs
- Maintaining `td_*` (dimension) tables as reference data for ETL processes

FraiseQL does **not** manage ETL pipelines or data loading.

**Detailed specifications**: See:

- `docs/architecture/analytics/aggregation-model.md` - Compilation and execution
- `docs/architecture/analytics/fact-dimension-pattern.md` - Table structure patterns
- `docs/specs/aggregation-operators.md` - Database-specific operators
- `docs/specs/analytical-schema-conventions.md` - Naming conventions and best practices
- `docs/guides/analytics-patterns.md` - Practical query patterns

---

## 4. Security Model

### 4.1 Core Philosophy

> **Authentication is external.**
> **Authorization is declarative.**
> **Enforcement is deterministic.**

FraiseQL:

- Does not authenticate users itself
- Does not execute authorization logic dynamically
- Does not allow auth rules to run arbitrary code

### 4.2 Authentication (AuthN)

Authentication is handled by **pluggable auth providers**.

Supported provider categories include:

- JWT / session-based providers
- OAuth2 / OIDC-compatible providers
- Auth0, Keycloak, and similar systems
- Custom providers via a defined interface

#### Auth Provider Responsibilities

An auth provider:

- Validates incoming credentials
- Produces a **typed, immutable auth context**
- Does not mutate or observe query execution

Example auth context:

```json
<!-- Code example in JSON -->
{
  "subject": "user_123",
  "roles": ["admin"],
  "claims": {
    "tenant_id": "t1",
    "email": "a@b.com"
  }
}
```text
<!-- Code example in TEXT -->

The auth context is:

- Schema-declared
- Versioned
- Validated at compile time
- Treated as immutable input at runtime

### 4.3 Authorization (AuthZ)

Authorization is **declared at compile time** and **enforced at runtime**.

#### 4.3.1 Authorization Scope

Authorization requirements may be declared at:

- Query or mutation level
- Type level
- Field level

Rules reference fields in the auth context and compile into static requirements.

Authorization requirements are **additive**: all applicable rules must be satisfied.

#### 4.3.2 Authorization Declaration (Conceptual)

```graphql
<!-- Code example in GraphQL -->
type User
  @auth(role: "admin") {
  id: ID!
  email: String!
}

query users
  @auth(claim: "tenant_id") {
  users {
    id
    email
  }
}
```text
<!-- Code example in TEXT -->

Authorization directives:

- Are compile-time only
- Do not execute code
- Do not exist at runtime
- Are validated against the declared auth context schema

#### 4.3.3 Authorization Enforcement Layers

Authorization may be enforced in three layers, in order of preference:

1. **Pre-execution rejection**
   - The request is rejected if requirements are unmet

2. **Database-level enforcement (preferred)**
   - Row-level security
   - Scoped views
   - Parameterized bindings using auth context values

3. **Post-projection filtering (last resort)**
   - Deterministic
   - Monotonic (only removes data)
   - Does not affect query shape, timing, or error behavior

#### 4.3.4 Forbidden Patterns

FraiseQL explicitly forbids:

- Resolver-level authorization logic
- Dynamic permission checks
- Request-context mutation
- Imperative branching based on auth
- Runtime directives or hooks

---

## 5. GraphQL Semantics

### 5.1 Supported Features (Core)

- Object types
- Scalars and custom scalars
- Non-null semantics
- Queries
- Mutations (command-style)
- Input types
- Arguments
- Aliases
- Static fragments
- Pagination
- Partial introspection

### 5.2 Supported Features (Advanced, Constrained)

- Interfaces
- Unions
- Compile-time directives
- Subscriptions (DB-driven only)
- Compile-time federation

#### Subscriptions

FraiseQL subscriptions are **compiled database event projections** delivered through multiple transport adapters, not GraphQL resolver-based subscriptions.

**Architecture:**

- Events originate from **database transactions** (LISTEN/NOTIFY, CDC)
- Events buffered in `tb_entity_change_log` for durability and replay
- Subscriptions declared at schema definition time (compile-time)
- Row-level filtering enforced via compiled WHERE clauses
- Multiple transports (graphql-ws, webhooks, Kafka, gRPC) consume same event stream

**Event sources (database-native, no polling):**

- PostgreSQL: LISTEN/NOTIFY + Logical Decoding
- MySQL: Debezium CDC + event outbox
- SQL Server: Native Change Data Capture
- SQLite: Trigger-based (pull-only, development use)

**Supported transports (observed latencies in reference deployment):**

- `graphql-ws` — WebSocket for real-time UI updates (~5-10ms target, local network)
- HTTP Webhooks — Push-based delivery to external systems (50-200ms typical)
- Kafka/SQS/Kinesis — High-throughput event streaming for data platforms (target: 100k+ events/sec)
- gRPC — Future service-to-service streaming

**Subscription features:**

- Compile-time WHERE clause filters with authentication context (rules defined at compile-time)
- Runtime variables for additional runtime filtering (values bound safely at subscription establishment)
- Field projection same as queries (select only needed fields)
- Authorization enforcement (row-level, field-level) with runtime-safe parameter binding
- Per-entity event ordering with monotonic sequence numbers
- Replay capability from event buffer

**Database support (PostgreSQL is reference implementation):**

- PostgreSQL: LISTEN/NOTIFY + CDC (reference, full feature parity)
- MySQL: Debezium CDC
- SQL Server: Native CDC
- SQLite: Trigger-based (development-use only)

**Constraints:**

- Subscriptions are read-only (event projections only)
- Filters must be compile-time deterministic (no dynamic WHERE generation)
- No user code execution in subscriptions
- Event ordering guaranteed per-entity, not globally

**See:** `docs/architecture/realtime/subscriptions.md` for complete specification

### 5.3 Explicitly Not Supported

- Field resolvers
- Runtime hooks or middleware
- Executable directives
- Inline service calls
- Dynamic schemas
- Runtime schema stitching

### 5.4 Custom Scalars

Custom scalar semantics are defined at compile time. Input coercion and validation are enforced by the Rust runtime prior to execution. Databases may apply additional constraints, but runtime validation is authoritative for GraphQL correctness.

---

## 6. Composition & Scale

### 6.1 Federation

#### 6.1.1 Federation Model

FraiseQL implements **Apollo Federation v2** as a subgraph with three entity resolution strategies:

**Strategy 1: Local Resolution**

- Entity owned by current subgraph
- Direct database query to `v_{entity}`
- Latency: <5ms

**Strategy 2: Direct DB Federation** (Optimized for FraiseQL-to-FraiseQL)

- Entity in another FraiseQL subgraph
- Direct database connection (no HTTP)
- Rust runtime queries remote database views
- Supports multi-database: PostgreSQL ↔ SQL Server ↔ MySQL ↔ SQLite
- Latency: <10-20ms depending on network

**Strategy 3: HTTP Federation** (Standard for external subgraphs)

- Entity in non-FraiseQL subgraph (Apollo Server, Yoga, etc.)
- Standard Apollo Federation v2 protocol
- HTTP POST to external subgraph's `_entities` endpoint
- Automatic fallback when direct DB unavailable
- Latency: 50-200ms

**Key Principles:**

- ✅ Automatic strategy selection at compile time
- ✅ Each subgraph independently compiled for its database
- ✅ Database-specific WHERE operators preserved per subgraph
- ✅ Graceful fallback: If database unreachable, uses HTTP
- ✅ Full Apollo Federation v2 compliance

#### 6.1.2 Federation and Authorization

- Each subgraph has independent auth context
- Authorization rules evaluated per subgraph
- Cross-subgraph authorization via external fields and federation directives
- @key, @external, @requires, @provides fully supported
- No schema may weaken authorization guarantees of another (compile-time validation)

### 6.2 Cache Invalidation

Because execution is deterministic and writes are declarative, FraiseQL can emit:

- Entity-level invalidations
- Relationship-level invalidations
- Cascade invalidation hints

Cache invalidation is a **first-class output** of execution.

---

## 7. Operational Guarantees

### 7.1 Error Model

FraiseQL follows the standard GraphQL error shape:

- `message`
- `path`
- `locations`
- `extensions`

Standard error codes include:

| Code | Meaning |
|------|---------|
| `AUTH_REQUIRED` | Authentication required but not provided |
| `AUTH_FORBIDDEN` | Authorization requirements not met |
| `VALIDATION_ERROR` | GraphQL validation failed |
| `BINDING_ERROR` | Database binding failed |
| `DB_ERROR` | Database execution error |
| `UNSUPPORTED_FEATURE` | Feature not supported |
| `UNSUPPORTED_PLANE` | Data plane not available |

### 7.2 Versioning & Compatibility

- CompiledSchema includes explicit version metadata
- Runtime enforces compatibility guarantees
- Feature flags declare support for:
  - Arrow
  - Federation
  - Subscriptions

Backward compatibility is enforced at schema boundaries.

---

## 8. Non-Goals & Boundaries

### 8.1 Explicit Non-Goals

FraiseQL is **not**:

- A general-purpose web framework
- A resolver-based GraphQL server
- An ORM
- A workflow engine
- A service orchestration platform

### 8.2 Architectural Boundaries

| Concern | Belongs To | Does NOT Belong To |
|---------|------------|-------------------|
| Schema definition | Authoring layer | Runtime |
| Type validation | Compiler | Runtime (beyond compiled checks) |
| Query execution | Rust runtime | Authoring layer |
| Business logic | Database | Runtime or authoring layer |
| Authentication | External provider | FraiseQL |
| Authorization rules | Compiler (as metadata) | Runtime (as logic) |
| Data transformation | Database views | Runtime |

---

## 9. Database Capabilities Reference

### 9.1 PostgreSQL (Primary Target)

PostgreSQL is the primary supported database, enabling:

- **Transactional consistency**
- **Row-level security** for authorization enforcement
- **Rich type system** (JSONB, arrays, ranges, etc.)
- **Extension ecosystem** for specialized workloads

#### 9.1.1 Supported Extensions

| Extension | Capability | Operators Added |
|-----------|------------|-----------------|
| **pgvector** | Vector similarity search | `_cosine_distance`, `_l2_distance`, `_inner_product` |
| **PostGIS** | Geospatial queries | `_st_contains`, `_st_within`, `_st_distance`, `_st_intersects` |

Extensions must be explicitly declared in the schema configuration.

### 9.2 Future Database Targets

FraiseQL is designed to support additional databases. Each database will have its own capability manifest defining available operators.

FraiseQL does not standardize database features. Capabilities are exposed explicitly via bindings in the CompiledSchema. Database-specific features are exposed as **opaque bindings** — the runtime executes them but does not interpret their semantics.

---

## 10. Complete System Architecture

FraiseQL v2 is a **unified system** with three complementary data planes serving different access patterns, all compiled from a single schema and executed deterministically:

### 10.1 The Three-Plane System

**JSON Plane** — Interaction layer

- GraphQL queries and mutations
- Request-response semantics
- Nested object graphs (client-friendly)
- Keyset-based pagination
- Real-time caching and invalidation

**Arrow Plane** — Computation layer

- Columnar, batch-oriented analytics
- Pre-computed analytical projections
- Multiple entity batches per query
- High-throughput data access
- BI tool integration

**Delta Plane** — Change data layer

- Event-driven subscription model
- Database-native change capture (LISTEN/NOTIFY, CDC)
- Durable event buffer with replay capability
- Multiple transport adapters (graphql-ws, webhooks, Kafka, gRPC)
- Multi-tenant event filtering and authorization

**All three planes:**

- Source from the same database transactions
- Use identical authorization model (compile-time WHERE clauses)
- Share type system and schema bindings
- Support multi-database targets (PostgreSQL, MySQL, SQL Server, SQLite)
- Are fully compiled at build time (no runtime interpretation)

### 10.2 Why Three Planes?

| Plane | Access Pattern | Use Case | Protocol |
|-------|----------------|----------|----------|
| **JSON** | Pull (request-response) | "Give me this user" | HTTP/GraphQL |
| **Arrow** | Pull (bulk read) | "Give me 100K rows for analysis" | HTTP/Parquet |
| **Delta** | Push (event stream) | "Tell me when data changes" | WebSocket/Kafka |

One schema. Three optimized execution paths.

### 10.3 Complete Feature Set

FraiseQL v2 provides a complete system:

- ✅ **Language-agnostic authoring** (Python, TypeScript, YAML, GraphQL SDL)
- ✅ **Compile-time schema specialization** (database-specific operator availability)
- ✅ **Three data planes** (JSON, Arrow, Delta) fully integrated
- ✅ **Federation** (Apollo Federation v2 with database-level optimizations)
- ✅ **Authorization** (compile-time rules, row-level security, field masking)
- ✅ **Multi-database support** (PostgreSQL reference, MySQL/SQL Server/SQLite)
- ✅ **Caching** (query result caching, automatic invalidation)
- ✅ **Change data capture** (Debezium-compatible event format)
- ✅ **Event streaming** (multiple transport protocols)
- ✅ **Deterministic execution** (compiled SQL, no user code)

---

## 11. Design Invariants

These invariants are **non-negotiable** and must hold across all implementations:

1. **No user code executes at runtime**
   The runtime executes compiled plans only.

2. **All operators are compile-time validated**
   If a query compiles, the database can execute it.

3. **Authorization is monotonic**
   Auth rules can only restrict access, never expand it.

4. **Federation is compile-time only**
   No runtime service-to-service GraphQL calls.

5. **Arrow batches are single-level**
   Nested queries produce multiple batches, not nested structures.

6. **Subscriptions are database-driven**
   The runtime never polls or simulates events.

7. **The database owns all derived state**
   Computed fields, aggregations, and transformations live in database views.

8. **Errors are typed and predictable**
   Error codes are standardized and documented.

---

## Appendix A: Why No Resolvers?

Traditional GraphQL servers use resolvers — functions that execute for each field. FraiseQL forbids resolvers because:

1. **Resolvers create N+1 problems**
   Each field can trigger a database call. FraiseQL compiles to optimal query plans.

2. **Resolvers hide complexity**
   Business logic scattered across resolvers is hard to reason about. FraiseQL keeps logic in the database.

3. **Resolvers break determinism**
   Resolvers can have side effects, randomness, or external dependencies. FraiseQL execution is deterministic.

4. **Resolvers prevent compilation**
   You can't optimize what you can't analyze. Resolvers are opaque to the compiler.

5. **Resolvers couple execution to authoring**
   FraiseQL separates these concerns completely.

**The alternative:** Database views, stored procedures, and triggers provide the same power with better performance, transactional guarantees, and analyzability.

---

## Appendix B: Document History

| Version | Date | Changes |
|---------|------|---------|
| 2.0 | 2026-02-05 | Updated for v2.0.0-alpha.1 release; Arrow and subscriptions marked as implemented |
| 1.0 | 2026-01-11 | Initial PRD |

---

## Appendix C: Related Documents

### Specifications

- `specs/compiled-schema.md` — CompiledSchema JSON specification
- `specs/capability-manifest.md` — Database capability manifest format
- `specs/cdc-format.md` — Change Data Capture event format
- `specs/schema-conventions.md` — Database schema conventions reference

### Architecture Documents

- `architecture/integration/federation.md` — Hybrid HTTP + database-level federation
- `architecture/realtime/subscriptions.md` — Database-driven subscription architecture
- `architecture/database/arrow-plane.md` — Columnar Arrow data plane for analytics
- `architecture/core/execution-model.md` — Query/mutation/subscription execution pipeline
- `architecture/core/compilation-pipeline.md` — Compilation phases (1-6)
- `architecture/database/database-targeting.md` — Multi-database specialization

### Additional Architecture Documents

- `architecture/security/authentication-detailed.md` — Authentication and authorization flow
- `architecture/security/security-model.md` — Security model overview
- `architecture/core/execution-semantics.md` — JSONB projection and composition patterns
- `architecture/analytics/aggregation-model.md` — Analytical query patterns

---

*End of PRD*
