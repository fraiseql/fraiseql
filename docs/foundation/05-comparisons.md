# 1.5: FraiseQL Compared to Other Approaches

**Audience:** Technical decision-makers, architects evaluating GraphQL solutions
**Prerequisite:** Topics 1.1 (What is FraiseQL?), 1.2 (Core Concepts), 1.4 (Design Principles)
**Reading Time:** 20-25 minutes

---

## Overview

FraiseQL is one of several approaches to building GraphQL APIs. This topic compares FraiseQL with popular alternatives to help you understand where each approach excels and where it makes tradeoffs.

**Key Question:** Which approach is right for your project?

The answer depends on:

- Your data source (relational database vs. mixed sources)
- Your team's expertise (database, backend, frontend)
- Your performance requirements (predictability vs. flexibility)
- Your development speed priorities (time-to-market vs. long-term maintainability)

---

## Comparison Matrix: At a Glance

| Aspect | FraiseQL | Apollo Server | Hasura | WunderGraph | Custom REST |
|--------|----------|---------------|--------|-------------|-------------|
| **Primary Data Source** | Single relational DB | Multiple sources | PostgreSQL | Multiple sources | Anything |
| **Schema Definition** | Python/TypeScript code | GraphQL schema language | PostgreSQL schema | Multiple languages | Not applicable |
| **Compilation** | Build-time | Runtime | Runtime | Runtime | N/A |
| **Resolver Code** | Automatic (SQL) | Manual custom code | Automatic rules | Automatic + manual | Manual code |
| **Type Safety** | Database + GraphQL | GraphQL only | PostgreSQL only | GraphQL + custom validation | Code-level only |
| **Performance** | Predictable | Variable (resolver dependent) | Variable (rule based) | Moderate (middleware overhead) | Variable |
| **Flexibility** | Limited to DB schema | Very high | Limited to DB + rules | High | Complete |
| **Time to API** | Fast (schema → SQL) | Slow (code resolvers) | Fast (introspection) | Moderate | Slow |
| **Learning Curve** | Medium (GraphQL + SQL) | High (GraphQL + resolver patterns) | Low (just SQL) | Moderate | N/A |
| **Best For** | Deterministic OLTP APIs | Complex multi-source APIs | Quick PostgreSQL APIs | Flexible API gateway | Simple services |

---

## Detailed Comparisons

### FraiseQL vs. Apollo Server

**Apollo Server** is the most popular GraphQL framework. It's flexible, well-documented, and the industry standard.

#### What Apollo Server Excels At

#### Flexibility

```graphql
type Query {
  user(id: Int!): User
  trendingUsers: [User!]!
  searchUsers(query: String!): [User!]!
  recommendations(userId: Int!): [Recommendation!]!
}
```text

Each field can resolve from different sources:

- Database query
- REST API call
- Cache lookup
- Computed value
- File system

### Multi-Source Integration

```typescript
// Apollo: Combine data from multiple sources
const resolvers = {
  Query: {
    user: async (_, { id }, context) => {
      const user = await context.db.query('SELECT * FROM users WHERE id = ?', [id]);
      const profile = await context.externalAPI.getProfile(id);
      const recommendations = await context.ml.getRecommendations(id);
      return { ...user, profile, recommendations };
    }
  }
};
```text

### Ecosystem & Plugins

- Apollo Server Extensions (authentication, logging, monitoring)
- Data loader (N+1 prevention)
- Apollo Federation (schema stitching)
- Hundreds of community plugins

#### Where Apollo Server Struggles

#### Resolver Complexity

```typescript
// Apollo: Every field needs a resolver
const resolvers = {
  Query: {
    user: (_, { id }, context) => context.db.findUser(id),
  },
  User: {
    id: (user) => user.id,
    email: (user) => user.email,
    orders: (user, _, context) => context.db.findOrders(user.id),  // N+1 problem?
  },
  Order: {
    id: (order) => order.id,
    total: (order) => order.total,
    items: (order, _, context) => context.db.findItems(order.id),  // Another N+1?
  }
};
```text

### Manual Optimization

