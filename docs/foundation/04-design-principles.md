# 1.4: Design Principles

**Audience:** Architects, team leads, technical decision-makers
**Prerequisite:** Topics 1.1 (What is FraiseQL?), 1.2 (Core Concepts), 1.3 (Database-Centric Architecture)
**Reading Time:** 15-20 minutes

---

## Overview

FraiseQL is built on five core design principles that guide every architectural decision. These principles explain not just *what* FraiseQL does, but *why* it does it that way. Understanding these principles helps you:

- Know when FraiseQL is the right choice
- Predict how FraiseQL will behave in edge cases
- Understand the tradeoffs you're making
- Design your schema and data model for optimal results

---

## Principle 1: Database-Centric Design

**Statement:** *The database is the primary application interface, not an implementation detail.*

### What This Means

In FraiseQL, you don't build a GraphQL schema and then map it to a database. Instead, you design your **database schema first**, then derive your GraphQL API from it.

```text
Traditional Approach:
┌──────────────────┐
│  GraphQL Schema  │  (source of truth)
│  (in code)       │
└────────┬─────────┘
         │ (maps to)
┌────────▼─────────┐
│  Database Schema │  (implementation)
└──────────────────┘

FraiseQL Approach:
┌──────────────────┐
│ Database Schema  │  (source of truth)
│ (views, tables)  │
└────────┬─────────┘
         │ (generates)
┌────────▼─────────┐
│  GraphQL Schema  │  (API interface)
└──────────────────┘
```text

### Why This Matters

**1. Database constraints become API guarantees**

```sql
-- PostgreSQL
CREATE TABLE tb_users (
  pk_user_id SERIAL PRIMARY KEY,
  email VARCHAR(255) NOT NULL UNIQUE,  -- Constraint at database level
  created_at TIMESTAMP NOT NULL DEFAULT NOW()
);
```text

When you define `NOT NULL` in the database, FraiseQL's GraphQL schema will correctly reflect that the field is non-nullable. You don't need to specify it twice.

**2. Database performance directly translates to API performance**

```sql
-- Adding a database index
CREATE INDEX idx_users_email ON tb_users(email);
```text

This index doesn't require code changes to your FraiseQL schema. The GraphQL API automatically gets faster because the underlying SQL queries execute faster.

**3. Database expertise becomes API design expertise**

A good database schema—with clear relationships, appropriate denormalization, and strategic views—automatically becomes a good GraphQL API. Your database team's knowledge directly benefits API design.

### Implications

- ✅ Your database schema *is* your API contract
- ✅ Database changes can be tested independently before API deployment
- ✅ Database metrics directly predict API performance
- ❌ You cannot add fields to the GraphQL API that don't exist in the database
- ❌ You cannot hide database limitations—they become API limitations

---

## Principle 2: Compile-Time Optimization

**Statement:** *Move as much work as possible from runtime to compile time.*

### What This Means

FraiseQL has three distinct phases:

```text
Authoring (Developer writes code)
    ↓
Compilation (fraiseql-cli processes schema)
    ↓
Runtime (Server executes queries)
```text

Each phase should be optimized separately:

**Authoring:** Easy and ergonomic (Python/TypeScript decorators)

```python
from fraiseql import schema

@schema.type(table="tb_users")
class User:
    user_id: int
    email: str
    created_at: datetime
```text

**Compilation:** Expensive but one-time (comprehensive validation and optimization)

```bash
fraiseql-cli compile schema.json
# Validates relationships, optimizes joins, generates SQL templates
# Takes seconds, but only runs once at build time
```text

**Runtime:** Fast and deterministic (execute pre-compiled templates)

```graphql
query GetUser($id: Int!) {
  user(id: $id) {
    id
    email
  }
}
# Executes pre-compiled SQL template: no parsing, no validation, no interpretation
```text

### Why This Matters

**1. Predictable performance**

Every request executes the same pre-compiled code path. No interpretation overhead. No JIT compilation surprises. No query plan variations.

**2. Early error detection**

Schema errors, relationship problems, and type mismatches are caught at compile time, not when a user tries that query.

**3. Security improvements**

All SQL is pre-compiled at compile time, eliminating injection vulnerabilities. Parameters are always bound safely.

### Implications

