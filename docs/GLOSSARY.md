# FraiseQL v2 Glossary

**Version:** 1.0
**Date:** January 11, 2026
**Status:** Complete
**Purpose:** Centralized terminology reference for all FraiseQL v2 documentation

---

## How to Use This Glossary

- **Terms are alphabetically ordered**
- **Each term links to relevant specifications**
- **Bold text** indicates the canonical definition
- *Italics* indicate alternative names or deprecated terms
- See also: related terms worth understanding together

---

## A

### APQ (Automatic Persisted Queries)

**A protocol for registering and reusing GraphQL queries by hash.**

APQ allows clients to send a SHA-256 hash of a query instead of the full query text. The server stores the hash→query mapping and executes queries by hash lookup. This:

- Reduces network bandwidth (hash is smaller than query)
- Enables query allowlisting for security
- Improves cache hit rates

**Three security modes:**

1. **OPTIONAL** — Accept both hashed and full queries
2. **REQUIRED** — Only accept hashed queries (allowlist security)
3. **DISABLED** — Ignore APQ extension, require full queries

**Important:** APQ is NOT query result caching. It stores query text by hash, not execution results.

**Related specs:**

- `docs/specs/persisted-queries.md` — Complete APQ specification
- `docs/specs/caching.md` — Query result caching (different feature)

**See also:** Query Result Caching, Cache Invalidation

---

### Arrow Plane

**An optional columnar data projection format for analytics workloads.**

Arrow plane produces typed, columnar batches instead of nested JSON objects. Benefits:

- Batch-oriented (not row-oriented)
- Zero-copy deserialization
- Efficient for OLAP queries, BI tools, ML pipelines

**Content-Type:** `application/x-arrow`

**Key difference from JSON plane:**

- JSON: Nested object graph in single response
- Arrow: Multiple flat batches with explicit key references

**Related specs:**

- `docs/architecture/database/arrow-plane.md` — Arrow plane architecture
- `docs/prd/PRD.md` Section 3.4.2 — Overview

**See also:** JSON Plane, Projection

---

### Audit Columns

**Standard timestamp and user tracking columns required in all FraiseQL tables.**

All tables must include:

- `created_at` (TIMESTAMPTZ) — Creation timestamp
- `created_by` (INTEGER FK) — User who created
- `updated_at` (TIMESTAMPTZ) — Last update timestamp
- `updated_by` (INTEGER FK) — User who updated
- `deleted_at` (TIMESTAMPTZ) — Soft delete timestamp (NULL if active)
- `deleted_by` (INTEGER FK) — User who deleted

**Enables:**

- Automatic audit trails
- Soft delete with temporal queries
- Cache invalidation based on `updated_at`
- CDC event generation

**Related specs:**

- `docs/specs/schema-conventions.md` Section 3 — Audit columns
- `docs/enterprise/audit-logging.md` — Audit event logging
- `docs/prd/PRD.md` Section 3.2 — Schema conventions

**See also:** Soft Delete, CDC, Schema Conventions

---

### AuthContext

**Immutable, typed data structure representing authenticated user identity and claims.**

Produced by external authentication providers and consumed by authorization enforcement. Contains:

- `subject` — User identifier
- `roles` — Array of role names
- `claims` — Key-value map of additional attributes (tenant_id, email, etc.)

**Properties:**

- Schema-declared at compile time
- Validated at runtime
- Immutable during request execution
- Does not participate in query execution (only authorization checks)

**Related specs:**

- `docs/prd/PRD.md` Section 4 — Security model
- `docs/specs/security-compliance.md` — Authentication integration
- `docs/enterpri../../guides/authorization-quick-start.md` — Role-based authorization

**See also:** Authorization, Authentication Provider

---

### Authentication Provider

**External system responsible for validating user credentials and producing AuthContext.**

FraiseQL does NOT authenticate users itself. Authentication is delegated to pluggable providers:

- JWT / session-based providers
- OAuth2 / OIDC providers
- Auth0, Keycloak, etc.
- Custom providers via defined interface

**Provider responsibilities:**

1. Validate incoming credentials
2. Produce typed, immutable AuthContext
3. Do not mutate or observe query execution

**Related specs:**

- `docs/prd/PRD.md` Section 4.2 — Authentication
- `docs/specs/security-compliance.md` — Provider integration

**See also:** AuthContext, Authorization

---

### Authorization

**Declarative, compile-time rules enforced at runtime to restrict data access.**

Authorization in FraiseQL:

- Declared at compile time (not runtime logic)
- Enforced at runtime deterministically
- Uses AuthContext for decision input
- Three enforcement layers:
  1. Pre-execution rejection (cheapest)
  2. Database-level (row-level security, scoped views)
  3. Post-projection filtering (last resort)

**NOT allowed:**

- Resolver-level authorization logic
- Dynamic permission checks
- Runtime directives or hooks
- Imperative branching based on auth

**Related specs:**

- `docs/prd/PRD.md` Section 4.3 — Authorization
- `docs/enterpri../../guides/authorization-quick-start.md` — Role-based access control
- `docs/specs/authoring-contract.md` — Authorization declarations

**See also:** AuthContext, RBAC, Field-Level Authorization

---

### AuthoringIR (Authoring Intermediate Representation)

**Language-agnostic intermediate representation that all authoring languages compile to.**

AuthoringIR unifies schema definitions from Python, TypeScript, YAML, GraphQL SDL, and CLI into a single canonical structure before compilation. This enables:

- Organization-scale language choice (one canonical language per org)
- Identical execution plans regardless of authoring language
- Translation tools for migration between languages

**Compilation flow:**

```text
Python/TypeScript/YAML/SDL/CLI → AuthoringIR → Compilation Pipeline → CompiledSchema
```text

**Related specs:**

- `docs/architecture/core/authoring-languages.md` — Language-agnostic authoring
- `docs/architecture/core/compilation-pipeline.md` Phase 1b — IR building
- `docs/prd/PRD.md` Principle 10 — Language-agnostic authoring

**See also:** CompiledSchema, Compilation Pipeline

---

## B

### Backend Lowering

**Process of translating WHERE filters from SDL (Schema Definition Language) predicates to database-specific SQL.**

Each database target has a lowering module:

- `backends/postgresql.rs` — PostgreSQL-specific SQL generation
- `backends/mysql.rs` — MySQL-specific SQL generation
- `backends/sqlite.rs` — SQLite-specific SQL generation
- `backends/sqlserver.rs` — SQL Server-specific SQL generation

**Key property:** No runtime translation or emulation. If an operator isn't in the capability manifest, it cannot be expressed in the GraphQL schema.

**Related specs:**

- `docs/architecture/database/database-targeting.md` Section 3 — Backend lowering
- `docs/architecture/core/execution-model.md` Phase 4 — Database execution

**See also:** Database Capability Manifest, Database Target, WHERE Operators

---

### Binding

**Connection between a GraphQL type and a database view or stored procedure.**

Bindings declare how types map to database resources:

- **Read bindings** — Type → database view (e.g., `User` → `v_user`)
- **Write bindings** — Mutation → stored procedure (e.g., `createUser` → `fn_create_user`)

**Properties:**

- Declared in authoring schema
- Validated at compile time
- One type can have multiple bindings (multi-projection)
- One view can back multiple types

**Related specs:**

- `docs/specs/authoring-contract.md` Section 4 — Bindings
- `docs/prd/PRD.md` Section 3.1 — Database contract

**See also:** Projection, View

---

## C

### Cache Invalidation

**Process of emitting signals indicating which cached data is stale after mutations.**

FraiseQL mutations return cascade metadata indicating:

- **Updated entities** — IDs of modified entities
- **Deleted entities** — IDs of removed entities
- **Invalidations** — Type-level or relationship-level invalidation hints

**Deterministic:** Because execution is deterministic and writes are declarative, invalidation signals are predictable and complete.

**Integrations:**

- graphql-cascade library (JavaScript/TypeScript clients)
- Custom cache invalidation handlers

**Related specs:**

- `docs/specs/caching.md` Section 4 — Cache invalidation
- `docs/specs/persisted-queries.md` — APQ + caching
- `docs/architecture/core/execution-model.md` Phase 6 — Cache invalidation emission

**See also:** Query Result Caching, APQ, Cascade Metadata

---

### Capability Manifest

**Static JSON declaration of which WHERE operators each database supports.**

The capability manifest is the source of truth for multi-database support. For each database target (PostgreSQL, MySQL, SQLite, etc.), it declares:

- Scalar type operators (string, numeric, boolean, etc.)
- Complex type operators (JSONB, arrays, vectors, etc.)
- Extension operators (pgvector, PostGIS, LTree, etc.)
- Logical combinators (_and,_or, _not)

**Properties:**

- Static (checked into version control)
- Declarative (not code)
- Extensible (add new databases by adding manifest entries)
- Source of truth for compiler Phase 4 (WHERE type generation)

**Related specs:**

- `docs/architecture/database/database-targeting.md` Section 2 — Capability manifest
- `docs/prd/PRD.md` Section 3.3 — Compile-time database specialization

**See also:** Database Target, WHERE Operators, Backend Lowering

---

### Cascade Metadata

**Structured data returned by mutations indicating which cached data should be invalidated.**

Format:

```json
{
  "status": "success",
  "entity": { ... },
  "cascade": {
    "updated": ["user:123", "post:456"],
    "deleted": ["comment:789"],
    "invalidations": ["user:123.posts", "post:456.comments"]
  }
}
```text

**Related specs:**

- `docs/specs/caching.md` Section 4 — Cache invalidation
- `docs/architecture/core/execution-model.md` Phase 6 — Emission

**See also:** Cache Invalidation, Query Result Caching

---

### CDC (Change Data Capture)

**System for capturing and streaming database changes as structured events.**

FraiseQL emits Debezium-compatible CDC events containing:

- Before/after snapshots
- Event type (insert, update, delete)
- Business semantics (entity ID, tenant ID, etc.)
- Cryptographic hash chain for integrity
- HMAC signatures for authenticity

**Use cases:**

- Real-time subscriptions
- Audit trails
- Event-driven architectures
- Cross-system synchronization

**Related specs:**

- `docs/specs/cdc-format.md` — Event format specification
- `docs/enterprise/audit-logging.md` — Enterprise audit system

**See also:** Audit Logging, Subscriptions

---

### CompiledSchema

**Immutable JSON artifact produced by compiler, consumed by Rust runtime.**

CompiledSchema contains:

- Type system (all types, fields, scalars)
- Query and mutation definitions
- Database bindings (JSON + Arrow)
- Authorization requirements (metadata, not logic)
- Federation metadata
- Database capability manifest
- Feature flags and versioning

**Properties:**