```typescript
// Apollo: You must implement optimization patterns
const dataLoaders = {
  userLoader: new DataLoader(async (userIds) => {
    return context.db.query('SELECT * FROM users WHERE id = ANY(?)', [userIds]);
  }),
};
```text

### Performance Unpredictability

- Query performance depends on resolver implementation
- N+1 problems can hide until production
- No compile-time visibility into query costs
- Hard to debug performance issues

### Synchronizing Schemas

```text
TypeScript type definitions
       ↔️ (must match)
GraphQL schema
       ↔️ (must match)
Database schema
```text

If you change the database, you must update two more places.

#### FraiseQL vs. Apollo: Decision

| Your Priority | Better Choice | Why |
|---------------|---------------|-----|
| **Single-source API from relational database** | FraiseQL | Simpler, faster, more predictable |
| **Multi-source data aggregation** | Apollo Server | FraiseQL doesn't support this well |
| **Complex custom business logic in resolvers** | Apollo Server | FraiseQL executes at database layer |
| **Time-to-market with existing REST APIs** | Apollo Server | Easier to wrap external services |
| **Performance predictability** | FraiseQL | Compile-time analysis guarantees |
| **Team has database expertise** | FraiseQL | Database knowledge directly applies |
| **Team has JavaScript expertise** | Apollo Server | Lower learning curve |

---

### FraiseQL vs. Hasura

**Hasura** automatically generates a GraphQL API by introspecting your PostgreSQL schema.

#### What Hasura Excels At

#### Fast Time to API

```bash
# Hasura: Point at database, get GraphQL API instantly
docker run hasura/graphql-engine:latest \
  --database-url postgresql://user:pass@db:5432/mydb
```text

Result: Complete GraphQL API with CRUD operations, relationships, and filtering—without writing a line of code.

### Database-First Approach

```sql
CREATE TABLE users (
  id SERIAL PRIMARY KEY,
  email VARCHAR(255) NOT NULL UNIQUE,
  created_at TIMESTAMP DEFAULT NOW()
);

CREATE TABLE orders (
  id SERIAL PRIMARY KEY,
  user_id INTEGER REFERENCES users(id),
  total DECIMAL(10, 2)
);
```text

Hasura immediately exposes:

```graphql
type User {
  id: Int!
  email: String!
  createdAt: DateTime!
  orders: [Order!]!
}

type Order {
  id: Int!
  user: User!
  total: Float!
}
```text

### Permission Rules

```yaml
# Hasura: Row-level security via rules
Users:
  select:
    columns:
      - id
      - email
    filter:
      id: { _eq: X-Hasura-User-Id }
```text

### Simplicity for Standard CRUD

```graphql
query {
  users {
    id
    email
    orders {
      id
      total
    }
  }
}
# Hasura handles the SQL automatically
```text

#### Where Hasura Struggles

#### Fixed Query Patterns

```graphql
# Hasura: No custom computed fields without Actions
query {
  user(id: 1) {
    id
    email
    orders {
      id
      total
      items {        # ✅ Can do this
        name
      }
    }
    orderCount       # ❌ Requires custom Action (REST API call)
  }
}
```text

### Action-Based Extensions

```yaml
# Hasura: Must implement custom logic via Actions
actions:
  - name: searchUsers
    definition:
      kind: query
      arguments:
        query: string!
      output_type: SearchResult
      handler: https://api.example.com/search
```text

This converts back to the multi-source problem (like Apollo).

### Runtime Performance

- No compile-time query analysis
- Permission checks at runtime
- No optimization of permission rules
- Can cause N+1 queries if permissions are complex

### Schema Coupling

```text
PostgreSQL schema ←→ GraphQL schema (1:1 mapping)
```text

Change the database table name, and the GraphQL API changes. This can break clients.

#### FraiseQL vs. Hasura: Decision

| Your Priority | Better Choice | Why |
|---------------|---------------|-----|
| **Time to basic CRUD API** | Hasura | Introspection is instant |
| **Standard database queries** | Hasura | Zero code needed |
| **Custom computed fields** | FraiseQL | Explicit schema control |
| **Query performance visibility** | FraiseQL | Compile-time analysis |
| **Schema versioning/evolution** | FraiseQL | Explicit schema lets you version |
| **Team only knows SQL** | Hasura | No need for Python/TypeScript |
| **Production SLA requirements** | FraiseQL | Predictable performance |
| **Rapid prototyping** | Hasura | Get API in minutes |

