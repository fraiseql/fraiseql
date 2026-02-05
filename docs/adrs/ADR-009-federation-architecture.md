# ADR-009: Federation Architecture (Direct DB + HTTP Fallback)

**Date:** January 11, 2026
**Status:** Accepted
**Authors:** FraiseQL Architecture Team
**Relates to:** federation.md, PRD.md Section 6.1

---

## Problem Statement

How should FraiseQL implement federation for composing multiple subgraphs into a single federated graph?

**Context:**

- FraiseQL is a compiled GraphQL backend with database as the primary source of truth
- Each subgraph is independently compiled for its target database
- Federation enables composition of multiple FraiseQL instances + external subgraphs
- Performance and simplicity are critical concerns

**Questions to resolve:**

1. Should federation use HTTP only, or optimize for FraiseQL-to-FraiseQL coupling?
2. How to handle multi-database scenarios (PostgreSQL + SQL Server + MySQL)?
3. Should database-level linking mechanisms (FDW, Linked Servers) be used?
4. How to preserve database-specific WHERE operators across subgraphs?

---

## Options Considered

### Option 1: HTTP-Only Federation (Standard Apollo Federation v2)

**Approach:**

- All entity resolution via HTTP POST to external subgraph's `_entities` endpoint
- Works with any GraphQL server (Apollo Server, Yoga, Mercurius, etc.)
- Database-agnostic; no special setup required

**Advantages:**

- ✅ Simplicity: Standard federation protocol
- ✅ Compatibility: Works with ANY GraphQL server
- ✅ Portability: No database-specific configuration
- ✅ No network assumptions between subgraphs

**Disadvantages:**

- ❌ Performance: 50-200ms latency for each entity batch
- ❌ Lost optimization opportunity for FraiseQL-to-FraiseQL
- ❌ Network overhead even for same-database subgraphs
- ❌ Connection complexity: Each subgraph needs HTTP endpoint

**Example latency:**

```text
User → Order federation:
  1. HTTP POST to Orders subgraph: 100-150ms
  2. Network round-trip: 50-100ms
  3. Remote database query: 10-50ms
```text

---

### Option 2: Database-Level Linking (FDW, Linked Servers, FEDERATED)

**Approach:**

- Use database-specific mechanisms for same-database subgraphs:
  - PostgreSQL: FDW (Foreign Data Wrapper)
  - SQL Server: Linked Servers
  - MySQL: FEDERATED storage engine
- HTTP fallback for cross-database or external subgraphs

**Advantages:**

- ✅ Performance: <10ms for same-database
- ✅ Optimization for tightly-coupled services
- ✅ Native database mechanisms

**Disadvantages:**

- ❌ Complexity: Different setup per database type
- ❌ Operational overhead: FDW/Linked Servers configuration
- ❌ Database-specific knowledge required
- ❌ Fragile: Connection failures hard to diagnose
- ❌ Not portable: Requires specific database instances
- ❌ Cross-database still needs HTTP

---

### Option 3: Direct Database Connections from Rust Runtime (SELECTED)

**Approach:**

- Rust runtime maintains connection pools to all accessible FraiseQL databases
- For each entity resolution request:
  1. Detect if target is FraiseQL subgraph or external
  2. If FraiseQL: Query remote database directly via native driver (PostgreSQL, SQL Server, MySQL)
  3. If external: HTTP POST to subgraph's `_entities` endpoint
- Each subgraph independently compiled for its database
- Each database executes queries in its native dialect

**Entity Resolution Strategies:**

1. **Local Resolution** (<5ms)

   ```text
   User subgraph resolving User entity
   → Query local PostgreSQL database
   → SELECT data FROM v_user WHERE id = ?
   ```text

2. **Direct DB Federation** (<10-20ms)

   ```text
   User subgraph resolving Order entity (from Orders subgraph on SQL Server)
   → Rust runtime has SQL Server connection pool
   → Query remote SQL Server directly
   → SELECT data FROM v_order WHERE user_id = ?
   ```text

