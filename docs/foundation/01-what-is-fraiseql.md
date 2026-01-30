# 1.1: What is FraiseQL?

**Audience:** Developers, architects, technical decision-makers
**Prerequisite:** None (this is foundational)
**Reading Time:** 10-15 minutes

---

## Overview

FraiseQL is a **compiled GraphQL execution engine** that transforms schema definitions into optimized SQL at build time, eliminating runtime overhead and enabling deterministic, high-performance query execution.

Unlike traditional GraphQL servers that interpret queries at runtime, FraiseQL compiles your entire schema and all possible queries upfront, generating optimized SQL templates that execute with zero interpretation overhead.

**One-sentence definition:** *A build-time compilation system that transforms Python or TypeScript schema definitions into production-ready GraphQL APIs backed by relational databases.*

---

## The Core Insight: Why FraiseQL?

### The Problem with Traditional GraphQL

Traditional GraphQL servers (Apollo Server, Hasura, WunderGraph) excel at flexibility but struggle with performance and complexity at scale:

- **Runtime overhead:** GraphQL queries are parsed and interpreted on every request
- **Schema interpretation:** Type resolution and relationship handling happen at query time
- **Resolver complexity:** Teams write custom code for data loading, caching, and optimization
- **Debugging difficulty:** Performance issues are hard to diagnose (where's the bottleneck?)
- **Scaling challenges:** More queries, more CPUs needed

### FraiseQL's Approach: Compilation, Not Interpretation

FraiseQL shifts work from runtime to build time:

| Aspect | Traditional GraphQL | FraiseQL |
|--------|-------------------|----------|
| **When schema changes** | Deployed live | Compiled, verified, optimized |
| **When query arrives** | Parse & interpret | Execute pre-compiled template |
| **Data relationships** | Resolved at runtime | Compiled into SQL joins |
| **Query optimization** | Heuristics at runtime | Comprehensive build-time analysis |
| **Debugging** | Runtime tracing, profilers | Examine compiled SQL, pre-execution plans |

**Result:** Predictable performance, zero runtime surprises, deterministic behavior.

---

## What FraiseQL Solves

### 1. Performance & Predictability

**Traditional GraphQL Problem:**
```python
# Apollo Server - Query interpreted at runtime
query GetUserOrders {
  user(id: 1) {
    name
    orders {
      total
      items {
        name
        price
      }
    }
  }
}
# Runtime: Parse, validate schema, resolve relationships, execute N queries
# Speed depends on resolver implementation and caching
```

**FraiseQL Solution:**
```python
# FraiseQL - Query compiled to optimized SQL
query GetUserOrders {
  user(id: 1) {
    name
    orders {
      total
      items {
        name
        price
      }
    }
  }
}
# Compile time: Generated optimal SQL (single JOIN or intentional batching)
# Runtime: Execute pre-compiled template
# Speed: Predictable, database-native performance
```

**Benefit:** Queries execute at database speed, not application speed.

### 2. Type Safety & Correctness

All type checking, validation, and authorization rules are verified at compile time:

```python
# Phase 1: Schema Authoring (Python)
@fraiseql.type
class User:
    user_id: int  # Maps to pk_user
    name: str
    email: str
    orders: List[Order]  # Relationship validated at compile time

# Phase 2: Compilation
# - Verify User.user_id matches database pk_user
# - Check relationships exist (foreign keys)
# - Generate type-safe resolvers

# Phase 3: Runtime
# - Execute type-safe queries
# - Zero runtime type coercion errors
```

**Benefit:** Catch schema errors before production.

### 3. Database Alignment

FraiseQL treats the database as the primary source of truth, not an afterthought:

```sql
-- Your database schema defines your API
CREATE TABLE tb_users (
    pk_user BIGINT PRIMARY KEY,
    username VARCHAR(255),
    email VARCHAR(255)
);

CREATE TABLE tb_orders (
    pk_order BIGINT PRIMARY KEY,
    fk_user BIGINT REFERENCES tb_users(pk_user),
    total DECIMAL(10, 2),
    created_at TIMESTAMP
);

-- FraiseQL automatically derives GraphQL API from this schema
# { user { username orders { total } } }  ← Automatically available
```

**Benefit:** Schema is source of truth; no impedance mismatch.

### 4. Operational Simplicity

Traditional GraphQL requires:
- Custom resolver code
- N+1 query prevention logic
- Cache invalidation strategies
- Authorization middleware
- Error handling layers

FraiseQL handles this at compile time:

```python
# FraiseQL: All built-in
@fraiseql.query
def get_user(user_id: int) -> User:
    """Automatically:
    - Compiled to optimal SQL
    - N+1 prevention via batching
    - Authorization rules applied
    - Result caching configured
    - Error handling defined
    """
    return db.query(User).filter(user_id=user_id).first()
```

**Benefit:** Less code, fewer moving parts, easier debugging.

---

## When to Use FraiseQL

### ✅ FraiseQL is Ideal For

**1. Data-Centric Applications**
- Databases are your primary data source
- GraphQL is an API layer over relational databases
- You want type-safe database access
- Example: E-commerce, SaaS, analytics platforms

**2. Performance-Critical Systems**
- Latency and throughput matter
- You need predictable query performance
- N+1 queries are unacceptable
- Example: High-volume APIs, real-time platforms, data pipelines

**3. Structured Data Domains**
- Data follows clear schemas (not fully unstructured)
- Relationships are well-defined (foreign keys)
- Authorization is role/permission-based
- Example: Business applications, operational systems

**4. Teams That Want Simplicity**
- Your team wants less custom code
- You prefer convention over configuration
- You want clear, debuggable query execution
- Example: Startups, lean teams, DevOps-focused shops

---

### Real-World Examples

#### Example 1: E-Commerce Platform

**Challenge:** Managing complex product catalog with fast searches

```python
# FraiseQL Schema
@fraiseql.type
class Product:
    product_id: int
    name: str
    price: Decimal
    in_stock: bool  # Synced from tb_inventory
    reviews: List[Review]  # One-to-many

@fraiseql.query
def search_products(query: str, category: str) -> List[Product]:
    """Returns matching products with stock status and reviews"""
    pass
```

**Result:**
- Compiled to single optimized SQL query (no N+1)
- In-stock filter pushed to database
- Reviews batched in single query
- Results cached automatically

#### Example 2: Multi-Tenant SaaS

**Challenge:** Isolating data between tenants, enforcing permissions

```python
# FraiseQL handles tenant isolation
@fraiseql.type
class Invoice:
    invoice_id: int
    customer_id: int
    amount: Decimal
    created_at: datetime

# At compile time, FraiseQL generates:
# - Tenant isolation in every query (WHERE tenant_id = ?)
# - Authorization checks (can user see this tenant's data?)
# - Audit logging rules
```

**Result:**
- Zero tenant data leaks (verified at compile time)
- Permission checks optimized into SQL
- Audit trail automatic

#### Example 3: Data Pipeline

**Challenge:** Moving 1GB datasets efficiently

```graphql
# FraiseQL: Arrow Flight data plane for columnar output
query GetUserAnalytics {
  users(limit: 1000000) {
    user_id
    created_at
    total_spent
    last_purchase_at
  }
}
# Arrow Flight: 100+ MB/s columnar streaming
# vs JSON plane: 10-20 MB/s row-by-row
```

**Result:**
- 5-10x throughput improvement
- Lower memory usage
- Direct to analytics pipeline

---

## When NOT to Use FraiseQL

### ❌ FraiseQL Is Not Ideal For

**1. Highly Dynamic APIs**
- Your schema changes frequently (not at build time)
- You need runtime schema customization
- GraphQL is more than a database API
- **Alternative:** Apollo Server, WunderGraph

**2. Unstructured or Document-Based Data**
- Data doesn't fit relational schema
- You need flexible document queries
- **Alternative:** Hasura, PostGraphile, custom resolvers

**3. Microservices Federation Heavy**
- You're aggregating many external APIs
- GraphQL is orchestration layer, not data access
- You need extensive transformation logic
- **Alternative:** Apollo Federation, WunderGraph

**4. Extremely Simple APIs**
- Your API is trivial (why pay compilation cost?)
- You just need a REST wrapper
- **Alternative:** Simple REST API, GraphQL for simple cases

---

## Comparison with Alternatives

### FraiseQL vs Apollo Server

| Feature | FraiseQL | Apollo Server |
|---------|----------|---------------|
| **How it works** | Compiled schema → SQL | Interpreted resolvers |
| **Performance** | Database-native, predictable | Application-dependent |
| **Setup** | Define schema, run compiler | Write resolvers |
| **Flexibility** | Database-first | Unlimited (code-first) |
| **Type Safety** | Compile-time (all paths) | Runtime (with TypeScript) |
| **Best For** | Data APIs, performance | Flexible APIs, custom logic |

**Winner for:** E-commerce, data platforms, SaaS
**Winner for:** Microservices, custom logic, flexible schemas

---

### FraiseQL vs Hasura

| Feature | FraiseQL | Hasura |
|---------|----------|--------|
| **Schema Definition** | Python/TypeScript code | Visual editor or YML |
| **How it works** | Compiled | Interpreted (with caching) |
| **Learning Curve** | Developers familiar with code | Visual, quick start |
| **Customization** | Database-centric, limited custom logic | Webhooks for custom code |
| **Performance** | Optimal compiled SQL | Good, cached execution |

**Winner for:** Teams with engineering rigor, performance needs
**Winner for:** Rapid prototyping, visual-first teams

---

### FraiseQL vs Custom REST + Resolvers

| Aspect | FraiseQL | Custom REST |
|--------|----------|-------------|
| **Development Time** | Fast (schema → API) | Slow (write all endpoints) |
| **Consistency** | High (compiled) | Varies (depends on code quality) |
| **Performance** | Predictable | Depends on implementation |
| **Type Safety** | Full (compile-time) | Partial (depends on code quality) |

**FraiseQL wins on:** Time-to-value, consistency, predictability

---

## Who Uses FraiseQL?

### Ideal Users: Your Target Audience

**1. Python Data Engineers**
- Building data platforms
- Need typed, performant APIs
- Familiar with databases
- Want GraphQL without complexity

**2. TypeScript/Node Teams**
- Building SaaS applications
- Want type-safe backend
- Performance-conscious
- Prefer conventions over custom code

**3. DevOps/SRE Teams**
- Operating database-centric systems
- Want predictable performance
- Need observability
- Prefer less custom code to maintain

**4. Technical Architects**
- Designing new systems
- Need clear performance model
- Want proven patterns
- Building with PostgreSQL, MySQL, SQLite, SQL Server

---

## What You Get With FraiseQL

### By the Numbers

- **Zero runtime interpretation** — All GraphQL semantics resolved at build time
- **100% type safety** — Every field, type, and relationship verified at compile time
- **Deterministic performance** — Query speed depends on database, not application code
- **N+1 proof** — Relationship loading compiled into SQL joins
- **Built-in authorization** — Permissions compiled into every query
- **Multi-database support** — PostgreSQL, MySQL, SQLite, SQL Server
- **Production-ready** — No custom resolver code needed

---

## The Three Layers

FraiseQL's architecture separates concerns:

### Layer 1: Authoring
```python
# Your Python or TypeScript code
@fraiseql.type
class User:
    user_id: int
    name: str
    emails: List[Email]
```

### Layer 2: Compilation
```bash
$ fraiseql-cli compile schema.py
# Generates: schema.compiled.json + SQL templates
```

### Layer 3: Runtime
```bash
$ fraiseql-server --schema schema.compiled.json
# Executes GraphQL queries using pre-compiled SQL
```

**Benefit:** Clear separation of concerns, easy to reason about.

---

## Next Steps

If FraiseQL sounds like a fit:

1. **Understand the fundamentals** → Read Topic 1.2 (Core Concepts & Terminology)
2. **Learn how compilation works** → Read Topic 2.1 (Compilation Pipeline)
3. **Start authoring schemas** → Read Topic 3.1 (Python Schema Authoring) or Topic 3.2 (TypeScript)

If you need something else:

- **Rapid prototyping?** → Try Hasura
- **Maximum flexibility?** → Use Apollo Server
- **Just need REST?** → Build simple REST API

---

## Key Takeaways

✅ **FraiseQL is a compiled GraphQL execution engine** that transforms schema definitions into optimized SQL at build time

✅ **Best for:** Database-centric applications, performance-critical systems, teams valuing simplicity

✅ **Core benefits:** Predictable performance, type safety, zero N+1 queries, operational simplicity

✅ **Different from traditional GraphQL:** Compilation instead of interpretation, database-first not code-first

✅ **Next: Learn the core concepts and architecture** in subsequent topics

---

## Related Topics

- **Topic 1.2:** Core Concepts & Terminology — Understand FraiseQL vocabulary
- **Topic 2.1:** Compilation Pipeline — How schemas become executable APIs
- **Topic 3.1:** Python Schema Authoring — Start writing FraiseQL schemas
- **Topic 1.5:** FraiseQL Compared to Other Approaches — Detailed comparisons

---

**Quick Reference:**
- Problem: Traditional GraphQL interprets at runtime
- Solution: FraiseQL compiles at build time
- Result: Predictable performance, type safety, simplicity
- Use When: Database-centric applications with performance needs
- Skip When: Unstructured data, heavy microservices federation, trivial APIs