- ✅ Runtime performance is highly predictable
- ✅ All queries execute in O(1) time (lookup + execute)
- ✅ Validation errors caught before production
- ✅ Security properties verified at compile time
- ❌ Dynamic queries not supported (all queries must be known at compile time)
- ❌ Schema changes require recompilation and redeployment
- ❌ Runtime query introspection is limited to compiled schema

---

## Principle 3: Type Safety as a Constraint

**Statement:** *Types should be enforced at the database layer, not as a suggestion.*

### What This Means

In FraiseQL, type safety is enforced at multiple levels:

1. **Database schema enforcement**

   ```sql
   CREATE TABLE tb_products (
     pk_product_id INT PRIMARY KEY,
     name VARCHAR(255) NOT NULL,      -- String type
     price NUMERIC(10, 2) NOT NULL,   -- Numeric type
     is_active BOOLEAN NOT NULL       -- Boolean type
   );
   ```text

2. **GraphQL schema enforcement**

   ```graphql
   type Product {
     id: Int!
     name: String!
     price: Float!
     isActive: Boolean!
   }
   ```text

3. **Authorization enforcement**

   ```python
   @schema.permission("user_role = 'admin'")
   def delete_user(user_id: int) -> None:
       pass
   ```text

### Why This Matters

**1. Invalid states become impossible**

You cannot accidentally query a null field that's marked as non-null. You cannot update a string field with a number. The type system prevents invalid operations before they reach the database.

**2. Database constraints are API guarantees**

Foreign keys in the database become relationship requirements in the API. Unique constraints become uniqueness guarantees.

**3. Permissions become part of the type system**

Authorization rules are checked at compile time for consistency and at runtime for enforcement.

### Example: Type Safety in Action

```python
# Bad schema (types don't match database)
@schema.type(table="tb_users")
class User:
    user_id: str  # ❌ Wrong: database column is INT
    email: str

# Good schema (types match database exactly)
@schema.type(table="tb_users")
class User:
    user_id: int  # ✅ Correct: matches INT column
    email: str
```text

### Implications

- ✅ Type mismatches caught at compile time
- ✅ Invalid data cannot reach the database
- ✅ Client developers know exactly what types to expect
- ❌ Cannot work around database type constraints in the API
- ❌ Type changes require schema recompilation

---

## Principle 4: Performance Through Determinism

**Statement:** *Deterministic behavior enables optimization and predictability.*

### What This Means

Because FraiseQL compiles queries and uses only the database as a data source, the performance characteristics of every query are determined at compile time:

**Compile-time analysis can answer:**

- How many database queries will this GraphQL query execute?
- What indexes are needed for optimal performance?
- Can this query cause N+1 problems?
- What's the worst-case latency?

```python
# FraiseQL can analyze this and guarantee no N+1 queries
query GetUserOrders {
  user(id: 1) {        # 1 query
    orders {           # Automatically joined or batched
      items {          # Automatically joined or batched
        product {      # Automatically joined or batched
          name
        }
      }
    }
  }
}
# Compile time: Plan the optimal SQL query structure
# Runtime: Execute single optimized query (or pre-batched queries)
```text

### Why This Matters

**1. Database teams can optimize effectively**

With deterministic queries, database teams can identify bottlenecks, create appropriate indexes, and validate query plans before deployment.

**2. Capacity planning becomes predictable**

You can measure the cost of each query and accurately predict system capacity needs.

**3. Performance regressions are detectable**

If a query suddenly becomes slow, you know something changed in the database (new data, missing index, schema change)—not a code change in the application layer.

### Implications

- ✅ Query performance is predictable and reproducible
- ✅ Optimization efforts have clear ROI
- ✅ Performance problems are database problems (visible and fixable)
- ❌ Unexpected data patterns can cause issues (e.g., if a query becomes expensive as data grows)
- ❌ Cannot adapt queries at runtime based on data distribution

---

## Principle 5: Simplicity Over Flexibility

**Statement:** *Assume a single primary data source and optimize for that case.*

### What This Means

FraiseQL is designed with a core assumption: **Your primary data is in a relational database.** If that's true, everything else becomes simpler.

```text
If your data is in a single relational database:
  - No need to write custom resolvers
  - No need to orchestrate data from multiple sources
  - No need to implement caching logic
  - No need to handle relationship resolution

Result: A GraphQL API with minimal code
```text

### Examples of This Principle

**Single source of truth**

```python
# FraiseQL assumes: one database, source of truth
@schema.type(table="tb_users")
class User:
    user_id: int
    email: str
    # No custom resolver logic needed
    # No data fetching from external APIs
    # No caching management
```text