3. **HTTP Fallback** (50-200ms)

   ```text
   User subgraph resolving Review entity (from Apollo Server)
   → HTTP POST to reviews-api.example.com/graphql
   → _entities query with representation
   ```text

**Advantages:**

- ✅ Performance: <5-20ms for FraiseQL-to-FraiseQL (10x better than HTTP)
- ✅ Simplicity: Just connection strings, no FDW/Linked Servers setup
- ✅ Portability: Works with PostgreSQL, SQL Server, MySQL, SQLite
- ✅ Multi-database support: PostgreSQL + SQL Server + MySQL in single graph
- ✅ Database-agnostic: Each database executes in native dialect
- ✅ Graceful degradation: Falls back to HTTP if database unreachable
- ✅ Full Apollo Federation v2 compliance
- ✅ Works with external (non-FraiseQL) subgraphs
- ✅ No complex database linking mechanisms

**Disadvantages:**

- ⚠️ Requires network access from Rust runtime to remote databases
- ⚠️ Database credentials must be configured
- ⚠️ Firewall rules needed for database connections
- ⚠️ No ACID transactions across databases (acceptable: read-heavy federation)

---

## Decision

**SELECTED: Option 3 - Direct Database Connections from Rust Runtime**

### Rationale

1. **Simplicity with Optimization:**
   - Avoids FDW/Linked Servers complexity
   - Just connection strings + driver configuration
   - Rust naturally handles multi-database via multiple drivers

2. **Performance & Portability:**
   - 10x faster for FraiseQL-to-FraiseQL (20ms vs 200ms)
   - Works equally well with PostgreSQL, SQL Server, MySQL, SQLite
   - No database-specific setup

3. **Database-Specific Operators Preserved:**
   - Each subgraph independently compiled
   - PostgreSQL uses `ILIKE`, `REGEXP`, `JSONB` operators
   - SQL Server uses `LIKE`, collation handling
   - MySQL uses `REGEXP`, JSON operators
   - No translation layer needed

4. **Multi-Database Scenarios:**
   - Single federation graph can mix databases
   - PostgreSQL Users + SQL Server Orders + MySQL Products = works naturally
   - Cross-database falls back to HTTP automatically
   - No special configuration per-database-pair

5. **Enterprise Readiness:**
   - Full Apollo Federation v2 compliance
   - Works with Apollo Server, Yoga, Mercurius, etc.
   - Gradual migration path: HTTP-only → Direct DB optimization
   - Graceful fallback if database unavailable

6. **Architectural Alignment:**
   - Consistent with FraiseQL philosophy: "database as source of truth"
   - Rust runtime layer handles all complexity
   - Each subgraph independently deployable

---

## Implementation

### Compile-Time

**Phase: Federation Analysis & Validation**

```python
# Compiler detects federation targets
for extended_type in schema.extended_types:
    target_subgraph = discover_subgraph(extended_type.typename)

    if target_subgraph.is_fraiseql:
        # FraiseQL subgraph: use direct DB
        field.resolution_strategy = ResolutionStrategy.DirectDB(
            db_type=target_subgraph.database_type,
            db_url=target_subgraph.database_url
        )
    else:
        # External subgraph: use HTTP
        field.resolution_strategy = ResolutionStrategy.HTTP(
            subgraph_url=target_subgraph.graphql_url
        )
```text

### Runtime

**Initialization:**

```rust
pub struct FederationRuntime {
    local_pool: DatabasePool,                      // Local database
    remote_pools: HashMap<String, DatabasePool>,   // Remote FraiseQL databases
    http_client: reqwest::Client,                  // For external subgraphs
}

// Create pools for all accessible FraiseQL databases
for subgraph in &config.subgraphs {
    if subgraph.is_fraiseql {
        remote_pools.insert(
            subgraph.typename,
            create_pool(subgraph.db_type, subgraph.db_url)
        );
    }
}
```text

**Entity Resolution:**