---

### FraiseQL vs. WunderGraph

**WunderGraph** is a newer framework that positions itself as a "serverless GraphQL federation platform." It supports multiple data sources and aims for developer productivity.

#### What WunderGraph Excels At

#### Configuration-First Development

```yaml
# WunderGraph: Configure data sources and relationships
dataSources:
  - name: database
    kind: postgresql
    database_url: ${DATABASE_URL}
  - name: external_api
    kind: graphql
    url: https://api.example.com/graphql
```text

### Flexible Data Source Support

- Relational databases (PostgreSQL, MySQL, MongoDB)
- GraphQL APIs
- REST APIs
- Custom operations

### Built-in Authentication

```yaml
# WunderGraph: Auth integrated
authentication:
  providers:
    - github
    - auth0
    - custom_webhook
```text

### Federation Support

```typescript
// WunderGraph: Compose multiple GraphQL APIs
import { introspectAndCompose } from '@wundergraph/SDK';

export default {
  apis: [
    introspectAndCompose({
      apiNamespace: 'users',
      url: 'http://users-service/graphql',
    }),
    introspectAndCompose({
      apiNamespace: 'products',
      url: 'http://products-service/graphql',
    }),
  ],
};
```text

#### Where WunderGraph Struggles

#### Still Manual for Complex Queries

```typescript
// WunderGraph: You write resolvers for complex operations
export default async function GetUserRecommendations(
  ctx: Context,
  input: GetUserRecommendationsInput,
) {
  const user = await ctx.user.findOne({ id: input.id });
  const recommendations = await ctx.ml.getRecommendations(user.id);
  // Still writing custom code
  return recommendations;
}
```text

### Middle-Ground Positioning

- Not as fast as Hasura (requires more code)
- Not as predictable as FraiseQL (runtime interpretation)
- Not as flexible as Apollo (no custom middleware)
- Good at everything, expert at nothing

### Performance Still Variable

```typescript
// WunderGraph: You're still responsible for optimization
// No compile-time guarantees about query efficiency
export default async function GetUserWithOrders(
  ctx: Context,
  input: GetUserWithOrdersInput,
) {
  const user = await ctx.db.users.findOne({ id: input.id }); // 1 query
  const orders = await ctx.db.orders.findMany({ userId: user.id }); // 1 query
  // What if orders has 10,000 items? Pagination? Filtering?
  // You have to handle this manually
  return { user, orders };
}
```text

#### FraiseQL vs. WunderGraph: Decision

| Your Priority | Better Choice | Why |
|---------------|---------------|-----|
| **Single relational database** | FraiseQL | More predictable, simpler |
| **Multiple data sources** | WunderGraph | Explicit multi-source support |
| **Quick API for single service** | Hasura | Faster than both |
| **Complex business logic** | Apollo Server | More mature ecosystem |
| **Performance predictability** | FraiseQL | Compile-time guarantees |
| **Team learning curve** | WunderGraph | Mid-point between options |

---

### FraiseQL vs. Custom REST APIs

Before GraphQL was popular, teams built custom REST APIs. This is still the baseline to compare against.

#### What Custom REST Excels At

#### Simplicity for Simple Services

```python
# REST: Simple to understand
@app.get("/users/{user_id}")
def get_user(user_id: int):
    return db.query("SELECT * FROM users WHERE id = ?", [user_id])
```text

### Familiarity

- Every developer knows REST
- No GraphQL learning curve
- Mature tooling and libraries

### Fine-Grained Control

```python
# REST: You control exactly what goes into each endpoint
@app.get("/users/{user_id}/recommendations")
def get_recommendations(user_id: int, limit: int = 10):
    # Your logic: exactly what you need, nothing more
    return db.query(
        "SELECT * FROM recommendations WHERE user_id = ? LIMIT ?",
        [user_id, limit]
    )
```text

#### Where Custom REST Struggles

