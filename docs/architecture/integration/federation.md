# Federation: Hybrid HTTP + Database-Level Linking

**Version:** 2.0
**Date:** February 5, 2026
**Status:** ✅ Implemented in v2.0.0-alpha.1
**Audience:** Architects, Integration Engineers, Runtime Developers

## Table of Contents

1. [Introduction & Philosophy](#1-introduction--philosophy)
2. [View-Based Federation Contract](#2-view-based-federation-contract)
3. [Federation Architecture](#3-federation-architecture)
4. [Compile-Time Federation Pipeline](#4-compile-time-federation-pipeline)
5. [Schema Authoring](#5-schema-authoring)
6. [Database Setup & Connection Configuration](#6-database-setup--connection-configuration)
7. [HTTP Federation Implementation](#7-http-federation-implementation)
8. [@requires and @provides Support](#8-requires-and-provides-support)
9. [Runtime Entity Resolution Architecture](#9-runtime-entity-resolution-architecture)
10. [Multi-Database Federation Architecture](#10-multi-database-federation-architecture)
11. [Deployment & Configuration](#11-deployment--configuration)
12. [Federation Examples](#12-federation-examples)
13. [Performance Characteristics & Limitations](#13-performance-characteristics--limitations)

---

## 1. Introduction & Philosophy

### What is Federation?

FraiseQL implements **Apollo Federation v2** as a **subgraph** (not a gateway), enabling multiple backends to be composed into a single federated graph by an Apollo Router or compatible gateway.

### Design Principle: View-Based Federation as an Open Protocol

FraiseQL's federation architecture is built on a **standardized protocol** that any database-backed system can implement:

1. **View-based transport** — Database-backed subgraphs expose standardized `v_{entity}` views for high-performance entity resolution
2. **HTTP as universal fallback** — Works with any GraphQL server (Apollo Server, Yoga, Mercurius, non-database systems)
3. **Automatic optimization** — FraiseQL compiler automatically generates views implementing the protocol
4. **Ecosystem compatibility** — Non-FraiseQL systems can manually implement views to gain performance benefits

### The Three Resolution Strategies

| Strategy | Databases | Latency | Use Case |
|----------|-----------|---------|----------|
| **Local** | Any | <5ms | Entity owned by this subgraph |
| **HTTP** | Any | 50-200ms | External subgraph or cross-database |
| **Database Linking** | PostgreSQL, SQL Server, MySQL | <10ms | Same-database FraiseQL-to-FraiseQL (optional optimization) |

### Why Multiple Transports?

**HTTP alone** is universal but has network overhead for tightly-coupled services.

**View-based transport alone** is fast but requires database access and shared contracts.

**Both together** provides optimal flexibility:

- ✅ Works everywhere (HTTP for any GraphQL service)
- ✅ Optimizes where possible (view-based for database-backed systems)
- ✅ Extensible (future: gRPC, message queues for other patterns)
- ✅ Protocol-driven (view contract is open, not vendor-specific)

### Support Matrix

| Database | Local | Direct DB | HTTP Fallback |
|----------|-------|-----------|---------------|
| **PostgreSQL** | ✅ | ✅ | ✅ |
| **SQL Server** | ✅ | ✅ | ✅ |
| **MySQL** | ✅ | ✅ | ✅ |
| **SQLite** | ✅ | ✅ | ✅ |
| **Apollo Server** | N/A | N/A | ✅ |
| **Any GraphQL** | N/A | N/A | ✅ |

---

## 2. View-Based Federation Contract

### The `v_{entity}` View Protocol

For a database-backed system to participate in view-based federation, it must expose views matching this contract:

**View naming:** `v_{entity_name}` (lowercase, singular)

- Example: `v_user`, `v_order`, `v_product`

**View structure:**

```sql
-- Minimum contract (required)
CREATE VIEW v_user AS
SELECT
  id,                              -- Primary key, used for federation lookups
  jsonb_build_object(...)  AS data -- Complete entity payload as JSON/JSONB
FROM ...;
```

**Columns:**

| Column | Type | Purpose | Required |
|--------|------|---------|----------|
| `id` | Native (UUID, String, Int, etc.) | Entity identifier for lookups | ✅ Yes |
| `data` | JSON/JSONB | Complete entity as JSON object | ✅ Yes |
| Additional columns | Any | Database-specific optimizations | ❌ No |

**Example implementations:**

**PostgreSQL (FraiseQL automatic):**

```sql
CREATE VIEW v_user AS
SELECT
  id,
  jsonb_build_object('id', id, 'email', email, 'name', name) AS data
FROM tb_user
WHERE deleted_at IS NULL;
```

**SQL Server (manual):**

```sql
CREATE VIEW v_user AS
SELECT
  id,
  (SELECT * FROM tb_user WHERE id = tb_user.id FOR JSON PATH, WITHOUT_ARRAY_WRAPPER) AS data
FROM tb_user
WHERE deleted_at IS NULL;
```

**Go/Python service (manual query):**

```python
# Instead of a database view, return same shape:
async def get_user_view(user_id):
    user = await db.get_user(user_id)
    return {
        "id": user.id,
        "data": json.dumps({
            "id": user.id,
            "email": user.email,
            "name": user.name
        })
    }
```

### Why This Contract?

- **`id` as native column:** Allows indexed lookups (WHERE id IN (...)) for batching
- **`data` as JSON:** Portable across federation, preserves field structure, enables @requires/@provides
- **Minimal footprint:** Simple enough for manual implementation, natural for compiled systems

### Protocol Guarantees

- **Deterministic:** Same `id` always returns same entity
- **Current:** Data reflects current state (subject to replication lag in some systems)
- **Complete:** `data` payload contains all fields needed for queries
- **Ordered:** Results can be returned in any order; caller handles re-ordering

---

## 3. Federation Architecture

### High-Level Topology

```
┌─────────────────────────────────────────────┐
│ Apollo Router (Query Planning & Composition)│
│ Sends standard GraphQL federation queries   │
└─────────────────────────────────────────────┘
           ↓ HTTP (standard federation protocol)
    ┌──────┬──────┬──────┬────────┐
    ↓      ↓      ↓      ↓        ↓
 ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐
 │FraiseQL│FraiseQL │FraiseQL│Apollo │
 │Postgres│SQL Srv  │MySQL   │Server │
 │(Users) │(Orders) │(Prod)  │(Reviews)
 │─────── │─────── │─────── │────────
 │Rust RT │Rust RT │Rust RT │GraphQL
 └──────┘ └──────┘ └──────┘ └──────┘
   ↓         ↓         ↓      ↓
   Direct DB Direct DB Direct DB HTTP
   Connection Connection Connection Post
```

### Entity Resolution: Three Transport Strategies

#### Strategy 1: Local Resolution

Entity owned and resolved by current subgraph:

```
Apollo Router requests User(id: "123")
    ↓
FraiseQL receives _entities query
    ↓
Execute local database query
    ↓
Return User entity via HTTP response
    ↓ <5ms total
```

**Latency:** <5ms (direct database query, no network overhead)

#### Strategy 2: HTTP Federation (Universal)

Entity in any external subgraph (GraphQL-based):

```
Apollo Router needs Product data
    ↓
FraiseQL receives _entities query with Product representations
    ↓
FraiseQL makes HTTP request to Products subgraph
    ↓
Products subgraph queries its database
    ↓
Response returned via HTTP
    ↓ 50-200ms total
```

**Latency:** 50-200ms (network round-trip + remote query)

**Used when:**

- Subgraph uses different database than source (PostgreSQL → SQL Server)
- Subgraph is external (Apollo Server, Yoga, etc.)
- SQLite is involved (no database linking available)

#### Strategy 3: View-Based Federation (Database-Backed Systems)

Entity in database-backed subgraph implementing view contract:

```
Apollo Router needs Order data
    ↓
Subgraph receives _entities query
    ↓
Subgraph executes query against v_order view
    ↓
View returns entity with JSONB data payload
    ↓ <10-20ms total
```

**Latency:** <10-20ms (single database query, no HTTP round-trip)

**Used when:**

- Target subgraph is database-backed (FraiseQL, Go service, data platform, etc.)
- Target subgraph exposes `v_{entity}` view matching federation contract
- Network path exists from caller's Rust runtime to target database
- Target database credentials are securely configured

**Implementations:**

- **FraiseQL:** Automatic (compiler generates views)
- **Other systems:** Manual SQL view implementation
- **Any database type:** PostgreSQL, SQL Server, MySQL, SQLite, Snowflake, DuckDB, etc.

### Compile-Time Strategy Selection

The compiler automatically selects the optimal strategy:

```
For each federation link (e.g., User → Order):
  1. Is target subgraph FraiseQL?
     NO → Use HTTP
  2. Can Rust runtime connect to target database?
     NO → Use HTTP (fallback)
  3. YES → Use Direct DB Connection
```

**Result: Zero configuration needed** — Compiler detects subgraph types and configures connections.

### Performance Characteristics

#### Same Database (PostgreSQL ↔ PostgreSQL)

```
Apollo Router: _entities query (User by id)
    ↓ HTTP (10ms network)
Rust runtime: Direct DB connection, SELECT FROM v_user (1ms)
    ↓ HTTP response (10ms network)
Total: ~21ms (but bulk batching reduces per-entity cost to ~0.2ms each)
```

**For 100 entities:** ~3ms (batched in single query)

#### Cross Database (PostgreSQL ↔ SQL Server)

```
Apollo Router: _entities query
    ↓ HTTP (50-200ms total)
FraiseQL: HTTP request to SQL Server subgraph
    ↓ HTTP request (50-200ms)
Total: 50-200ms per batch
```

**For 100 entities:** 50-200ms (single HTTP batch request)

#### External (FraiseQL ↔ Apollo Server)

```
Apollo Router: _entities query
    ↓ HTTP (50-200ms total)
FraiseQL: HTTP request to Apollo Server
    ↓ HTTP request (50-200ms)
Total: 50-200ms per batch
```

**For 100 entities:** 50-200ms (same as cross-database)

---

## 3. Federation Contract Implementation

### The Apollo Federation Contract

FraiseQL implements the standard Apollo Federation v2 contract. Every FraiseQL subgraph exposes a GraphQL endpoint with three special queries:

#### 3.1 `_service { sdl }`

Returns the subgraph's schema as GraphQL SDL (Schema Definition Language).

**Query:**

```graphql
{
  _service {
    sdl
  }
}
```

**Response:**

```json
{
  "data": {
    "_service": {
      "sdl": "directive @key(fields: String!) on OBJECT | INTERFACE\n\ntype User @key(fields: \"id\") {\n  id: ID!\n  email: String!\n  name: String!\n  orders: [Order]\n}\n\ntype Query {\n  users: [User]\n}"
    }
  }
}
```

**Implementation in FraiseQL:**

- SDL is embedded in `CompiledSchema` during compilation
- Generated with all `@key`, `@external`, `@requires`, `@provides` directives
- Includes all local types, queries, and extended types

#### 3.2 `_entities(representations: [_Any!]!)`

Resolves entities by their representations (key values).

**Query:**

```graphql
query {
  _entities(representations: [
    { __typename: "User", id: "123" }
    { __typename: "User", id: "456" }
  ]) {
    ... on User {
      id
      email
      name
    }
  }
}
```

**Response:**

```json
{
  "data": {
    "_entities": [
      { "__typename": "User", "id": "123", "email": "alice@example.com", "name": "Alice" },
      { "__typename": "User", "id": "456", "email": "bob@example.com", "name": "Bob" }
    ]
  }
}
```

**Implementation in FraiseQL:**

1. Accept `representations` array from Apollo Router
2. Group by `__typename` and key fields
3. For each group, determine strategy (Local, HTTP, or DatabaseLink)
4. Execute query using selected strategy
5. Return results in original order (preserving representation order)

#### 3.3 `_Entity` Union

Auto-generated union of all entity types (types with `@key` directive).

**In SDL:**

```graphql
union _Entity = User | Order | Product

directive @federation__service(sdl: String!) on SCHEMA
```

### The `_Any` Scalar

Represents entity representations (opaque JSON objects with `__typename`).

**Structure:**

```json
{
  "__typename": "User",
  "id": "123",
  "email": "alice@example.com",
  ... other key fields ...
}
```

**Parsing in FraiseQL:**

1. Parse JSON representation
2. Extract `__typename`
3. Extract key fields (defined in `@key` directive)
4. Validate against schema
5. Prepare for database query

### Reference Implementation

**SDL Generation** (compile-time):

```
CompiledSchema
  ↓
Extract all types with @key directive
  ↓
Generate SDL with federation directives
  ↓
Embed in CompiledSchema.federation.sdl
```

**Entity Resolution** (runtime):

```
HTTP Request: _entities(representations: [_Any!]!)
  ↓
Parse representations
  ↓
Determine strategy per entity type
  ↓
Execute queries (Local, HTTP, or DatabaseLink)
  ↓
Return results in original order
  ↓
HTTP Response: _entities
```

### Federation Compliance

FraiseQL federation is fully compliant with:

- **Apollo Federation Specification v2.0+**
- **Apollo Router compatibility**
- **Standard GraphQL federation queries**
- **Federation SDL directives**: @key, @external, @requires, @provides

Subgraphs can be composed with:

- Other FraiseQL subgraphs (any database)
- Apollo Server subgraphs
- Yoga Server subgraphs
- Any Apollo-compatible subgraph implementation

---

## 4. Compile-Time Federation Pipeline

### Overview: Four-Phase Compilation Process

Federation adds three new phases to the FraiseQL compilation pipeline:

1. **Phase 4b: Federation Analysis** — Parse federation directives, identify entities, validate integrity
2. **Phase 5b: Entity Resolution SQL Generation** — Generate database functions for entity resolution
3. **Phase 6: Federation Metadata Generation** — Update CompiledSchema with federation information

### Phase 4b: Federation Analysis

**Input:** AuthoringSchema with federation decorators
**Output:** FederationMetadata for CompiledSchema
**Validations:** Compile-time checks for federation integrity

#### 4b.1 Parse Federation Directives

Extract federation directives from schema:

```python
@fraiseql.type
@fraiseql.key(fields=["id"])
@fraiseql.key(fields=["email"])  # Multiple keys allowed
class User:
    id: ID
    email: str
    name: str

@fraiseql.type(extend=True)
@fraiseql.key(fields=["id"])
class User:
    id: ID = fraiseql.external()
    orders: list[Order] = fraiseql.requires(fields=["email"])
```

**Compiler extracts:**

- Entity types: `User`, `Order` (types with @key)
- Key definitions: `id`, `email`
- External fields: `User.id` (in extended types)
- Requires dependencies: `User.orders` requires `User.email`
- Provides optimizations: Fields on external types

#### 4b.2 Identify Entity Types

Collect all types with `@key` directive:

| Type | Keys | Status |
|------|------|--------|
| `User` | `id`, `email` | **Entity** (local) |
| `Order` | `id` | **Entity** (local) |
| `Product` | `upc` | **Entity** (extended, external) |

#### 4b.3 Validation Rules

**✅ Compile-Time Validations:**

1. **Key fields must exist in type:**

   ```
   @fraiseql.type
   @fraiseql.key(fields=["user_id"])  # ❌ ERROR: user_id not a field
   class User:
       id: ID
   ```

2. **Key fields must be selectable:**

   ```
   @fraiseql.type
   @fraiseql.key(fields=["id"])  # ✅ OK
   class User:
       id: ID
       _internal: str = fraiseql.internal()  # ✅ OK, internal fields excluded
   ```

3. **@external only on extended types:**

   ```
   @fraiseql.type(extend=True)
   class User:
       id: ID = fraiseql.external()  # ✅ OK, extended type

   @fraiseql.type
   class User:
       id: ID = fraiseql.external()  # ❌ ERROR: not extended
   ```

4. **@requires must reference valid fields:**

   ```
   orders: list[Order] = fraiseql.requires(fields=["email"])  # ✅ OK, email exists
   orders: list[Order] = fraiseql.requires(fields=["nonexistent"])  # ❌ ERROR
   ```

5. **No duplicate key definitions:**

   ```
   @fraiseql.key(fields=["id"])
   @fraiseql.key(fields=["id"])  # ❌ ERROR: duplicate
   class User: ...
   ```

6. **Database views must expose key columns:**

   ```
   -- ✅ GOOD: key columns are native
   CREATE VIEW v_user AS
   SELECT pk_user, id, email, data FROM tb_user;

   -- ❌ BAD: key only in JSONB
   CREATE VIEW v_user AS
   SELECT pk_user, jsonb_build_object('id', id) AS data FROM tb_user;
   ```

### Phase 5b: Entity Resolution SQL Generation

**Input:** FederationMetadata from Phase 4b
**Output:** SQL DDL for entity resolution functions
**Database Support:** PostgreSQL, SQL Server, MySQL (SQLite falls back to HTTP)

#### 5b.1 Generate Entity Resolution Database Functions

**Key Point**: Database functions are **trivial view queries**. All complexity (cross-subgraph communication, strategy selection, response shaping) is handled by the Rust runtime.

For each entity type with `@key`, generate one simple database function per key variant:

**PostgreSQL Example:**

```sql
-- Single key resolution - trivial view query
CREATE FUNCTION resolve_user_by_id(keys UUID[]) RETURNS JSONB[] AS $$
  SELECT array_agg(data ORDER BY idx)
  FROM unnest(keys) WITH ORDINALITY AS t(key, idx)
  JOIN v_user ON v_user.id = t.key
$$ LANGUAGE sql STABLE PARALLEL SAFE;

-- Alternative key resolution - trivial view query
CREATE FUNCTION resolve_user_by_email(keys TEXT[]) RETURNS JSONB[] AS $$
  SELECT array_agg(data ORDER BY idx)
  FROM unnest(keys) WITH ORDINALITY AS t(key, idx)
  JOIN v_user ON v_user.email = t.key
$$ LANGUAGE sql STABLE PARALLEL SAFE;
```

**SQL Server Example:**

```sql
-- Entity resolution - trivial view query
CREATE FUNCTION resolve_user_by_id (@keys NVARCHAR(MAX))
RETURNS TABLE
AS
RETURN
  SELECT [data]
  FROM [dbo].[v_user]
  WHERE [id] IN (SELECT JSON_VALUE(value, '$') FROM OPENJSON(@keys))
GO
```

**MySQL Example:**

```sql
-- Entity resolution - trivial view query
DELIMITER //
CREATE FUNCTION resolve_user_by_id(keys JSON)
RETURNS JSON
DETERMINISTIC
READS SQL DATA
BEGIN
  SELECT JSON_ARRAYAGG(
    JSON_OBJECT(
      'id', HEX(id),
      'email', email,
      'name', name,
      'data', `data`
    )
  )
  FROM v_user
  WHERE id IN (
    SELECT UNHEX(JSON_UNQUOTE(value))
    FROM JSON_TABLE(keys, '$[*]' COLUMNS (value VARCHAR(36) PATH '$')) AS jt
  );
END//
DELIMITER ;
```

**Why so simple?**

- ✅ View already contains all JSONB data
- ✅ Just batch-fetch by key
- ✅ Rust runtime handles:
  - Local vs HTTP vs Direct DB strategy selection
  - Cross-subgraph HTTP requests
  - Remote database queries via native drivers
  - Response formatting and shaping
  - Error handling and retries

#### 5b.2 Database Connection Configuration

The compiler generates configuration for Rust runtime to connect to remote databases:

**For each FraiseQL subgraph detected:**

- Database type (PostgreSQL, SQL Server, MySQL, SQLite)
- Connection string (hostname, port, database name)
- Schema name
- View name (v_{entity})
- Entity type name

**Example detected subgraph:**

```
Typename: Order
DatabaseType: sqlserver
DatabaseURL: sqlserver://orders-db.internal/orders_db
SchemaName: dbo
ViewName: v_order
```

No database-specific SQL generation needed. Rust drivers handle connections transparently.

### Federation Metadata Generation

**Input:** Entity resolution strategy decisions and subgraph configuration
**Output:** Updated CompiledSchema with federation metadata

#### 6.1 CompiledSchema Federation Section

```json
{
  "federation": {
    "enabled": true,
    "sdl": "directive @key(fields: String!) ...",
    "entities": {
      "User": {
        "keys": [
          {
            "fields": ["id"],
            "db_function": "resolve_user_by_id",
            "arg_types": ["uuid[]"],
            "strategy": "local"
          },
          {
            "fields": ["email"],
            "db_function": "resolve_user_by_email",
            "arg_types": ["text[]"],
            "strategy": "local"
          }
        ],
        "external_fields": []
      },
      "Order": {
        "keys": [
          {
            "fields": ["id"],
            "db_function": "resolve_order_by_id",
            "arg_types": ["uuid[]"],
            "strategy": "database_link",
            "database_type": "postgresql",
            "link_name": "orders_fdw"
          }
        ],
        "external_fields": ["user"]
      }
    },
    "links": {
      "User.orders": {
        "target_type": "Order",
        "strategy": "database_link",
        "database_type": "postgresql",
        "requires_fields": ["email"],
        "provides_fields": []
      },
      "Product.vendor": {
        "target_type": "Vendor",
        "strategy": "http",
        "subgraph_url": "https://vendors-api.internal/graphql",
        "requires_fields": [],
        "provides_fields": ["name"]
      }
    }
  }
}
```

---

## 5. Schema Authoring

### Federation Decorators

#### 5.1 @fraiseql.key()

Defines the primary key for federation entity resolution.

**Syntax:**

```python
@fraiseql.key(fields=["id"])
@fraiseql.key(fields=["email"])  # Multiple keys allowed
class User:
    id: ID
    email: str
    name: str
```

**Multiple Keys Example:**

```python
@fraiseql.type
@fraiseql.key(fields=["upc"])
@fraiseql.key(fields=["sku"])
class Product:
    upc: String  # Unique Product Code
    sku: String  # Stock Keeping Unit
    name: String
```

**Compiler Behavior:**

1. Generates entity resolution function for each key
2. Adds `@key` directive to SDL
3. Validates key fields exist in type

#### 5.2 @fraiseql.external()

Marks field as external (defined in other subgraph).

**Syntax:**

```python
@fraiseql.type(extend=True)
@fraiseql.key(fields=["id"])
class User:
    id: ID = fraiseql.external()
    email: str = fraiseql.external()
    orders: list[Order]  # Local field
```

**Compiler Behavior:**

1. Adds `@external` directive to SDL
2. Excludes external fields from local resolution
3. Enables field composition from source subgraph

#### 5.3 @fraiseql.requires()

Defines dependencies for federation field resolution.

**Syntax:**

```python
orders: list[Order] = fraiseql.requires(fields=["email"])
```

**Meaning:** "To resolve orders, I need the user's email field from the source subgraph."

**Compiler Behavior:**

1. Adds `@requires` directive to SDL
2. Includes required fields in entity selection
3. Generates join logic for database linking

**Example - Email Lookup:**

```python
@fraiseql.type(extend=True)
@fraiseql.key(fields=["id"])
class User:
    id: ID = fraiseql.external()
    email: str = fraiseql.external()
    orders: list[Order] = fraiseql.requires(fields=["email"])
    # Orders are joined by email in Orders subgraph
```

#### 5.4 @fraiseql.provides()

Documents optimization provided to other subgraphs.

**Syntax:**

```python
@fraiseql.type(extend=True)
class Product:
    upc: String = fraiseql.external()
    vendor: Vendor = fraiseql.provides(fields=["vendor.id"])
    # We provide vendor.id without external query
```

**Compiler Behavior:**

1. Adds `@provides` directive to SDL
2. Includes provided fields in view projection
3. Optimizes field resolution (no additional queries needed)

### TypeScript/YAML Equivalents

**TypeScript (future):**

```typescript
@Key({ fields: ["id"] })
@Key({ fields: ["email"] })
export class User {
  @Federation.Key
  id: ID;

  @Federation.Key
  email: string;

  @Federation.External
  email: string;

  @Federation.Requires("email")
  orders: Order[];
}
```

**YAML (future):**

```yaml
types:
  User:
    keys:
      - fields: [id]
      - fields: [email]
    fields:
      id:
        type: ID
      orders:
        type: "[Order]"
        requires: [email]
```

### Compiler Validation Rules

**During schema parsing:**

1. ✅ Key fields must exist in type
2. ✅ External fields only on extended types
3. ✅ Requires/Provides fields must exist
4. ✅ No circular extends (A extends B, B extends A)

**During database analysis:**

1. ✅ Key columns exist in database views
2. ✅ Key columns are native SQL types (not just JSONB)
3. ✅ Foreign tables accessible for database linking
4. ✅ Linked servers configured correctly

---

## 6. Database Setup & Connection Configuration

### Multi-Database Federation: Resolution Paths

Federation works with any database combination using the same principles:

| Source | Target | Strategy | Latency |
|--------|--------|----------|---------|
| PostgreSQL | PostgreSQL | Direct DB | <10ms |
| PostgreSQL | SQL Server | Direct DB | <10-20ms |
| PostgreSQL | Apollo Server | HTTP | 50-200ms |
| SQL Server | SQL Server | Direct DB | <10ms |
| SQL Server | MySQL | Direct DB | <10-20ms |
| MySQL | PostgreSQL | Direct DB | <10-20ms |
| SQLite | Any | HTTP | 50-200ms |

**Key principle:** Same underlying mechanism (Rust runtime maintains connections) applies to all database types.

#### PostgreSQL to PostgreSQL: Direct Connection

**Architecture:**

```
PostgreSQL Cluster
├── users_schema (Subgraph A)
│   ├── tb_user, v_user
│   └── Rust runtime connects here
└── orders_schema (Subgraph B)
    ├── tb_order, v_order
    └── Rust runtime connects here
```

**Database Setup (Minimal):**

No special database configuration needed. Each subgraph has standard views:

```sql
-- Users subgraph schema
CREATE SCHEMA users_schema;
CREATE TABLE users_schema.tb_user (
    pk_user BIGINT PRIMARY KEY,
    id UUID NOT NULL UNIQUE,
    email TEXT NOT NULL,
    name TEXT NOT NULL
);
CREATE VIEW users_schema.v_user AS
SELECT id, jsonb_build_object('id', id, 'email', email, 'name', name) AS data
FROM users_schema.tb_user;

-- Orders subgraph schema (same PostgreSQL instance)
CREATE SCHEMA orders_schema;
CREATE TABLE orders_schema.tb_order (
    pk_order BIGINT PRIMARY KEY,
    id UUID NOT NULL UNIQUE,
    user_id UUID NOT NULL,
    total NUMERIC
);
CREATE VIEW orders_schema.v_order AS
SELECT id, user_id, jsonb_build_object('id', id, 'user_id', user_id, 'total', total) AS data
FROM orders_schema.tb_order;
```

**Runtime Configuration:**

Rust runtime maintains single connection pool to PostgreSQL:

```toml
# fraiseql.toml (Users subgraph)
[database]
type = "postgresql"
url = "postgresql://user:pass@pg.internal/shared_db"
schema = "users_schema"

[[federation.subgraphs]]
typename = "Order"
is_fraiseql = true
database_type = "postgresql"
database_url = "postgresql://user:pass@pg.internal/shared_db"
schema_name = "orders_schema"
view_name = "v_order"
```

**Entity Resolution (Rust):**

```rust
// Both queries use single connection pool
let user = local_pool.query(
    "SELECT data FROM users_schema.v_user WHERE id = $1",
    &[&id]
).await?;

let orders = local_pool.query(
    "SELECT data FROM orders_schema.v_order WHERE user_id = $1",
    &[&user_id]
).await?;
```

#### SQL Server to SQL Server: Direct Connection

**Architecture:**

```
SQL Server Instance
├── users_db (Subgraph A)
│   ├── [dbo].[tb_user]
│   └── [dbo].[v_user]
└── orders_db (Subgraph B)
    ├── [dbo].[tb_order]
    └── [dbo].[v_order]
```

**Database Setup:**

No special server configuration. Each subgraph has standard views:

```sql
-- Users subgraph
CREATE VIEW [dbo].[v_user] AS
SELECT
  [id],
  (SELECT * FROM [dbo].[tb_user] WHERE [id] = [tb_user].[id] FOR JSON PATH, WITHOUT_ARRAY_WRAPPER) AS [data]
FROM [dbo].[tb_user];

-- Orders subgraph (different database)
CREATE VIEW [dbo].[v_order] AS
SELECT
  [id],
  [user_id],
  (SELECT * FROM [dbo].[tb_order] WHERE [id] = [tb_order].[id] FOR JSON PATH, WITHOUT_ARRAY_WRAPPER) AS [data]
FROM [dbo].[tb_order];
```

**Runtime Configuration:**

Rust runtime maintains connection pool to SQL Server:

```toml
# fraiseql.toml (Users subgraph)
[database]
type = "sqlserver"
url = "sqlserver://user:pass@mssql.internal/users_db"
schema = "dbo"

[[federation.subgraphs]]
typename = "Order"
is_fraiseql = true
database_type = "sqlserver"
database_url = "sqlserver://user:pass@mssql.internal/orders_db"
schema_name = "dbo"
view_name = "v_order"
```

**Entity Resolution (Rust):**

```rust
// Queries against both databases use same SQL Server driver
let user = local_pool.query(
    "SELECT [data] FROM [dbo].[v_user] WHERE [id] = @id",
    &[&id]
).await?;

let orders = remote_pool.query(
    "SELECT [data] FROM [dbo].[v_order] WHERE [user_id] = @user_id",
    &[&user_id]
).await?;
```

---

#### MySQL to MySQL: Direct Connection

**Architecture:**

```
MySQL Instance
├── users_db (Subgraph A)
│   ├── tb_user
│   └── v_user
└── orders_db (Subgraph B)
    ├── tb_order
    └── v_order
```

**Database Setup:**

Standard views on each database:

```sql
-- Users subgraph
CREATE VIEW v_user AS
SELECT
  id,
  JSON_OBJECT('id', id, 'email', email, 'name', name) AS data
FROM tb_user;

-- Orders subgraph (different database)
CREATE VIEW v_order AS
SELECT
  id,
  user_id,
  JSON_OBJECT('id', id, 'user_id', user_id, 'total', total) AS data
FROM tb_order;
```

**Runtime Configuration:**

```toml
# fraiseql.toml (Users subgraph)
[database]
type = "mysql"
url = "mysql://user:pass@mysql.internal/users_db"
schema = "public"

[[federation.subgraphs]]
typename = "Order"
is_fraiseql = true
database_type = "mysql"
database_url = "mysql://user:pass@mysql.internal/orders_db"
schema_name = "public"
view_name = "v_order"
```

**Entity Resolution (Rust):**

```rust
// Both queries use MySQL driver
let user = local_pool.query(
    "SELECT data FROM v_user WHERE id = ?",
    &[&id]
).await?;

let orders = remote_pool.query(
    "SELECT data FROM v_order WHERE user_id = ?",
    &[&user_id]
).await?;
```

---

### Cross-Database Federation: HTTP Fallback

When source and target databases are different types:

```
PostgreSQL Users → SQL Server Inventory: HTTP
PostgreSQL Users → Apollo Server Products: HTTP
SQL Server Orders → MySQL Logs: HTTP
SQLite Cache → Any: HTTP
```

**No special configuration needed** — Compiler automatically routes to HTTP.

**Rationale:**

- Database linking is not portable across database types
- HTTP is universal and works for all combinations
- Performance trade-off is acceptable for cross-database scenarios
- Complexity of cross-database joins not worth the benefit

---

## 7. HTTP Federation Implementation

### Standard Federation Endpoints

FraiseQL exposes two standard Apollo Federation v2 endpoints:

#### 1. Service Discovery: `GET /_service`

Returns the subgraph's GraphQL SDL with federation directives:

```graphql
type Query {
  user(id: ID!): User
}

type User @key(fields: "id") {
  id: ID!
  email: String!
  name: String!
  orders: [Order!]! @requires(fields: "email")
}

type Order @external {
  id: ID! @external
  user_email: String! @external
}
```

**Implementation:**

```rust
// Rust runtime: /src/runtime/federation.rs
pub async fn handle_service_request(schema: &CompiledSchema) -> ServiceResponse {
    ServiceResponse {
        sdl: schema.federation.sdl.clone(),  // Embedded during compilation
    }
}
```

**Compile-Time Generation:**
The compiler generates SDL from the schema and embeds it in CompiledSchema:

```python
# Compiler phase 6: Update CompiledSchema
compiled_schema.federation = {
    "sdl": generate_federation_sdl(authoring_schema),  # Includes @key, @external, @requires, @provides
    "entities": {
        "User": {...},
        "Order": {...}
    }
}
```

#### 2. Entity Resolution: `POST /_entities`

Resolves entities by key for composition:

**Request format:**

```json
POST /graphql
Content-Type: application/json

{
  "query": "query($_representations: [_Any!]!) { _entities(representations: $_representations) { ...on User { id email name } } }",
  "variables": {
    "_representations": [
      { "__typename": "User", "id": "123", "email": "alice@example.com" },
      { "__typename": "User", "id": "456", "email": "bob@example.com" }
    ]
  }
}
```

**Response format:**

```json
{
  "data": {
    "_entities": [
      { "id": "123", "email": "alice@example.com", "name": "Alice" },
      { "id": "456", "email": "bob@example.com", "name": "Bob" }
    ]
  }
}
```

**Rust Implementation:**

```rust
// Rust runtime handles all complexity
pub async fn resolve_entities(
    representations: Vec<_Any>,  // JSON-like representations
    schema: &CompiledSchema,
    db: &DatabasePool,
    http_client: &reqwest::Client,
    federation_config: &FederationConfig
) -> Result<Vec<Option<Entity>>> {
    // 1. Group representations by typename
    let groups = group_representations(representations, schema)?;

    // 2. For each typename, dispatch to appropriate resolution strategy
    let mut results = Vec::new();
    for (typename, reps) in groups {
        let entity_meta = schema.federation.entities.get(typename)?;

        match entity_meta.resolution_strategy {
            // Strategy 1: Local resolution (entity owned by this subgraph)
            ResolutionStrategy::Local => {
                let entities = resolve_local(typename, reps, db).await?;
                results.extend(entities);
            }

            // Strategy 2: HTTP resolution (external subgraph)
            ResolutionStrategy::HTTP { subgraph_url } => {
                let entities = resolve_via_http(
                    &subgraph_url,
                    typename,
                    reps,
                    http_client
                ).await?;
                results.extend(entities);
            }

            // Strategy 3: Database linking (same-database FraiseQL)
            ResolutionStrategy::DatabaseLink { db_function } => {
                let entities = resolve_via_database(
                    &db_function,
                    reps,
                    db
                ).await?;
                results.extend(entities);
            }
        }
    }

    // 3. Return in original representation order
    reorder_results(results, &representations)
}
```

### Local Entity Resolution

For entities owned by the current subgraph, query the local database:

```rust
async fn resolve_local(
    typename: &str,
    representations: Vec<_Any>,
    db: &DatabasePool
) -> Result<Vec<Entity>> {
    // Extract key values from representations
    let entity_meta = schema.federation.entities.get(typename)?;
    let keys = representations.extract_keys(&entity_meta.key_fields)?;

    // Build GraphQL query to fetch entities
    let graphql_query = format!(
        "query($ids: [ID!]!) {{ {}(where: {{ id: {{ _in: $ids }} }}) {{ {} }} }}",
        entity_meta.query_name,
        entity_meta.field_list
    );

    // Execute compiled GraphQL query
    let result = db.execute_compiled_query(&graphql_query, &keys).await?;

    // Parse and return entities
    Ok(parse_entity_response(result))
}
```

### HTTP Entity Resolution

For entities in external subgraphs, call their `_entities` endpoint:

```rust
async fn resolve_via_http(
    subgraph_url: &str,
    typename: &str,
    representations: Vec<_Any>,
    http_client: &reqwest::Client
) -> Result<Vec<Entity>> {
    // Build federation entity resolution query
    let entity_query = build_entity_query(typename);

    // Send HTTP request
    let response = http_client
        .post(format!("{}/graphql", subgraph_url))
        .json(&json!({
            "query": entity_query,
            "variables": {
                "_representations": representations
            }
        }))
        .send()
        .await?;

    // Parse response
    let body: GraphQLResponse = response.json().await?;
    Ok(parse_entities_response(body))
}
```

**Features:**

- ✅ Batching: 100 entities in single HTTP request
- ✅ Error handling: Null entities for missing data
- ✅ Timeouts: Configurable per-subgraph
- ✅ Retry logic: Exponential backoff for transient failures
- ✅ Connection pooling: Reuse HTTP connections

### Database-Level Linking Resolution

For same-database FraiseQL subgraphs, use compiled database functions:

```rust
async fn resolve_via_database(
    db_function: &str,
    representations: Vec<_Any>,
    db: &DatabasePool
) -> Result<Vec<Entity>> {
    // Extract key values from representations
    let keys = representations.extract_keys()?;

    // Call database function with batched keys
    // Example: resolve_user_by_id(ARRAY['123', '456', '789'])
    let result = db.call_function(db_function, &[keys]).await?;

    // Parse and return entities
    Ok(parse_entity_response(result))
}
```

**Key advantages:**

- ✅ No HTTP overhead (database join)
- ✅ Single-round-trip performance (<5ms for small batches)
- ✅ Transactional consistency if needed
- ✅ Automatic failover to HTTP if database link unavailable

---

## 8. @requires and @provides Support

### @requires: Fetching External Fields

`@requires` declares that a field needs data from another subgraph:

```graphql
type Order @key(fields: "id") {
  id: ID!

  # This field requires user email from Users subgraph
  user: User @requires(fields: "email")
}
```

**How @requires Works:**

1. **Compile-Time:** Compiler validates that `email` exists in Order and is accessible
2. **Runtime:** When Order is resolved with @requires field:
   - Extract the required fields (`email`) from the Order entity
   - Call the required subgraph's `_entities` endpoint with that field
   - Merge returned data into the response

**Rust Implementation:**

```rust
// When resolving Order.user @requires(fields: "email")
async fn resolve_requires_field(
    field_name: &str,              // "user"
    field_requires: &[&str],       // ["email"]
    entity: &JsonValue,            // Order entity with email
    schema: &CompiledSchema,
    http_client: &reqwest::Client
) -> Result<JsonValue> {
    let required_subgraph = schema.federation.get_field_subgraph(field_name)?;
    let user_type = schema.federation.get_type_name(field_name)?;

    // Build representation with required fields
    let representation = json!({
        "__typename": user_type,      // "User"
        ...field_requires.map(|f| (f, entity[f].clone()))
    });

    // Call User subgraph's _entities
    let user_entity = http_resolve_entities(
        &required_subgraph.url,
        vec![representation],
        http_client
    ).await?;

    Ok(user_entity[0].clone())
}
```

### @provides: Optimizing Field Resolution

`@provides` declares that this field already includes data from another subgraph:

```graphql
type Product {
  id: ID!
  name: String!

  # This field provides vendor data (no need to call Vendor subgraph)
  vendor: Vendor @provides(fields: "id name")
}
```

**How @provides Works:**

1. **Compile-Time:** Compiler validates that provided fields exist in the view
2. **Runtime:** No special handling needed
   - The view already includes the vendor data as JSONB
   - Router can satisfy vendor requests from this field without calling Vendor subgraph

**Database Level:**

The view already includes the vendor data:

```sql
-- Products view includes vendor information
CREATE VIEW v_product AS
SELECT
  p.pk_product,
  p.id,
  p.name,
  jsonb_build_object(
    'id', v.id,
    'name', v.name,
    'status', v.status
  ) AS data
FROM tb_product p
JOIN tb_vendor v ON p.fk_vendor = v.pk_vendor
WHERE p.deleted_at IS NULL;
```

**Compiler Recognition:**

```python
# Compiler detects that Product.vendor is already available
# No HTTP call needed, router can use this data directly
field_provides = {
    "vendor": ["id", "name"]  # These fields are already in the view
}
```

### Complex @requires: Chained Resolution

When @requires depends on data from yet another subgraph:

```graphql
type Order @key(fields: "id") {
  id: ID!

  # Requires email from User subgraph
  user: User! @requires(fields: "email")

  # But User.company requires data from Company subgraph
  # This is automatically handled: Order → User → Company chain
}
```

**Execution:**

1. Router calls Order subgraph with Order ID
2. Order subgraph needs User email (calls User subgraph via @requires)
3. User subgraph might need Company data (calls Company subgraph)
4. Results bubble back up the chain

**Rust handles this transparently** — @requires is recursive through the federation chain.

---

## 9. Runtime Entity Resolution Architecture

### Request Flow

```
Apollo Router (_entities query)
    ↓
FraiseQL Subgraph (HTTP POST /graphql)
    ↓ Parse federation request
FraiseQL Runtime (_entities resolver)
    ↓
1. Parse _representations (JSON)
2. Group by typename
3. Group by resolution strategy (Local/HTTP/DatabaseLink)
    ↓
For Local resolution:
  ├─ Extract keys from representations
  ├─ Build GraphQL query for local entities
  ├─ Execute via compiled query engine
  └─ Return JSONB results
    ↓
For HTTP resolution:
  ├─ Extract keys from representations
  ├─ Send HTTP request to external subgraph
  ├─ Await response with exponential backoff
  └─ Return parsed entities
    ↓
For DirectDB resolution:
  ├─ Extract keys from representations
  ├─ Query remote database via Rust driver
  ├─ (Native database connection handles remote query)
  └─ Return JSONB results
    ↓
4. Merge all strategies' results
5. Reorder to match input representation order
6. Return as GraphQL response
```

### Error Handling

Federation allows null entities when resolution fails:

```rust
async fn resolve_entities(
    representations: Vec<_Any>,
    schema: &CompiledSchema,
    db: &DatabasePool,
    http_client: &reqwest::Client
) -> Result<Vec<Option<Entity>>> {
    let mut results = Vec::new();

    for rep in representations {
        let entity = match resolve_single_entity(&rep, schema, db, http_client).await {
            Ok(entity) => Some(entity),
            Err(e) => {
                // Log error, but don't fail entire batch
                error!("Failed to resolve entity: {:?}", e);
                None  // Returns null in response
            }
        };
        results.push(entity);
    }

    Ok(results)
}
```

**Response with errors:**

```json
{
  "data": {
    "_entities": [
      { "id": "123", "email": "alice@example.com" },
      null,  // Resolution failed for this entity
      { "id": "789", "email": "charlie@example.com" }
    ]
  }
}
```

### Performance Optimization: Batching

Instead of resolving entities one-at-a-time, batch them:

```rust
// ❌ INEFFICIENT: 100 separate queries
for rep in representations {
    let entity = resolve_single_entity(&rep).await?;
    results.push(entity);
}

// ✅ EFFICIENT: Single batched query
let entities = resolve_batch(&representations).await?;
results.extend(entities);
```

**Batching strategies:**

| Strategy | Use Case | Performance |
|----------|----------|-------------|
| **Single batch** | < 1000 entities | <5ms |
| **Sub-batches** | 1000-10k entities | <50ms |
| **Streaming** | 10k+ entities | Pipelined |

**Rust implementation uses adaptive batching:**

```rust
const BATCH_SIZE: usize = 1000;  // Adjust based on payload size

if representations.len() <= BATCH_SIZE {
    // Single batch
    resolve_batch(&representations).await
} else {
    // Split into sub-batches, resolve in parallel
    let sub_batches = representations.chunks(BATCH_SIZE);
    futures::future::join_all(
        sub_batches.map(|batch| resolve_batch(batch))
    ).await
}
```

### Strategy Selection at Runtime

The dispatcher chooses the optimal strategy per request:

```rust
fn select_resolution_strategy(
    typename: &str,
    entity_meta: &EntityMetadata,
    db_link_available: bool,
    http_available: bool
) -> ResolutionStrategy {
    // Prefer local (fastest)
    if entity_meta.is_local {
        return ResolutionStrategy::Local;
    }

    // If database linking available and configured, use it
    if db_link_available && entity_meta.database_link.is_some() {
        return ResolutionStrategy::DatabaseLink {
            db_function: entity_meta.database_link.clone().unwrap()
        };
    }

    // Fall back to HTTP (always available)
    ResolutionStrategy::HTTP {
        subgraph_url: entity_meta.subgraph_url.clone()
    }
}
```

### Caching Federation Results

Federation entity resolution results can be cached:

```rust
// Optional: Cache entity resolution results
if schema.federation.cache_enabled {
    let cache_key = format!("{}_{}_{:?}", typename, hash_keys, strategy);

    if let Some(cached) = cache.get(&cache_key).await {
        return cached;
    }

    let entity = resolve_entity(...).await?;
    cache.set(&cache_key, entity.clone()).await?;
    return entity;
}
```

**Cache invalidation:**

- **Local entities**: Invalidate on mutations (automatic via CompiledSchema)
- **HTTP entities**: Cache with TTL, relies on external subgraph
- **Direct DB entities**: Cache with TTL or invalidation rules

---

## 10. Multi-Database Federation Architecture

### The Insight: Direct Database Connections

FraiseQL federation doesn't need FDW, Linked Servers, or FEDERATED because:

**Each FraiseQL subgraph is independently compiled for its database:**

```
Users Subgraph (PostgreSQL):
├── Compiled schema with PostgreSQL WHERE operators
├── v_user view with JSONB data
└── Rust runtime with PostgreSQL driver

Orders Subgraph (SQL Server):
├── Compiled schema with SQL Server WHERE operators
├── v_order view with JSONB data
└── Rust runtime with SQL Server driver

Products Subgraph (MySQL):
├── Compiled schema with MySQL WHERE operators
├── v_product view with JSONB data
└── Rust runtime with MySQL driver
```

**Federation via direct database connections:**

```
Apollo Router
    ↓ HTTP
┌───────────────────────────────────────┐
│ Users Subgraph (PostgreSQL)           │
│ Rust runtime maintains DB connections:│
├─ PostgreSQL: Local database           │
├─ SQL Server: Direct to Orders subgraph│
├─ MySQL: Direct to Products subgraph   │
└───────────────────────────────────────┘
    ↓ PostgreSQL driver queries Users DB
    ↓ SQL Server driver queries Orders DB
    ↓ MySQL driver queries Products DB
```

### Three Resolution Strategies (Simplified)

#### Strategy 1: Local Resolution

Entity owned by current subgraph:

```rust
// Users subgraph resolving User by id
async fn resolve_local(
    entity_type: &str,
    keys: Vec<ID>,
    db: &DatabasePool
) -> Result<Vec<Entity>> {
    // Query local database view: v_user
    let query = "SELECT data FROM v_user WHERE id = ANY($1)";
    let result = db.query(query, &[&keys]).await?;
    Ok(parse_entities(result))
}
```

**Latency:** <5ms (local query)

#### Strategy 2: Direct Database Connection

Entity in another FraiseQL subgraph (accessible via direct DB connection):

```rust
// Users subgraph resolving Order from Orders subgraph
async fn resolve_via_direct_db(
    subgraph_db_url: &str,      // "sqlserver://orders-db/orders_schema"
    entity_type: &str,           // "Order"
    keys: Vec<ID>,
    connection_pool: &ConnectionPool
) -> Result<Vec<Entity>> {
    // Get connection to remote database
    let remote_db = connection_pool.get_connection(subgraph_db_url).await?;

    // Query remote subgraph's view: v_order
    let query = "SELECT data FROM v_order WHERE id = ?";
    let result = remote_db.query(query, &[&keys]).await?;
    Ok(parse_entities(result))
}
```

**Latency:** <10ms (direct DB query, no HTTP overhead)

**Databases supported:**

- PostgreSQL → PostgreSQL, SQL Server, MySQL, SQLite
- SQL Server → PostgreSQL, SQL Server, MySQL, SQLite
- MySQL → PostgreSQL, SQL Server, MySQL, SQLite

#### Strategy 3: HTTP Fallback

Entity in non-FraiseQL subgraph or unreachable database:

```rust
// Users subgraph resolving Product from Apollo Server
async fn resolve_via_http(
    subgraph_url: &str,
    entity_type: &str,
    keys: Vec<ID>,
    http_client: &reqwest::Client
) -> Result<Vec<Entity>> {
    // Standard federation HTTP call
    let response = http_client.post(format!("{}/graphql", subgraph_url))
        .json(&json!({
            "query": build_entity_query(entity_type),
            "variables": { "_representations": build_representations(entity_type, keys) }
        }))
        .send()
        .await?;

    Ok(parse_entities_from_response(response).await?)
}
```

**Latency:** 50-200ms (HTTP round-trip + remote query)

### Compile-Time Strategy Selection

The compiler automatically selects the optimal strategy:

```python
# Compiler phase: Detect federation targets and select strategies
for extended_type in schema.extended_types:
    for field in extended_type.fields:
        if field.is_external_reference:
            target_subgraph = discover_subgraph(field.typename)

            if target_subgraph.is_fraiseql:
                # FraiseQL subgraph: use direct DB connection
                field.resolution_strategy = ResolutionStrategy.DirectDB(
                    db_type=target_subgraph.database_type,
                    db_url=target_subgraph.database_url,
                    schema_name=target_subgraph.schema_name
                )
            else:
                # Non-FraiseQL subgraph: use HTTP
                field.resolution_strategy = ResolutionStrategy.HTTP(
                    subgraph_url=target_subgraph.url
                )
```

**Example detection:**

```python
# Subgraph discovery
def discover_subgraph(typename: str, federation_config: FederationConfig):
    for subgraph in federation_config.subgraphs:
        # Try to detect if it's FraiseQL
        if can_connect_to_database(subgraph.db_url):
            # Check if v_{typename} view exists
            if view_exists(subgraph.db_url, f"v_{typename.lower()}"):
                return SubgraphInfo(
                    is_fraiseql=True,
                    database_type=subgraph.db_type,
                    database_url=subgraph.db_url
                )

    # Not FraiseQL, use HTTP
    return SubgraphInfo(
        is_fraiseql=False,
        http_url=subgraph.graphql_url
    )
```

### Database-Specific WHERE Operators

**Key insight:** Each subgraph uses its own compiled WHERE operators.

When User subgraph (PostgreSQL) federates with Orders subgraph (SQL Server):

```graphql
# Router's query (database-agnostic)
query {
  users(where: { email: { _like: "%@example.com" } }) {
    id
    orders(where: { createdAt: { _gt: "2025-01-01" } }) {
      id
      total
    }
  }
}
```

**Execution:**

```

1. Users subgraph (PostgreSQL):
   - WHERE_operators = [_eq, _ne, _like, _ilike, _regex, _jsonb_has_key, ...]
   - Receives: email { _like: "%@example.com" }
   - Compiles to: WHERE email ILIKE '%@example.com%'
   - Queries PostgreSQL v_user ✅

2. Orders subgraph (SQL Server):
   - WHERE_operators = [_eq, _ne, _like, ...]
   - Receives: createdAt { _gt: "2025-01-01" }
   - Compiles to: WHERE created_at > '2025-01-01'
   - Queries SQL Server v_order directly from Users subgraph ✅
   - (No HTTP call needed!)
```

**Each database executes in its native dialect:**

- PostgreSQL: `ILIKE`, `LIKE`, regex operators, array operators, JSONB operators
- SQL Server: `LIKE`, collation handling, date functions
- MySQL: `REGEXP`, JSON operators, string functions

### Multi-Database Federation Example

```
┌─────────────────────────────────────────┐
│ Apollo Router                           │
└─────────────────────────────────────────┘
    ↓ HTTP (federation protocol)

┌─────────────────────────┐
│ Users Subgraph          │
│ PostgreSQL              │
│ Rust + PostgreSQL driver│
│ Connects to:            │
├─ PostgreSQL (local)     │
├─ SQL Server (Orders)    │
├─ MySQL (Products)       │
└─────────────────────────┘
    ↓ PostgreSQL queries
    ↓ SQL Server direct DB queries
    ↓ MySQL direct DB queries

Database Layer:
├─ PostgreSQL: v_user view with JSONB data
├─ SQL Server: v_order view with JSONB data
└─ MySQL: v_product view with JSONB data
```

**Query execution:**

```
Router: Get users with their orders and products

Users subgraph (_entities for User):
  ├─ PostgreSQL query: SELECT data FROM v_user WHERE id IN (...)
  ├─ Orders subgraph federated reference detected
  ├─ Direct SQL Server connection: SELECT data FROM v_order WHERE user_id IN (...)
  └─ Products subgraph federated reference detected (via Order.product)
      └─ Direct MySQL connection: SELECT data FROM v_product WHERE id IN (...)

Result: User entity with nested Orders and Products
Response sent via HTTP to Router
```

**Performance characteristics:**

| Link | Latency | Mechanism |
|------|---------|-----------|
| Local (PostgreSQL → v_user) | <5ms | Direct query |
| PostgreSQL → SQL Server | <10ms | Direct DB connection + SQL Server query |
| PostgreSQL → MySQL | <10ms | Direct DB connection + MySQL query |
| SQL Server → Apollo Server | 50-200ms | HTTP fallback |

---

## 11. Deployment & Configuration

### Subgraph Configuration

Each FraiseQL subgraph declares which databases it can access:

**`fraiseql.toml` (subgraph configuration):**

```toml
# Local database
[database]
type = "postgresql"
url = "postgresql://user:pass@localhost/users_db"
schema = "users_schema"

# Federation: Declare accessible subgraph databases
[[federation.subgraphs]]
typename = "Order"  # The entity type
is_fraiseql = true
database_type = "sqlserver"
database_url = "sqlserver://user:pass@orders-db/orders_db"
schema_name = "orders_schema"
view_name = "v_order"

[[federation.subgraphs]]
typename = "Product"
is_fraiseql = true
database_type = "mysql"
database_url = "mysql://user:pass@products-db/products_db"
schema_name = "products_schema"
view_name = "v_product"

[[federation.subgraphs]]
typename = "Review"  # Non-FraiseQL: use HTTP
is_fraiseql = false
graphql_url = "https://reviews-api.example.com/graphql"
```

### Compile-Time Validation

The compiler validates federation configuration:

```python
# Compiler phase: Federation validation
def validate_federation_config(authoring_schema, federation_config):
    for extended_type in authoring_schema.extended_types:
        typename = extended_type.name

        # Find subgraph configuration
        subgraph = federation_config.find_subgraph(typename)
        if not subgraph:
            error(f"Extended type {typename} has no federation configuration")

        if subgraph.is_fraiseql:
            # Validate database connectivity
            if not can_connect(subgraph.database_url):
                error(f"Cannot connect to {typename} database: {subgraph.database_url}")

            # Validate view exists
            if not view_exists(subgraph.database_url, subgraph.view_name):
                error(f"View {subgraph.view_name} not found in {typename} database")

            # Validate view has expected JSONB structure
            schema = inspect_view(subgraph.database_url, subgraph.view_name)
            validate_jsonb_structure(schema, extended_type)
```

### Runtime Connection Management

Rust runtime manages connection pools to all accessible databases:

```rust
// Rust runtime initialization
pub struct FederationRuntime {
    local_pool: DatabasePool,           // PostgreSQL
    remote_pools: HashMap<String, DatabasePool>,  // SQL Server, MySQL, etc.
    http_client: reqwest::Client,
}

impl FederationRuntime {
    pub async fn new(config: &FederationConfig) -> Result<Self> {
        let mut remote_pools = HashMap::new();

        // Create connection pools for all FraiseQL subgraphs
        for subgraph in &config.subgraphs {
            if subgraph.is_fraiseql {
                let pool = create_pool(
                    &subgraph.database_type,
                    &subgraph.database_url
                ).await?;
                remote_pools.insert(subgraph.typename.clone(), pool);
            }
        }

        Ok(Self {
            local_pool: create_local_pool(config).await?,
            remote_pools,
            http_client: reqwest::Client::new(),
        })
    }
}
```

### Environment-Specific Configuration

Different environments have different database URLs:

**`.env.local` (development):**

```
FRAISEQL_DATABASE_URL=postgresql://dev:pass@localhost/users_db
FRAISEQL_FEDERATION_ORDERS_URL=sqlserver://dev:pass@localhost/orders_db
FRAISEQL_FEDERATION_PRODUCTS_URL=mysql://dev:pass@localhost/products_db
```

**`.env.production` (production):**

```
FRAISEQL_DATABASE_URL=postgresql://prod:${SECRET_PG_PASS}@pg.prod.internal/users_db
FRAISEQL_FEDERATION_ORDERS_URL=sqlserver://prod:${SECRET_MSSQL_PASS}@mssql.prod.internal/orders_db
FRAISEQL_FEDERATION_PRODUCTS_URL=mysql://prod:${SECRET_MYSQL_PASS}@mysql.prod.internal/products_db
```

### Health Checks

Runtime validates federation connections on startup:

```rust
pub async fn health_check(runtime: &FederationRuntime) -> HealthStatus {
    let mut status = HealthStatus::Healthy;

    // Check local database
    match runtime.local_pool.query("SELECT 1").await {
        Ok(_) => println!("✓ Local database connected"),
        Err(e) => {
            status = HealthStatus::Critical(e.to_string());
            return status;
        }
    }

    // Check remote databases (warnings only, not critical)
    for (typename, pool) in &runtime.remote_pools {
        match pool.query("SELECT 1").await {
            Ok(_) => println!("✓ {} database connected", typename),
            Err(e) => {
                println!("⚠ {} database unavailable: {}", typename, e);
                status = HealthStatus::Degraded;
                // Falls back to HTTP for this entity type
            }
        }
    }

    status
}
```

---

## 12. Federation Examples

### Example 1: PostgreSQL-Only Federation

Both subgraphs on same PostgreSQL instance, different schemas:

**Setup:**

```sql
-- Users subgraph schema
CREATE SCHEMA users_schema;
CREATE TABLE users_schema.tb_user (
    pk_user BIGINT PRIMARY KEY,
    id UUID NOT NULL UNIQUE,
    email TEXT NOT NULL,
    name TEXT NOT NULL
);
CREATE VIEW users_schema.v_user AS
SELECT id, jsonb_build_object('id', id, 'email', email, 'name', name) AS data
FROM users_schema.tb_user;

-- Orders subgraph schema (same instance)
CREATE SCHEMA orders_schema;
CREATE TABLE orders_schema.tb_order (
    pk_order BIGINT PRIMARY KEY,
    id UUID NOT NULL UNIQUE,
    user_id UUID NOT NULL,
    total NUMERIC
);
CREATE VIEW orders_schema.v_order AS
SELECT id, user_id, jsonb_build_object('id', id, 'user_id', user_id, 'total', total) AS data
FROM orders_schema.tb_order;
```

**Subgraph config (Users):**

```toml
[database]
type = "postgresql"
url = "postgresql://user:pass@localhost/shared_db"
schema = "users_schema"

[[federation.subgraphs]]
typename = "Order"
is_fraiseql = true
database_type = "postgresql"
database_url = "postgresql://user:pass@localhost/shared_db"
schema_name = "orders_schema"
view_name = "v_order"
```

**Federation resolution (Rust):**

```rust
// Same PostgreSQL instance, different schemas
// Both views are accessed via single connection pool
let user_entity = local_pool.query(
    "SELECT data FROM users_schema.v_user WHERE id = $1",
    &[&user_id]
).await?;

let order_entity = local_pool.query(
    "SELECT data FROM orders_schema.v_order WHERE user_id = $1",
    &[&user_id]
).await?;
```

**Latency:** <5ms (both queries single connection pool)

---

### Example 2: Multi-Database Federation (PostgreSQL + SQL Server + MySQL)

Three subgraphs on different database types:

**Topology:**

```
Users (PostgreSQL)
    ↓ Direct DB connection
Orders (SQL Server)
    ↓ Direct DB connection
Products (MySQL)
```

**Users subgraph config:**

```toml
[database]
type = "postgresql"
url = "postgresql://user:pass@pg.internal/users_db"
schema = "public"

[[federation.subgraphs]]
typename = "Order"
is_fraiseql = true
database_type = "sqlserver"
database_url = "sqlserver://user:pass@mssql.internal/orders_db"
schema_name = "dbo"
view_name = "v_order"

[[federation.subgraphs]]
typename = "Product"
is_fraiseql = true
database_type = "mysql"
database_url = "mysql://user:pass@mysql.internal/products_db"
schema_name = "products"
view_name = "v_product"
```

**Runtime connection pools:**

```rust
// Users subgraph runtime maintains three pools
federation_runtime = FederationRuntime {
    local_pool: PostgreSQLPool::new("postgresql://..."),
    remote_pools: {
        "Order": SQLServerPool::new("sqlserver://..."),
        "Product": MySQLPool::new("mysql://...")
    },
    http_client: reqwest::Client::new()
}

// When resolving Order from Users:
// Rust executes: SELECT data FROM v_order WHERE user_id = ?
// Via SQL Server driver (not HTTP)
```

**Query execution:**

```
Router: users { orders { products } }

Users subgraph:
  1. Query PostgreSQL v_user → get user entities
  2. Detect Order federation
  3. Query SQL Server v_order directly → get order entities
  4. Detect Product federation
  5. Query MySQL v_product directly → get product entities
  6. Return complete result to router
```

**Latency:**

- PostgreSQL query: <5ms
- SQL Server direct DB query: <10ms
- MySQL direct DB query: <10ms
- **Total: <25ms** (no HTTP round-trips for same-database entities)

---

### Example 3: Mixed Federation (FraiseQL + Apollo Server)

Fallback to HTTP for non-FraiseQL subgraphs:

**Users subgraph config:**

```toml
[database]
type = "postgresql"
url = "postgresql://user:pass@localhost/users_db"

[[federation.subgraphs]]
typename = "Order"
is_fraiseql = true
database_type = "postgresql"
database_url = "postgresql://user:pass@localhost/orders_db"
view_name = "v_order"

[[federation.subgraphs]]
typename = "Review"  # Apollo Server, not FraiseQL
is_fraiseql = false
graphql_url = "https://reviews-api.example.com/graphql"
```

**Runtime decision:**

```rust
// Order: FraiseQL subgraph on same PostgreSQL
// → Use direct DB connection (<10ms)
let order = local_pool.query(
    "SELECT data FROM v_order WHERE user_id = ?",
    &[user_id]
).await?;

// Review: Apollo Server, non-FraiseQL
// → Fall back to HTTP (50-200ms)
let review = http_resolve_entities(
    "https://reviews-api.example.com/graphql",
    &[representation]
).await?;
```

---

### Example 4: Graceful Fallback (Database Connection Failure)

If database connection fails at runtime, automatically fall back to HTTP:

```rust
// Try direct DB connection first
match remote_pools.get("Order").query(...).await {
    Ok(entities) => {
        // Direct DB succeeded
        return Ok(entities);
    }
    Err(db_error) => {
        // Direct DB failed, fall back to HTTP
        warn!("Direct DB connection failed for Order: {}", db_error);
        return http_resolve_entities(
            &config.get_subgraph("Order").graphql_url,
            representations
        ).await;
    }
}
```

**Availability:**

- If database network is down but HTTP is up → Fall back to HTTP
- If HTTP is up but database is down → Still works for other entities
- Degrades gracefully instead of complete failure

---

## 13. Performance Characteristics & Limitations

### Performance: Direct DB vs HTTP

**Local entity resolution:**

```
Query: users(id: [1,2,3])
PostgreSQL: SELECT data FROM v_user WHERE id = ANY($1)
Latency: <5ms (direct database query)
```

**Direct DB entity resolution (same database type):**

```
Query: users(id: [1,2,3]) { orders { id } }

Users subgraph (PostgreSQL):
  1. Query v_user: 2ms
  2. Direct SQL Server connection: Query v_order: 5ms
  3. Return result: 1ms
Total: ~8ms (no HTTP overhead)
```

**Direct DB entity resolution (different database type):**

```
Users (PostgreSQL) → Orders (SQL Server) → Products (MySQL)

  1. PostgreSQL query: 2ms
  2. SQL Server query via direct connection: 5ms
  3. MySQL query via direct connection: 5ms
  4. Network latency between databases: 3-5ms
Total: ~15-20ms
```

**HTTP entity resolution:**

```
Query: users { reviews { rating } }  (Review is Apollo Server)

Users subgraph:
  1. Query v_user: 2ms
  2. HTTP POST to reviews-api: 100-150ms
     - Network round-trip: 50-100ms
     - Remote query execution: 10-50ms
  3. Parse response: 1ms
Total: 103-152ms (10x slower than direct DB)
```

**Comparison table:**

| Scenario | Mechanism | Latency | Example |
|----------|-----------|---------|---------|
| Local entity | Direct query | <5ms | User by ID |
| Same DB, same type | Direct query | <5ms | User → Order (PG→PG) |
| Different instances | Direct DB connection | <15ms | User (PG) → Order (SQL Server) |
| Different database types | Direct DB connection | <20ms | User (PG) → Order (SQL Server) → Product (MySQL) |
| Non-FraiseQL subgraph | HTTP federation | 50-200ms | User → Review (Apollo Server) |
| Database unreachable | HTTP fallback | 50-200ms | Network failure triggers fallback |

### Optimization: Batching

Federation automatically batches multiple entity lookups:

```rust
// Instead of 100 individual queries
for id in [1,2,3,...,100] {
    let order = query_order(id).await?;
}

// Batch into single query
let orders = query_batch_orders(&[1,2,3,...,100]).await?;
```

**Performance impact:**

- Single entity: ~5ms
- 100 entities (batched): ~8ms (not 500ms!)
- 1000 entities (sub-batched): ~50ms

### Limitations & Considerations

#### ✅ Fully Supported

- **Direct DB federation** between FraiseQL subgraphs (any database type)
- **HTTP federation** with Apollo Server and other non-FraiseQL subgraphs
- **Mixed federations** (some FraiseQL via direct DB, some via HTTP)
- **Multi-database scenarios** (PostgreSQL + SQL Server + MySQL + SQLite mix)
- **Graceful fallback** (database unavailable → HTTP)
- **Database-specific WHERE operators** (each subgraph uses its own dialect)
- **Composite keys** (@key with multiple fields)
- **Extended types** (type extension + field resolution)
- **@requires & @provides** (through both direct DB and HTTP)
- **Federation v2 specification** compliance

#### ⚠️ Requires Network Access

**Direct DB federation requires:**

- Network connectivity from Rust runtime to all FraiseQL databases
- Database credentials securely managed
- Firewall rules allowing database connections
- SSL/TLS for encrypted connections

**If network access unavailable:**

- Configure HTTP URL as fallback
- Runtime automatically falls back to HTTP
- Performance degrades to HTTP latency (50-200ms)

#### ⚠️ Database-Specific Configuration

**Each database type has different setup:**

- PostgreSQL: Standard TCP connection
- SQL Server: TCP with authentication
- MySQL: TCP with authentication
- SQLite: File-based (single process, limited federation use)

**Configuration complexity:** Low (just connection strings)

#### ⚠️ Connection Pool Management

**Rust runtime manages pools:**

- One pool per local database
- One pool per remote FraiseQL database
- Total connections = (local pool size) + (N × remote pool size)
- Default pool sizes: 10-20 connections per database
- **Recommendation:** Monitor connection pool utilization

#### ⚠️ Cross-Database Transaction Semantics

**Direct DB federation:**

- Each query executes independently (no ACID across databases)
- Suitable for read-heavy federation
- Mutations: Handle in application layer if multi-database consistency needed

**Example:**

```
User mutation creates Order in Users subgraph
→ Orders subgraph must be updated separately
→ Not in same transaction
```

#### ✅ Federation Debugging

Federation resolution is transparent to query:

```graphql
# This query automatically selects optimal resolution strategy
query {
  users(id: 123) {
    orders {  # Direct DB or HTTP, depends on config
      products {  # Direct DB or HTTP, depends on config
        vendor {  # Direct DB or HTTP, depends on config
          name
        }
      }
    }
  }
}
```

**Enable federation tracing:**

```rust
// Rust runtime can emit traces for federation operations
if config.federation_tracing_enabled {
    trace!("Entity resolution: User");
    trace!("  Strategy: Local (direct query)");
    trace!("  Latency: 3ms");

    trace!("Entity resolution: Order");
    trace!("  Strategy: DirectDB (SQL Server)");
    trace!("  Latency: 6ms");

    trace!("Entity resolution: Review");
    trace!("  Strategy: HTTP (fallback)");
    trace!("  Latency: 120ms");
}
```

### Migration Path: HTTP → Direct DB

**Phase 1: HTTP-only federation**

```
All subgraphs communicate via HTTP
```

**Phase 2: Add direct DB to same-instance subgraph**

```
PostgreSQL (Users) ↔ PostgreSQL (Orders)
├─ Try direct DB: Queries v_order directly
└─ Fallback: HTTP if unavailable
```

**Phase 3: Multi-database optimization**

```
PostgreSQL (Users) ↔ SQL Server (Orders) ↔ MySQL (Products)
├─ Direct DB: Each connection optimized for database type
└─ HTTP: Fallback for external subgraphs
```

**No code changes required** — Compiler auto-detects capabilities and selects strategy.

---

## Summary

**View-Based Federation: An Open Protocol with FraiseQL as Reference Implementation**

**Federation Model:**

- **View-based transport** for database-backed systems (v_* views)
- **HTTP federation** for any GraphQL-compatible system
- **Automatic strategy selection** at runtime

**What FraiseQL Does:**

- Automatically generates v_* views implementing the federation contract
- Maintains connection pools to other database-backed subgraphs
- Falls back to HTTP for external systems
- Each subgraph compiled independently for its database

**What Any System Can Do:**

- Manually implement v_* views to opt into view-based federation
- Gain 10x performance improvement (20ms vs 200ms)
- Keep HTTP available as universal fallback
- No lock-in to FraiseQL ecosystem

**Performance:**

- Local: <5ms
- View-based (database-backed): <20ms
- HTTP (any GraphQL): 50-200ms

**Supported Databases:**

- ✅ PostgreSQL, SQL Server, MySQL (with views)
- ✅ Any database that supports JSON/JSONB columns
- ✅ Any GraphQL service (via HTTP)

**Key Design Principles:**

- **Protocol over implementation** — v_* views are a standard contract, not FraiseQL-specific
- **Opt-in optimization** — HTTP works everywhere; view-based is for those who want performance
- **Database-agnostic** — Each database executes in its native dialect
- **Ecosystem contribution** — Framework feature that benefits the entire federation ecosystem

---

*End of Federation Specification*