```rust
pub async fn resolve_entities(representations: Vec<_Any>) -> Result<Vec<Entity>> {
    for (typename, reps) in group_by_typename(representations) {
        match select_strategy(typename) {
            Strategy::Local => resolve_local(typename, reps),
            Strategy::DirectDB => resolve_via_direct_db(typename, reps),
            Strategy::HTTP => resolve_via_http(typename, reps),
        }
    }
}
```text

---

## Deployment

### Configuration

**`fraiseql.toml` (subgraph declares accessible databases):**

```toml
[database]
type = "postgresql"
url = "postgresql://user:pass@localhost/users_db"

[[federation.subgraphs]]
typename = "Order"
is_fraiseql = true
database_type = "sqlserver"
database_url = "sqlserver://user:pass@orders-db/orders_db"

[[federation.subgraphs]]
typename = "Review"
is_fraiseql = false
graphql_url = "https://reviews-api.example.com/graphql"
```text

### Health Checks

- Local database: Critical (fails if unavailable)
- Remote databases: Degraded (falls back to HTTP if unavailable)
- External subgraphs: Checked via HTTP

---

## Performance Characteristics

| Scenario | Mechanism | Latency | Notes |
|----------|-----------|---------|-------|
| Local entity | Direct query | <5ms | Single database query |
| Same DB, same type | Direct query | <5ms | PostgreSQL → PostgreSQL |
| Different instances | Direct DB | <10-20ms | Network latency included |
| Different DB types | Direct DB | <10-20ms | PostgreSQL → SQL Server |
| External subgraph | HTTP | 50-200ms | Network + remote query |
| DB unavailable | HTTP fallback | 50-200ms | Automatic fallback |

---

## Risk Mitigation

### Network Access Risk

- **Risk:** Rust runtime needs network access to remote databases
- **Mitigation:** Firewall rules, VPC configuration, SSL/TLS encryption
- **Fallback:** HTTP endpoint configured for each external subgraph

### Credential Management

- **Risk:** Database credentials in configuration
- **Mitigation:** Environment variables, secrets management, encrypted storage
- **Audit:** All database access logged via audit columns

### Connection Pool Exhaustion

- **Risk:** Too many connections from runtime
- **Mitigation:** Configurable pool sizes, monitoring, alerts
- **Default:** 10-20 connections per database

### Cross-Database Consistency

- **Risk:** Mutations across databases not atomic
- **Mitigation:** Document limitation, handle in application layer
- **Use case:** Read-heavy federation (OK), complex mutations (needs orchestration)

---

## Alternatives Rejected

### FDW-Only (Database-Level Linking)

- ❌ Too complex for operational benefit
- ❌ PostgreSQL-specific doesn't solve multi-database
- ❌ SQL Server Linked Servers are fragile
- ❌ MySQL FEDERATED has consistency issues

### HTTP-Only (No Direct DB Optimization)

- ❌ Loses 10x performance opportunity for FraiseQL-to-FraiseQL
- ❌ Unnecessary network overhead for same-database

### Hybrid (HTTP + Optional FDW)

- ❌ Adds complexity without significant benefit
- ❌ Direct DB connections simpler than FDW setup
- ❌ Still supports same-database optimization

---

## Future Directions

1. **gRPC Federation** (Lower latency for external subgraphs)
   - Could optimize HTTP path for performance-critical scenarios
   - Requires opt-in from external subgraph

2. **Connection Pooling Optimizations**
   - pgbouncer for PostgreSQL
   - Connection multiplexing for SQL Server
   - Connection pooling per-database-pair

3. **Distributed Transactions**
   - Two-phase commit across databases (if needed)
   - Sagas for eventual consistency
   - Still out-of-scope for v1.0

---

## References

- `docs/architecture/integration/federation.md` — Complete federation specification
- `docs/prd/PRD.md` Section 6.1 — Federation requirements
- Apollo Federation v2 specification — Standard protocol
- `docs/GLOSSARY.md` — Federation terminology

---

## Approval

- [x] Architecture Team
- [x] Runtime Team
- [x] Security Review (database connection security)
- [x] Operations Review (deployment considerations)

---

*End of ADR-009*