#### Versioning Chaos

```text
/api/v1/users/{id}
/api/v2/users/{id}
/api/v3/users/{id}
```text

Each API version requires separate endpoints and testing.

### Over-fetching & Under-fetching

```text
REST API returns:
GET /api/users/1
{
  "id": 1,
  "email": "user@example.com",
  "name": "John",
  "phone": "123-456-7890",    // You don't need this
  "address": { ... }          // Or this
}

Returned 500 bytes, needed 200 bytes
```text

Or:

```text
You need user + orders + order items
3 separate requests: GET /users/1, GET /users/1/orders, GET /orders/123/items
```text

### No Standard Query Language

```text
Custom filtering:
GET /api/users?filter=email:contains:@example.com&sort=-created_at&limit=10

Different service:
GET /api/products?q=coffee&sort=price&page=1&per_page=20

Inconsistent APIs everywhere
```text

### Documentation Burden

```text
Each endpoint needs separate documentation:

- GET /users/{id}
- GET /users/{id}/orders
- GET /users/{id}/recommendations
- POST /users
- PUT /users/{id}
- DELETE /users/{id}
- GET /users?search=...&limit=...&offset=...

And that's just for users. Multiply by 20 resources = 100s of endpoints
```text

#### FraiseQL vs. REST: Decision

| Your Priority | Better Choice | Why |
|---------------|---------------|-----|
| **Simple CRUD service** | Custom REST | Less overhead |
| **Mobile API with bandwidth concerns** | FraiseQL | Query-specific fields only |
| **Multi-use API (web + mobile + partners)** | FraiseQL | Single flexible API |
| **Team knows REST already** | Custom REST | No GraphQL learning needed |
| **Long-term API evolution** | FraiseQL | Single versioning story |
| **Fast development** | REST or Hasura | Pre-built patterns |

---

## FraiseQL's Unique Position

### What FraiseQL Brings

#### 1. Compile-Time Guarantees

```text
Query performance, authorization, and schema correctness all verified at build time,
not discovered in production.
```text

### 2. Database Expertise as API Design

```text
Your database team's work (indexes, views, optimization) directly
improves API performance. No resolver code to optimize.
```text

### 3. Deterministic Behavior

```text
Every query's performance is predictable and reproducible.
No "sometimes slow" queries. No hidden N+1 problems.
```text

### 4. Minimal Code

```text
No custom resolvers. No data loaders. No optimization patterns.
Just schema definitions and SQL automatically generated.
```text

### What FraiseQL Trades Off

#### Single Data Source

```text
Cannot easily aggregate data from multiple external APIs.
Best for monolithic database-centric services.
```text

### No Custom Resolver Logic

```text
Complex business logic must happen in the database (functions/views)
or in separate services.
Cannot add computed fields easily without database changes.
```text

### Build-Time Schema

```text
All queries must be known at compile time.
Dynamic queries require recompilation.
```text

### PostgreSQL-First

```text
Primary focus on PostgreSQL. MySQL/SQLite/SQL Server support,
but PostgreSQL gets features first.
```text

---

## Decision Framework: Choosing Your Approach

### If You Answer "YES" to Most of These → Use FraiseQL

- [ ] Your primary data is in a relational database (PostgreSQL, MySQL, SQLite, SQL Server)
- [ ] You want predictable, deterministic query performance
- [ ] Your team has database expertise
- [ ] Performance visibility and compile-time verification matter to you
- [ ] Your data relationships are well-defined (not highly dynamic)
- [ ] You want minimal application code (no custom resolvers)
- [ ] You can define your entire API schema upfront

### If You Answer "YES" to Most of These → Use Hasura

- [ ] You want to launch a GraphQL API as quickly as possible
- [ ] Your database schema is already well-designed
- [ ] Standard CRUD operations cover 80% of your use cases
- [ ] You're using PostgreSQL
- [ ] Simple permission rules are sufficient

### If You Answer "YES" to Most of These → Use Apollo Server

- [ ] You need to aggregate data from multiple sources
- [ ] You need complex custom resolver logic
- [ ] Your team has strong JavaScript/TypeScript expertise
- [ ] Flexibility is more important than performance predictability
- [ ] You're building an API gateway or federation platform