- Pure data (no executable code)
- Database-target-specific (PostgreSQL CompiledSchema differs from MySQL)
- Serializable and versionable (git-friendly)
- Immutable at runtime (changes require recompilation)

**Important:** CompiledSchema is NOT:

- The database schema (that's schema conventions)
- A GraphQL schema (that's schema.graphql, generated from CompiledSchema)
- Runtime-mutable
- The source of truth for types (authoring schema is; this is derived)

**Related specs:**

- `docs/specs/compiled-schema.md` — Complete JSON specification
- `docs/prd/PRD.md` Section 2 — System architecture

**See also:** AuthoringIR, Compilation Pipeline

---

### Compilation Pipeline

**Multi-phase process transforming authoring schema into CompiledSchema.**

**Phases:**

1. **Phase 1a:** Language-specific parsing (Python AST, TypeScript AST, etc.)
2. **Phase 1b:** AuthoringIR building (unify language-specific types)
3. **Phase 2:** Type system resolution
4. **Phase 3:** Binding validation
5. **Phase 4:** WHERE type generation (database-specific)
6. **Phase 5:** SQL lowering
7. **Phase 6:** CompiledSchema serialization

**Related specs:**

- `docs/architecture/core/compilation-pipeline.md` — Complete pipeline specification
- `docs/architecture/core/authoring-languages.md` — Language-agnostic authoring

**See also:** AuthoringIR, CompiledSchema, Database Target

---

## D

### Database Capability Manifest

*See:* **Capability Manifest**

---

### Database Target

**Configuration parameter specifying which database the schema compiles for.**

Example:

```python
config = CompilerConfig(
    database_target="postgresql",  # or "mysql", "sqlite", "sqlserver"
    schema_path="schema.py",
    output_dir="build/"
)
```text

**Impact:**

- Drives WHERE operator availability
- Affects SQL generation (backend lowering)
- Determines scalar type support
- Influences JSONB/vector/extension operators

**Key principle:** Same schema source, different compiled outputs per database target.

**Related specs:**

- `docs/architecture/database/database-targeting.md` — Complete multi-database architecture
- `docs/prd/PRD.md` Section 3.3 — Compile-time specialization

**See also:** Capability Manifest, Backend Lowering, WHERE Operators

---

### Dual-Key Strategy

**FraiseQL's three-key system for optimal performance and usability.**

Every entity has:

1. **`pk_{entity}` (INTEGER)** — Primary key for internal joins (4 bytes, max performance)
2. **`id` (UUID)** — External identity, globally unique, exposed via GraphQL (16 bytes)
3. **`identifier` (TEXT)** — Human-readable slug for URLs (e.g., "john-doe")

**Why integers for joins?**

- 4 bytes vs 16 bytes (UUIDs)
- Faster B-tree index operations
- Better cache efficiency

**Why UUIDs for external identity?**

- Globally unique across federated schemas
- No information leakage (unlike sequential integers)
- Client-side generation possible
- Cache key compatibility

**Why identifiers (slugs)?**

- Human-readable URLs: `/users/john-doe`
- No joins required for lookup (indexed)
- SEO-friendly

**Related specs:**

- `docs/specs/schema-conventions.md` Section 2 — Column conventions
- `docs/prd/PRD.md` Section 3.2 — Schema conventions

**See also:** Schema Conventions, Primary Key, Trinity Pattern

---

## E

### Execution Model

**The deterministic, phase-based process the Rust runtime follows to execute GraphQL requests.**

**Phases:**

- **Phase 0:** Request preparation (APQ resolution, cache check)
- **Phase 1:** GraphQL validation
- **Phase 2:** Authorization enforcement
- **Phase 3:** Query planning
- **Phase 4:** Database execution
- **Phase 5:** Result projection
- **Phase 6:** Cache invalidation emission

**Properties:**

- Deterministic (same input always produces same output)
- No user code execution
- No runtime schema interpretation
- Fixed at compile time

**Related specs:**

- `docs/architecture/core/execution-model.md` — Complete execution specification
- `docs/prd/PRD.md` Section 2.3 — Runtime responsibilities

**See also:** CompiledSchema, Compilation Pipeline

---

## F

### Federation

**Compile-time composition of multiple CompiledSchemas into a single unified execution plan.**

FraiseQL federation is NOT runtime GraphQL-to-GraphQL calls. Instead:

- Multiple schemas compile into single unified execution plan
- Cross-schema entity resolution via coordinated database joins
- Static entity keys
- Explicit ownership

**Properties:**

- Compile-time only (no runtime service calls)
- Shared auth context across schemas
- Authorization rules validated across boundaries
- Conflicting rules are compile-time errors

**Related specs:**

- `docs/architecture/integration/federation.md` — Federation architecture
- `docs/adrs/ADR-009-federation-architecture.md` — Federation design decisions
- `docs/prd/PRD.md` Section 6.1 — Federation model

**See also:** CompiledSchema, Entity Resolution

---

### Field-Level Authorization

**Authorization rules applied to individual fields within a type.**

Example: All users can see `User.name`, but only admins can see `User.email`.

**Enforcement:**

- Compile-time declaration in authoring schema
- Runtime enforcement during projection
- Deterministic (field present or absent based on AuthContext)
- Monotonic (only removes data, never adds)

**Performance:**

- Rust FFI filtering: <1 µs overhead per field
- Python filtering: 5-10 µs overhead per field

**Related specs:**

- `docs/enterpri../../guides/authorization-quick-start.md` Section 6 — Field-level authorization
- `docs/architecture/core/execution-model.md` Section 7.3 — Filtering during projection

**See also:** Authorization, RBAC

---

## G

### GraphQL SDL (Schema Definition Language)

**One of the supported authoring languages for defining FraiseQL schemas.**

SDL is the GraphQL native schema language:

```graphql
type User {
  id: ID!
  name: String!
  email: String
}
```text

**In FraiseQL:**

- SDL is an authoring language (not the only one)
- Compiles to AuthoringIR like all other languages
- Produces identical execution plans as Python/TypeScript/YAML/CLI

**Related specs:**

- `docs/architecture/core/authoring-languages.md` Section 5 — GraphQL SDL authoring
- `docs/specs/authoring-contract.md` — Type declarations

**See also:** AuthoringIR, Authoring Languages

---

## I

### Introspection

**GraphQL introspection protocol for schema reflection.**

FraiseQL supports three introspection policies:

1. **DISABLED** — No introspection (production default)
2. **AUTHENTICATED** — Introspection requires authentication
3. **PUBLIC** — Anyone can introspect (development only)

**Security consideration:** Introspection exposes full schema structure including types, fields, and authorization metadata. Disable in production unless schema is intentionally public.

**Related specs:**

- `docs/specs/introspection.md` — Introspection policies and security
- `docs/specs/security-compliance.md` — Security profiles

**See also:** Security Profile, Authentication

---

## J

### JSON Plane

**Default data projection format returning nested JSON objects.**

**Content-Type:** `application/json`

**Properties:**

- Nested object graph in single response
- Frontend-oriented
- Human-readable
- Efficient for OLTP (transactional) queries

**Example:**

```json
{
  "data": {
    "user": {
      "id": "123",
      "name": "Alice",
      "posts": [
        {"id": "456", "title": "Hello"}
      ]
    }
  }
}
```text

**Related specs:**

- `docs/prd/PRD.md` Section 3.4.1 — JSON plane
- `docs/architecture/core/execution-model.md` Phase 5 — Projection

**See also:** Arrow Plane, Projection

---

### JSONB Composition

**Strategy for composing nested types using PostgreSQL JSONB aggregation instead of ORM-style relationships.**

Each read view produces a `data` JSONB column containing the full projection. Nested fields compose projections via pre-aggregated views:

```sql
-- Pre-aggregated: posts grouped by user
CREATE VIEW v_posts_by_user AS
SELECT
    fk_user,
    jsonb_agg(data) AS posts
FROM v_post
GROUP BY fk_user;

-- Composition: join user + pre-aggregated posts
CREATE VIEW v_user_with_posts AS
SELECT
    u.data || jsonb_build_object('posts', COALESCE(p.posts, '[]'::jsonb)) AS data
FROM v_user u
LEFT JOIN v_posts_by_user p ON p.fk_user = u.pk_user;
```text

**Benefits:**

- Zero-cost projection (database does all composition)
- O(1) relationship composition (not N+1)
- Flexibility (add fields without schema migration)

**Related specs:**

- `docs/prd/PRD.md` Section 3.1.5 — Projection composition
- `docs/specs/schema-conventions.md` Section 4 — View composition

**See also:** Pre-Aggregated View, Projection, View

---

## M

### Mutation

**GraphQL mutation mapped to a stored procedure or database function.**

Mutations in FraiseQL:

- Invoked as stored procedures/functions (not resolvers)
- Input: GraphQL arguments → validated JSON payload
- Execution: Transactional in database
- Output: Procedure returns JSON with `status`, `entity`, `cascade`

**Example:**

```graphql
mutation {
  createUser(input: {name: "Alice", email: "alice@example.com"}) {
    id
    name
  }
}
```text

Executes: `fn_create_user(jsonb)` stored procedure.

**Related specs:**

- `docs/prd/PRD.md` Section 3.1.2 — Write model
- `docs/specs/authoring-contract.md` — Mutation declarations

**See also:** Stored Procedure, Binding, Cascade Metadata

---

## P

### Pre-Aggregated View

**Database view that groups related entities by foreign key for efficient composition.**

Pattern: `v_{entities}_by_{parent}`

Example:

```sql
CREATE VIEW v_posts_by_user AS
SELECT
    fk_user,
    jsonb_agg(data ORDER BY created_at DESC) AS posts
FROM v_post
GROUP BY fk_user;
```text

**Benefits:**

- O(1) composition (single join, not N queries)
- Database-owned (not runtime logic)
- Efficient (grouped once, joined many times)

**Related specs:**

- `docs/specs/schema-conventions.md` Section 4 — View patterns
- `docs/prd/PRD.md` Section 3.1.5 — Composition

**See also:** JSONB Composition, View, Schema Conventions

---

### Primary Key

**Internal integer key used for high-performance joins.**

Convention: `pk_{entity}` (e.g., `pk_user`, `pk_post`)

**Why integers?**

- 4 bytes (vs 16 bytes for UUID)
- Faster B-tree operations
- Better cache efficiency

**Not exposed via GraphQL** — clients use `id` (UUID) instead.

**Related specs:**

- `docs/specs/schema-conventions.md` Section 2 — Column conventions
- `docs/prd/PRD.md` Section 3.2 — Dual-key strategy

**See also:** Dual-Key Strategy, Foreign Key, UUID

---

### Projection

**The process of transforming database results into GraphQL response shape.**

Two forms:

1. **Type projection** — Mapping database view columns to GraphQL type fields
2. **Result projection** — Extracting requested fields from JSONB `data` column

**In execution:**

- Happens in Phase 5 (result projection)
- Uses compiled projection rules (not runtime interpretation)
- Field-level authorization applied during projection

**Related specs:**

- `docs/architecture/core/execution-model.md` Phase 5 — Result projection
- `docs/prd/PRD.md` Section 3.1 — Read model

**See also:** JSONB Composition, View, Binding

---

## Q

### Query Result Caching

**Caching of GraphQL query execution results (not to be confused with APQ).**

Caches the full response of a query:

- Key: Query hash + arguments + auth context
- Value: JSON response
- TTL: Configurable (default 5-60 minutes)
- Backends: Memory, Database (PostgreSQL), Custom

**Invalidation:**

- Automatic via cascade metadata from mutations
- Manual via cache invalidation API
- TTL-based expiration

**Important:** This is different from APQ (which caches query text, not results).

**Related specs:**

- `docs/specs/caching.md` — Query caching specification
- `docs/specs/persisted-queries.md` — APQ (different feature)

**See also:** APQ, Cache Invalidation, Cascade Metadata

---

## R

### RBAC (Role-Based Access Control)

**Hierarchical role system for authorization with multi-layer caching.**

**Features:**

- Role hierarchy with inheritance
- Field-level authorization
- Row-level security integration
- Per-tenant RBAC

**Performance:**

- Request-level cache: <1 µs
- PostgreSQL UNLOGGED cache: 0.1-0.3 ms
- Domain versioning for automatic invalidation

**Related specs:**

- `docs/enterpri../../guides/authorization-quick-start.md` — Complete RBAC specification
- `docs/prd/PRD.md` Section 4.3 — Authorization

**See also:** Authorization, Field-Level Authorization, AuthContext

---

## S

### Schema Conventions

**Opinionated database schema patterns required by FraiseQL.**

**Naming:**

- `tb_{entity}` — Write tables
- `v_{entity}` — Read views
- `v_{entities}_by_{parent}` — Pre-aggregated views
- `fn_{action}_{entity}` — Stored procedures

**Columns:**

- `pk_{entity}` (INTEGER) — Primary key
- `fk_{entity}` (INTEGER) — Foreign key
- `id` (UUID) — External identifier
- `identifier` (TEXT) — Human-readable slug
- `data` (JSONB) — Projection output

**Audit columns:**

- `created_at`, `created_by`, `updated_at`, `updated_by`, `deleted_at`, `deleted_by`

**Related specs:**

- `docs/specs/schema-conventions.md` — Complete conventions reference
- `docs/prd/PRD.md` Section 3.2 — Overview

**See also:** Dual-Key Strategy, Pre-Aggregated View, Audit Columns

---

### Security Profile

**Predefined security configuration level.**

Three profiles:

1. **STANDARD** — Basic security (development, internal tools)
2. **REGULATED** — Enhanced security, TLS required (production, regulated industries)
3. **RESTRICTED** — Maximum security, mTLS required (high-security environments)

Each profile configures:

- Security headers (CSP, HSTS, etc.)
- Introspection policy
- Rate limiting
- CSRF protection
- Token requirements

**Related specs:**

- `docs/specs/security-compliance.md` Section 2 — Security profiles
- `docs/prd/PRD.md` Section 4 — Security model

**See also:** Introspection, Rate Limiting

---

### Soft Delete

**Pattern of marking records as deleted without physically removing them.**

Uses `deleted_at` audit column:

- `NULL` = active
- `TIMESTAMPTZ` = soft deleted

**Read views filter soft-deleted records:**

```sql
CREATE VIEW v_user AS
SELECT ... FROM tb_user WHERE deleted_at IS NULL;
```text

**Benefits:**

- Audit trail preservation
- Temporal queries (see historical state)
- Cache invalidation tracking
- Accidental deletion recovery

**Related specs:**

- `docs/specs/schema-conventions.md` Section 3 — Audit columns
- `docs/prd/PRD.md` Section 3.2 — Conventions

**See also:** Audit Columns, Schema Conventions

---

### Stored Procedure

**Database function that executes mutations transactionally.**

Pattern: `fn_{action}_{entity}` (e.g., `fn_create_user`, `fn_update_post`)

**Input:** JSON payload (validated GraphQL arguments)

**Output:** JSON with structure:

```json
{
  "status": "success|error|noop",
  "entity": { ... },
  "cascade": {
    "updated": [...],
    "deleted": [...],
    "invalidations": [...]
  }
}
```text

**Related specs:**

- `docs/prd/PRD.md` Section 3.1.2 — Write model
- `docs/specs/schema-conventions.md` Section 5 — Stored procedures

**See also:** Mutation, Binding, Cascade Metadata

---

### Subscriptions

**Compiled database event projections delivered via multiple transport adapters.**

FraiseQL subscriptions are NOT GraphQL resolver-based subscriptions. Instead:

- Events originate from database transactions (source of truth)
- Subscriptions are declared at schema definition time (compile-time)
- Events are buffered in `tb_entity_change_log` (durability)
- Multiple transport adapters (graphql-ws, webhooks, Kafka) consume same event stream

**Key characteristics:**

- Database-native (LISTEN/NOTIFY, CDC)
- Deterministic, no user code execution
- Compile-time WHERE filters + runtime variables
- Row-level authorization enforcement
- Per-entity event ordering

**Supported transports:**

- `graphql-ws` — WebSocket for real-time UI updates
- Webhooks — HTTP POST to external systems
- Kafka/SQS — Event streaming to data platforms
- gRPC — Future service-to-service streaming

**Database support (PostgreSQL is reference implementation):**

- PostgreSQL: LISTEN/NOTIFY + CDC
- MySQL: Debezium CDC
- SQL Server: Native CDC
- SQLite: Trigger-based

**Related specs:**

- `docs/architecture/realtime/subscriptions.md` — Complete subscription architecture and implementation
- `docs/specs/cdc-format.md` — CDC event structure (events originate here)
- `docs/prd/PRD.md` Section 5.2 — Subscription requirements
- `docs/specs/schema-conventions.md section 6` — Event buffering table (`tb_entity_change_log`)

**See also:** CDC, Transport Adapter, Subscription Filter, Subscription Variable, WebSocket

---

### Subscription Filter

**Compile-time WHERE clause that determines which events match a subscription.**

Subscription filters:

- Defined using `WhereEntity` types at schema authoring time
- Compiled to SQL predicates
- Eligible for database indexing and optimization
- Can reference authentication context (e.g., user_id, org_id, role)

**Example:**

```python
@FraiseQL.subscription
class OrderCreated:
    where: WhereOrder = FraiseQL.where(
        user_id=FraiseQL.context.user_id  # Only current user's orders
    )
```text

**Distinguished from runtime variables** — Filters are static, variables are dynamic (see Subscription Variable).

**Related specs:**

- `docs/architecture/realtime/subscriptions.md` section 5 — Filtering & Variables

**See also:** Subscriptions, Subscription Variable, Authorization

---

### Subscription Variable

**Typed runtime variable that further filters subscription events at execution time.**

Subscription variables:

- Declared explicitly in subscription schema (type-safe)
- Provided by client at subscription time (runtime)
- Can modify filters (e.g., date range, amount threshold)
- Validated by compiler against WHERE operators

**Example:**

```python
@FraiseQL.subscription
class OrderCreated:
    where: WhereOrder = FraiseQL.where(user_id=context.user_id)

    @FraiseQL.variable(name="since_date")
    class Filter:
        created_at: DateTimeRange  # Runtime variable
```text

**Client usage:**

```graphql
subscription OrderCreated($since_date: DateTime) {
  orderCreated(since_date: $since_date) {
    id amount created_at
  }
}
```text

**Distinguished from compile-time filters** — Variables are client-provided, filters are schema-defined.

**Related specs:**

- `docs/architecture/realtime/subscriptions.md` section 5 — Filtering & Variables

**See also:** Subscriptions, Subscription Filter

---

### Transport Adapter

**Pluggable module that delivers events from the subscription system to different destinations.**

FraiseQL uses a layered architecture:

```text
Database Event Stream (LISTEN/NOTIFY, CDC)
         ↓
Subscription Matcher (Filter evaluation)
         ↓
Transport Adapters
    ├─ graphql-ws (WebSocket)
    ├─ Webhook (HTTP POST)
    ├─ Kafka (Event stream)
    └─ gRPC (Service-to-service)
```text

**Each adapter handles:**

- Connection lifecycle (establish, authenticate, maintain, close)
- Message transformation (event → transport format)
- Retry logic and backpressure
- Delivery semantics (at-least-once, exactly-once, etc.)

**Example: Webhook adapter**

```python
config = FraiseQLConfig(
    webhooks={
        "OrderCreated": {
            "url": "https://analytics.example.com/events",
            "retry_max_attempts": 3,
            "retry_backoff_seconds": [1, 5, 30]
        }
    }
)
```text

**Related specs:**

- `docs/architecture/realtime/subscriptions.md` section 4 — Transport Protocols
- `docs/architecture/realtime/subscriptions.md` section 9 — Performance Characteristics

**See also:** Subscriptions, Event Buffer

---

### Event Buffer (Subscriptions Context)

**Persistent table (`tb_entity_change_log`) that stores database change events for durability and replay.**

The event buffer serves four purposes in subscriptions:

1. **Durability** — Events persisted across system restarts
2. **Replay** — Clients can request events from any point in time
3. **Backpressure** — Slow subscribers don't block event generation
4. **Ordering** — Monotonic sequence numbers ensure per-entity ordering

**Structure:**

```sql
tb_entity_change_log (
    id BIGINT PRIMARY KEY,
    object_type TEXT,           -- Entity type
    object_id UUID,             -- Entity ID
    modification_type TEXT,     -- INSERT|UPDATE|DELETE
    object_data JSONB,          -- Debezium envelope
    created_at TIMESTAMPTZ
)
```text

**Event delivery timeline:**

```text
Database Transaction (Commit)
    → LISTEN/NOTIFY notification (<1ms)
    → tb_entity_change_log insert (1-5ms)
    → Subscription matching (1-2ms)
    → Transport delivery (5-100ms depending on transport)
```text

**Retention policy:** Configurable (default: 30 days)

**Related specs:**

- `docs/specs/cdc-format.md` — CDC event format
- `docs/specs/schema-conventions.md section 6` — Table schema and indices
- `docs/architecture/realtime/subscriptions.md` section 2 — Architecture

**See also:** CDC, Subscriptions, Transport Adapter

---

## T

### Trinity Pattern

*Alternative name for:* **Dual-Key Strategy**

The "trinity" refers to the three identifiers: `pk_*`, `id`, `identifier`.

**See:** Dual-Key Strategy

---

## U

### UUID (Universally Unique Identifier)

**External entity identifier exposed via GraphQL.**

Column: `id` (UUID type, 16 bytes)

**Why UUIDs?**

- Globally unique (federated schemas)
- No information leakage
- Client-side generation possible
- Cache key compatible

**Not used for joins** — internal joins use `pk_*` (INTEGER) for performance.

**Related specs:**

- `docs/specs/schema-conventions.md` Section 2 — Column conventions
- `docs/prd/PRD.md` Section 3.2 — Dual-key strategy

**See also:** Dual-Key Strategy, Primary Key

---

## V

### View

**Logical database view exposing projections for GraphQL types.**

**Read view pattern:** `v_{entity}` (e.g., `v_user`, `v_post`)

**Structure:**

```sql
CREATE VIEW v_user AS
SELECT
    pk_user,
    id,
    identifier,
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email
    ) AS data
FROM tb_user
WHERE deleted_at IS NULL;
```text

**Key columns:**

- `pk_{entity}` — Internal primary key
- `id` — UUID for external identity
- `identifier` — Human-readable slug
- `data` — JSONB projection output

**Related specs:**

- `docs/specs/schema-conventions.md` Section 4 — View patterns
- `docs/prd/PRD.md` Section 3.1.1 — Read model

**See also:** Projection, Binding, JSONB Composition

---

## W

### WHERE Operators

**Filter operators available in GraphQL `where` input arguments.**

Examples:

- **Equality:** `_eq`, `_neq`
- **Comparison:** `_lt`, `_gt`, `_lte`, `_gte`
- **Inclusion:** `_in`, `_nin`
- **Pattern:** `_like`, `_ilike`, `_regex`, `_similar`
- **JSONB:** `_contains`, `_has_key`, `_jsonb_path`
- **Array:** `_overlaps`, `_contains`
- **Vector:** `_cosine_distance`, `_l2_distance`
- **Geospatial:** `_st_contains`, `_st_within`

**Database-specific:** PostgreSQL gets 60+ operators, MySQL gets 20+, SQLite gets 15.

**Related specs:**

- `docs/reference/where-operators.md` — Complete operator reference
- `docs/architecture/database/database-targeting.md` — Multi-database operator generation

**See also:** Database Target, Capability Manifest, Backend Lowering

---

### WHERE Type

**GraphQL input type for filtering queries.**

Generated automatically by compiler based on database target capabilities.

Example:

```graphql
input UserWhereInput {
  id: IDFilter
  name: StringFilter
  email: StringFilter
  _and: [UserWhereInput!]
  _or: [UserWhereInput!]
  _not: UserWhereInput
}

input StringFilter {
  _eq: String
  _neq: String
  _like: String
  _ilike: String      # PostgreSQL only
  _regex: String      # PostgreSQL only
}
```text

**Database-specific:** Same schema source produces different WHERE types per database target.

**Related specs:**

- `docs/architecture/database/database-targeting.md` Section 4 — WHERE type generation
- `docs/prd/PRD.md` Section 3.3 — Compile-time specialization

**See also:** WHERE Operators, Database Target

---

## Cross-Reference Index

### By Feature Area

**Compilation:**

- AuthoringIR
- Compilation Pipeline
- CompiledSchema
- Database Target

**Execution:**

- Execution Model
- Projection
- Backend Lowering
- Query Result Caching

**Database:**

- Schema Conventions
- View
- Stored Procedure
- Binding
- Pre-Aggregated View

**Security:**

- Authentication Provider
- AuthContext
- Authorization
- RBAC
- Field-Level Authorization
- Security Profile

**Performance:**

- Dual-Key Strategy
- JSONB Composition
- Query Result Caching
- APQ

**Operations:**

- Introspection
- CDC
- Audit Columns
- Soft Delete

**Federation:**

- Federation v2
- Subgraph
- Entity Resolution
- Direct DB Federation
- @key
- @external
- @requires
- @provides

---

### Federation v2

**Apollo Federation v2 protocol for composing multiple GraphQL subgraphs into a single federated graph.**

FraiseQL implements Federation v2 as a subgraph (not a gateway), using Apollo Router or compatible gateway for composition.

**Key concepts:**

- **Subgraph**: A FraiseQL backend that exposes `_service` and `_entities` endpoints
- **Entity**: A type with `@key` that can be resolved across subgraphs
- **Gateway**: Apollo Router or compatible federation-capable gateway that composes subgraphs

**Three resolution strategies in FraiseQL:**

1. **Local**: Entity owned by current subgraph (direct query, <5ms)
2. **Direct DB**: Entity in another FraiseQL subgraph (direct database connection, <10ms)
3. **HTTP**: Entity in non-FraiseQL subgraph (standard federation HTTP, 50-200ms)

**Related specs:**

- `docs/architecture/integration/federation.md` — Complete federation specification
- `docs/prd/PRD.md` Section 6.1 — Federation requirements

**See also:** Subgraph, Entity Resolution, @key, @external, @requires, @provides

---

### Subgraph

**A self-contained GraphQL backend that participates in federation.**

Each FraiseQL instance is a subgraph. Subgraphs:

- Expose `_service` endpoint (returns SDL with federation directives)
- Expose `_entities` endpoint (resolves entities by key)
- Have `@key` decorated types that can be extended by other subgraphs
- Can extend types from other subgraphs with `@external`

**Not a subgraph gateway:** FraiseQL is the subgraph, not the gateway. Apollo Router acts as the gateway.

**Related specs:**

- `docs/architecture/integration/federation.md` Section 1-2 — Subgraph architecture
- Apollo Federation v2 specification

**See also:** Federation v2, Entity Resolution

---

### Entity Resolution

**The process of fetching entity instances by key for federation composition.**

When Apollo Router needs to resolve an entity (e.g., User with id "123"), it sends `_entities` query to the appropriate subgraph. FraiseQL supports three resolution strategies:

1. **Local**: Query local database view `v_{entity}`, <5ms
2. **Direct DB**: Query remote FraiseQL database via direct connection, <10ms
3. **HTTP**: Call external subgraph's `_entities` endpoint, 50-200ms

**Batching:** Multiple entities resolved in single batch request, not individual queries.

**Error handling:** Null entities allowed in response if resolution fails.

**Related specs:**

- `docs/architecture/integration/federation.md` Section 9 — Runtime entity resolution
- `docs/architecture/integration/federation.md` Section 10-11 — Strategy selection

**See also:** Subgraph, @key, Direct DB Federation

---

### Direct DB Federation

**FraiseQL's optimization: Direct database connections between FraiseQL subgraphs instead of HTTP.**

**Key insight:** Each FraiseQL subgraph is independently compiled for its database. Rust runtime maintains connections to all accessible FraiseQL databases and queries them directly:

```text
Users Subgraph (PostgreSQL)
├─ Query v_user (local)
├─ Query v_order (via SQL Server connection)
└─ Query v_product (via MySQL connection)
```text

**Performance:**

- Same database: <5ms
- Different databases: <10-20ms
- HTTP (fallback): 50-200ms

**Requirements:**

- Both subgraphs must be FraiseQL
- Network access from Rust runtime to remote database
- Database credentials securely configured

**Graceful fallback:** If database connection unavailable, automatically falls back to HTTP.

**Related specs:**

- `docs/architecture/integration/federation.md` Section 10 — Multi-database federation architecture
- `docs/architecture/integration/federation.md` Section 11 — Deployment & configuration

**See also:** Federation v2, Entity Resolution, Subgraph

---

### @key

**GraphQL federation directive declaring which fields uniquely identify an entity across subgraphs.**

**Syntax:**

```graphql
type User @key(fields: "id") {
  id: ID!
  name: String!
}

# Multiple keys allowed
type Product @key(fields: "upc") @key(fields: "sku") {
  upc: String!
  sku: String!
  name: String!
}
```text

**Compile-time validation:**

- Key fields must exist in type
- Key fields must be selectable (in database view)
- Key must be unique identifier for entity

**Runtime behavior:**

- `_entities` query uses @key fields to identify entities
- Multiple keys enable multiple federation patterns

**Related specs:**

- `docs/architecture/integration/federation.md` Section 3 — Federation contract
- `docs/architecture/integration/federation.md` Section 5 — Schema authoring with @key
- Apollo Federation v2 specification

**See also:** @external, @requires, @provides, Entity Resolution

---

### @external

**GraphQL federation directive marking fields provided by another subgraph.**

**Syntax:**

```graphql
# In extended type
type User @key(fields: "id") {
  id: ID! @external
  name: String! @external
  # Fields we add:
  orders: [Order!]!
}
```text

**Indicates:**

- These fields come from owning subgraph
- This subgraph should not query them from database
- Router will fetch them from entity resolution

**Compile-time validation:**

- External fields must match owning subgraph's schema
- Cannot mark owned fields as external

**Related specs:**

- `docs/architecture/integration/federation.md` Section 3 — Federation contract
- `docs/architecture/integration/federation.md` Section 5 — Schema authoring
- Apollo Federation v2 specification

**See also:** @key, @requires, @provides, Entity Resolution

---

### @requires

**GraphQL federation directive declaring that a field needs data from another subgraph.**

**Syntax:**

```graphql
type Order @key(fields: "id") {
  id: ID!
  user: User @requires(fields: "email")  # Needs email from User subgraph
}
```text

**Execution:**

1. Fetch Order entity (local)
2. Extract `email` field from Order
3. Call User subgraph's `_entities` with email
4. Merge returned User into response

**Supports:**

- Direct DB federation: Database join via foreign table
- HTTP federation: HTTP call to external subgraph

**Related specs:**

- `docs/architecture/integration/federation.md` Section 8 — @requires support
- Apollo Federation v2 specification

**See also:** @provides, Entity Resolution, Direct DB Federation

---

### @provides

**GraphQL federation directive declaring that a field already includes data from another subgraph (optimization).**

**Syntax:**

```graphql
type Product {
  id: ID!
  name: String!
  vendor: Vendor @provides(fields: "id name")  # View already has vendor data
}
```text

**Optimization:** Router can satisfy vendor requests from this field without calling Vendor subgraph.

**Database level:** View already includes vendor data as JSONB:

```sql
CREATE VIEW v_product AS
SELECT
  p.id,
  p.name,
  jsonb_build_object('id', v.id, 'name', v.name) AS vendor_data
FROM tb_product p
JOIN tb_vendor v ON p.fk_vendor = v.pk_vendor;
```text

**Related specs:**

- `docs/architecture/integration/federation.md` Section 8 — @provides support
- Apollo Federation v2 specification

**See also:** @requires, @key, View-Based Composition

---

*End of Glossary*