**Relationships are explicit**

```sql
-- Foreign keys define relationships
ALTER TABLE tb_orders
  ADD CONSTRAINT fk_orders_user
  FOREIGN KEY (fk_user_id) REFERENCES tb_users(pk_user_id);
```text

FraiseQL uses these foreign keys to automatically enable GraphQL relationships. No custom resolver needed.

**Multi-database, single schema**

```python
# You can use multiple databases (PostgreSQL + MySQL + SQLite)
# But each is treated independently
# No cross-database joins, no distributed transactions
@schema.source("postgres", db_url="postgresql://...")
@schema.source("mysql", db_url="mysql://...")
class Schema:
    pass
```text

Each database is a separate source of truth for its own domain.

### Why This Matters

**1. Less code to write and maintain**

No resolvers, no data loaders, no caching logic. Just schema definitions.

**2. Easier to understand**

A simpler system is easier to reason about, easier to debug, and easier to optimize.

**3. Better performance**

Without flexibility overhead, you get better performance for the common case (single relational database).

### Implications

- ✅ Minimal schema code needed (just table definitions)
- ✅ Clear data ownership and responsibility
- ✅ Easy to understand and audit data access
- ❌ Cannot easily aggregate data from multiple external sources
- ❌ Cannot cache derived data (other than database views)
- ❌ Complex calculations must happen in the database or between queries

---

## How These Principles Work Together

The five principles form a coherent design philosophy:

```text
Database-Centric Design (Principle 1)
    ↓
    Means: Database schema is your API contract
    ↓
Compile-Time Optimization (Principle 2)
    ↓
    Means: Validate and optimize schema at build time
    ↓
Type Safety (Principle 3)
    ↓
    Means: Enforce types at every layer
    ↓
Performance Through Determinism (Principle 4)
    ↓
    Means: Predictable, optimizable queries
    ↓
Simplicity Over Flexibility (Principle 5)
    ↓
    Means: Single data source, minimal code
```text

### Real-World Consequence: Auditing

These principles together enable a powerful property: **complete query auditability**.

```sql
-- Every GraphQL query compiles to a predictable SQL query
-- Which you can see, analyze, and optimize

-- Compiled schema.json contains all SQL templates
-- So you can audit what data each API endpoint accesses
-- And verify authorization rules are correct
```text

### Real-World Consequence: Performance Optimization

```text

1. Add database index (Simplicity: you're working with the database)
2. Recompile schema (Compile-time: detect optimization opportunity)
3. API automatically uses new index (Determinism: no code changes needed)
4. Performance improves (Database-centric: optimization at source)
5. Type safety maintained throughout (Type safety: no errors introduced)
```text

---

## When These Principles Apply

### ✅ FraiseQL Is Right When

- Your data lives in a relational database
- Performance and predictability matter
- You want to minimize application code
- Your team has database expertise
- You can define your entire API schema upfront

### ❌ FraiseQL Is Wrong When

- Your primary data is in a NoSQL system
- You need to aggregate data from multiple external sources
- You need highly dynamic queries that change at runtime
- You cannot accept build-time constraints
- You need GraphQL introspection for dynamic UIs

---

## Related Topics

- **Topic 1.1:** What is FraiseQL? (benefits that flow from these principles)
- **Topic 1.2:** Core Concepts & Terminology (how principles are named)
- **Topic 1.3:** Database-Centric Architecture (how principles shape architecture)
- **Topic 1.5:** FraiseQL Compared to Other Approaches (how principles differ from alternatives)
- **Topic 2.1:** Compilation Pipeline (how Principle 2 is implemented)
- **Topic 3.1:** Python Schema Authoring (how Principle 1 guides schema design)

---

## Summary

The five design principles of FraiseQL are:

1. **Database-Centric Design** - Database is primary interface, GraphQL derives from it
2. **Compile-Time Optimization** - Move work from runtime to build time
3. **Type Safety** - Types enforced at database and API layers
4. **Performance Through Determinism** - Predictable queries enable optimization
5. **Simplicity Over Flexibility** - Assume single relational database, optimize for that

These principles work together to create a GraphQL system that is:

- **Simple:** Minimal code needed
- **Predictable:** Deterministic behavior enables optimization
- **Safe:** Type safety at every layer
- **Fast:** Compile-time optimization eliminates runtime overhead
- **Auditable:** Complete visibility into data access patterns