### If You Answer "YES" to Most of These → Use Custom REST

- [ ] This is a simple, single-purpose service
- [ ] You don't need a flexible query language
- [ ] Your team prefers REST familiarity
- [ ] Simplicity matters more than advanced features

---

## Real-World Examples

### Example 1: E-Commerce Platform

#### Requirements:

- Product catalog with search, filtering, recommendations
- Orders with items and order history
- User profiles and permissions
- Shopping cart state

### Best Choice: FraiseQL

Why:

- Well-defined schema (products, orders, users, cart)
- Deterministic queries (catalog, recommendations)
- Performance is critical (search must be fast)
- Clear data relationships
- Database team can optimize indexes independently

### API would include:

```python
@schema.type(table="v_products")
class Product:
    id: int
    name: str
    price: float
    rating: float

@schema.query()
def product_search(query: str, limit: int = 10) -> List[Product]:
    pass

@schema.query()
def user_recommendations(user_id: int) -> List[Product]:
    pass
```text

### Example 2: Multi-Tenant SaaS Dashboard

#### Requirements:

- Multiple data sources (main DB, analytics DB, external services)
- Complex permission rules (tenant isolation, role-based)
- Custom computed fields (user's total spend, team metrics)
- Real-time updates via WebSocket

### Best Choice: Apollo Server

Why:

- Multiple data sources (can't consolidate into one DB)
- Custom business logic needed (computations, complex auth)
- Flexibility more important than performance predictability
- Mature ecosystem for SaaS patterns

### Example 3: Rapid Internal Tool

#### Requirements:

- Quick GraphQL API over existing PostgreSQL database
- Standard CRUD operations
- Simple permission rules
- Time to launch: 1 week

### Best Choice: Hasura

Why:

- Time to launch is critical
- Schema is already defined (existing database)
- Standard operations are sufficient
- Zero code = faster development

### Example 4: Mobile App Backend

#### Requirements:

- Minimize bandwidth (mobile networks)
- Fetch exactly the fields needed
- Consistent schema across multiple client versions
- Performance matters (cellular networks)

### Best Choice: FraiseQL or Apollo Server

Why:

- GraphQL eliminates over-fetching (good for mobile)
- FraiseQL for predictable performance
- Apollo Server for complex aggregation (if needed)

---

## Summary

| Situation | Best Choice | Runner-Up |
|-----------|-------------|-----------|
| **Single relational DB, performance critical** | FraiseQL | Hasura |
| **Multiple data sources, complex logic** | Apollo Server | WunderGraph |
| **Rapid API for existing PostgreSQL** | Hasura | FraiseQL |
| **Flexible federation of services** | WunderGraph | Apollo Server |
| **Simple CRUD service** | Custom REST | Hasura |
| **Mobile app backend** | FraiseQL | Apollo Server |
| **Data platform / analytics** | FraiseQL | Custom REST |
| **Complex business logic, monolith** | Apollo Server | FraiseQL |

---

## Related Topics

- **Topic 1.1:** What is FraiseQL? (benefits that differentiate FraiseQL)
- **Topic 1.4:** Design Principles (why FraiseQL makes these tradeoffs)
- **Topic 2.1:** Compilation Pipeline (how FraiseQL enables compile-time optimization)
- **Topic 4.1:** Schema Design Best Practices (database design patterns for FraiseQL)
- **Topic 5.1:** Performance Optimization (how to get the most from FraiseQL)

---

## Conclusion

FraiseQL is not the right tool for every job. It excels when:

1. **Your data is in a relational database** (primary source)
2. **You want deterministic performance** (compile-time guarantees)
3. **Your team values database expertise** (schema design knowledge)
4. **You prefer simplicity over flexibility** (minimal code)

If your use case matches these criteria, FraiseQL will give you a fast, predictable, auditable GraphQL API with minimal code. If you need multi-source aggregation or extreme flexibility, other tools (Apollo Server, WunderGraph) may be better choices.

The key insight: **Different tools for different jobs. Choose based on your actual constraints, not hype.**
