# WP-028: Create Framework-Specific Migration Guides

**Assignee:** TW-CORE
**Priority:** P1 (Important)
**Estimated Hours:** 12
**Week:** 3
**Dependencies:** WP-003 (Trinity Migration Guide)

---

## Objective

Create practical migration guides for teams switching from popular Python GraphQL frameworks (Strawberry, Graphene, PostGraphile) to FraiseQL, addressing the missing documentation referenced in journey guides.

**Current State:** Documentation in `docs/journeys/backend-engineer.md:60-64` references migration guides that don't exist. The `/docs/migration/` directory does not exist.

**Target State:** Clear, actionable migration guides with code examples, schema mappings, and time estimates for each major framework.

---

## Problem Statement

**From Journey Doc Verification:**
- `docs/journeys/backend-engineer.md` references:
  - `docs/migration/from-strawberry.md`
  - `docs/migration/from-graphene.md`
  - `docs/migration/from-postgraphile.md`
- These files do not exist, breaking the Backend Engineer evaluation journey
- Backend engineers evaluating FraiseQL need concrete migration effort estimates
- Without migration guides, adoption friction is high (teams assume migration is risky)

---

## Files to Create

### Directory Structure
```
docs/migration/
├── README.md                    # Migration overview (this WP)
├── from-strawberry.md           # Strawberry → FraiseQL (this WP)
├── from-graphene.md             # Graphene → FraiseQL (this WP)
├── from-postgraphile.md         # PostGraphile → FraiseQL (this WP)
└── migration-checklist.md       # Generic checklist (this WP)
```

---

## Content Outline

### 1. `docs/migration/README.md` - Migration Hub

**Purpose:** Overview and decision tree for migration

**Content:**
```markdown
# Migrating to FraiseQL

Welcome! This guide helps you migrate from other GraphQL frameworks to FraiseQL.

## Choose Your Framework

- **[From Strawberry](from-strawberry.md)** - Pure Python, type annotations
- **[From Graphene](from-graphene.md)** - Mature Python framework
- **[From PostGraphile](from-postgraphile.md)** - Database-first (closest to FraiseQL)

## Migration Effort Estimates

| Framework | API Size | Estimated Time | Complexity |
|-----------|----------|----------------|------------|
| Strawberry | 50 resolvers | 2-3 weeks (2 engineers) | Medium |
| Graphene | 50 resolvers | 1-2 weeks (2 engineers) | Low-Medium |
| PostGraphile | 50 tables | 3-4 days (1 engineer) | Low |

## General Migration Steps

1. **Audit Current Schema** - Map all types, queries, mutations
2. **Create Database Views** - Implement trinity pattern (tb_/v_/tv_)
3. **Convert Resolvers** - Map framework-specific resolvers to FraiseQL
4. **Test Thoroughly** - Validate all queries return same results
5. **Deploy Gradually** - Blue-green deployment or feature flags

## Key Differences

| Aspect | Traditional GraphQL | FraiseQL |
|--------|---------------------|----------|
| **Schema Source** | Python classes | PostgreSQL views |
| **N+1 Queries** | Manual DataLoader | Automatic (JSONB aggregation) |
| **Type Safety** | Runtime validation | Database schema + Python types |
| **Performance** | Python JSON | Rust JSON pipeline (7-10x faster) |

## Migration Tools

- **Confiture** - Database migration tool (`pip install confiture`)
- **Schema Mapper** - Coming soon (WP-029)

## Need Help?

- Join our [Discord community](https://discord.gg/fraiseql)
- Read [Migration Checklist](migration-checklist.md)
- Contact enterprise support: contact@fraiseql.com
```

---

### 2. `docs/migration/from-strawberry.md` - Strawberry Migration Guide

**Purpose:** Detailed Strawberry → FraiseQL migration

**Content Outline:**

```markdown
# Migrating from Strawberry to FraiseQL

**Estimated Time:** 2-3 weeks for 50-resolver API (2 engineers)
**Difficulty:** Medium
**Prerequisites:** PostgreSQL database, Python 3.10+

## Why Migrate?

- **Performance:** 7-10x faster JSON serialization (Rust pipeline)
- **Database-first:** Schema reflects database reality (no drift)
- **N+1 Prevention:** Automatic via JSONB aggregation (no DataLoader needed)

## Migration Timeline

| Phase | Duration | Tasks |
|-------|----------|-------|
| 1. Audit | 2-3 days | Map all Strawberry types/resolvers |
| 2. Database | 1 week | Create tv_ views, trinity pattern |
| 3. Conversion | 1 week | Convert resolvers to FraiseQL |
| 4. Testing | 2-3 days | Validate all queries |
| 5. Deployment | 1 day | Blue-green deployment |

## Step-by-Step Guide

### Step 1: Audit Current Strawberry Schema

**Identify all components:**
```python
# Example Strawberry schema
import strawberry

@strawberry.type
class User:
    id: int
    name: str
    email: str

@strawberry.type
class Post:
    id: int
    title: str
    author: User

@strawberry.type
class Query:
    @strawberry.field
    def users(self) -> list[User]:
        # Resolver logic
        return db.query(User).all()

    @strawberry.field
    def posts(self) -> list[Post]:
        # N+1 problem: each post loads author separately
        return db.query(Post).all()
```

**Create inventory:**
- Types: `User`, `Post`
- Queries: `users`, `posts`
- Mutations: (list them)
- Custom resolvers: (list them)

### Step 2: Create Database Views (Trinity Pattern)

**Convert Strawberry types to PostgreSQL views:**

```sql
-- Base tables (probably already exist)
CREATE TABLE tb_user (
    id SERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT UNIQUE NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE tb_post (
    id SERIAL PRIMARY KEY,
    title TEXT NOT NULL,
    content TEXT,
    author_id INT REFERENCES tb_user(id),
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Simple views (v_*)
CREATE VIEW v_user AS
SELECT id, name, email, created_at FROM tb_user;

CREATE VIEW v_post AS
SELECT id, title, content, author_id, created_at FROM tb_post;

-- Computed views (tv_*) - Solves N+1 problem
CREATE VIEW tv_post_with_author AS
SELECT
    p.id,
    p.title,
    p.created_at,
    jsonb_build_object(
        '__typename', 'Post',
        'id', p.id,
        'title', p.title,
        'content', p.content,
        'author', jsonb_build_object(
            '__typename', 'User',
            'id', u.id,
            'name', u.name,
            'email', u.email
        )
    ) as data
FROM tb_post p
JOIN tb_user u ON u.id = p.author_id;
```

### Step 3: Convert Resolvers to FraiseQL

**Before (Strawberry):**
```python
@strawberry.type
class Query:
    @strawberry.field
    def users(self) -> list[User]:
        return db.query(User).all()  # Manual ORM query

    @strawberry.field
    def posts(self) -> list[Post]:
        return db.query(Post).all()  # N+1 problem here
```

**After (FraiseQL):**
```python
from fraiseql import fraise_type, query

@fraise_type
class User:
    id: int
    name: str
    email: str

@fraise_type
class Post:
    id: int
    title: str
    author: User

@query
async def users(info) -> list[User]:
    # FraiseQL automatically queries v_user
    return await info.context["db"].fetch("SELECT * FROM v_user")

@query
async def posts(info) -> list[Post]:
    # No N+1: tv_post_with_author returns nested JSONB
    return await info.context["db"].fetch("SELECT data FROM tv_post_with_author")
```

**Key Changes:**
- `@strawberry.type` → `@fraise_type`
- `@strawberry.field` → `@query` or `@mutation`
- ORM queries → Direct PostgreSQL queries (or FraiseQL query builder)
- N+1 solved by tv_ views (no DataLoader needed)

### Step 4: Mapping Table

| Strawberry Concept | FraiseQL Equivalent | Migration Action |
|--------------------|---------------------|------------------|
| `@strawberry.type` | `@fraise_type` | Direct replacement |
| `@strawberry.field` | `@query` or `@mutation` | Replace decorator |
| `@strawberry.input` | `@fraise_type` (input) | Add `input=True` param |
| DataLoader (N+1 fix) | tv_ views (JSONB) | Create views, remove DataLoader |
| SQLAlchemy ORM | PostgreSQL views | Replace ORM with views |
| Custom scalars | PostgreSQL types | Map to native types |

### Step 5: Testing

**Validate migration:**
```python
# Test query equivalence
import httpx

# Strawberry endpoint
strawberry_response = httpx.post(
    "http://old-api/graphql",
    json={"query": "{ posts { id title author { name } } }"}
)

# FraiseQL endpoint
fraiseql_response = httpx.post(
    "http://new-api/graphql",
    json={"query": "{ posts { id title author { name } } }"}
)

# Assert results match
assert strawberry_response.json() == fraiseql_response.json()
```

**Load testing:**
```bash
# Compare performance
locust -f tests/load_test.py --users 100 --spawn-rate 10 --host http://old-api
locust -f tests/load_test.py --users 100 --spawn-rate 10 --host http://new-api

# Expected: FraiseQL 7-10x faster JSON serialization
```

### Step 6: Deployment

**Blue-green deployment:**
1. Deploy FraiseQL app alongside Strawberry app
2. Route 10% traffic to FraiseQL (feature flag or load balancer)
3. Monitor error rates, latency
4. Gradually increase to 100%
5. Decommission Strawberry app

## Common Pitfalls

### 1. Forgetting N+1 Prevention
**Problem:** Direct translation of Strawberry resolvers still has N+1 problem
**Solution:** Use tv_ views with JSONB aggregation (not separate queries)

### 2. Missing Trinity Pattern
**Problem:** Querying tb_ tables directly (breaks abstraction)
**Solution:** Always query v_* or tv_* views, never tb_* tables

### 3. Custom Scalars
**Problem:** Strawberry custom scalars (e.g., DateTime, JSON) need mapping
**Solution:** Use PostgreSQL native types (TIMESTAMPTZ, JSONB)

## Performance Comparison

| Metric | Strawberry | FraiseQL | Improvement |
|--------|------------|----------|-------------|
| JSON serialization | 100 req/s | 700-1000 req/s | 7-10x |
| N+1 queries | 1 + N queries | 1 query (JSONB) | 10-100x |
| Memory usage | High (ORM overhead) | Low (direct PostgreSQL) | 50-70% reduction |

## FAQ

**Q: Can I migrate gradually (one resolver at a time)?**
A: Yes, use feature flags or proxy pattern to route queries

**Q: What about Strawberry DataLoader?**
A: Replace with tv_ views (better performance, no manual code)

**Q: Do I lose Strawberry's type safety?**
A: No, FraiseQL has similar type annotations + database schema validation

## Need Help?

- [FraiseQL Discord](https://discord.gg/fraiseql)
- [Migration Checklist](migration-checklist.md)
- Enterprise support: contact@fraiseql.com
```

---

### 3. `docs/migration/from-graphene.md` - Graphene Migration Guide

**Purpose:** Graphene → FraiseQL migration (similar structure to Strawberry guide)

**Key Differences from Strawberry:**
- Graphene uses `graphene.ObjectType` instead of `@strawberry.type`
- Graphene resolvers use `resolve_fieldname` pattern
- Migration typically faster (Graphene less opinionated)

**Content outline:** Similar structure to Strawberry guide, adapted for Graphene syntax

---

### 4. `docs/migration/from-postgraphile.md` - PostGraphile Migration Guide

**Purpose:** PostGraphile → FraiseQL migration

**Key Points:**
- **Easiest migration** - PostGraphile is already database-first
- Main task: Convert PostGraphile smart comments to FraiseQL views
- Estimated time: 3-4 days for 50 tables (1 engineer)

**Content Outline:**
```markdown
# Migrating from PostGraphile to FraiseQL

**Estimated Time:** 3-4 days for 50-table schema (1 engineer)
**Difficulty:** Low
**Prerequisites:** Existing PostgreSQL schema with PostGraphile

## Why Migrate?

- **Rust Performance:** 7-10x faster JSON (PostGraphile is Node.js-based)
- **Python Ecosystem:** Use Python libraries (ML, data science)
- **Unified Stack:** If rest of stack is Python-based

## Migration is Easy

PostGraphile and FraiseQL share the same philosophy: **database-first GraphQL**.

### Similarities
- Both generate GraphQL from PostgreSQL schema
- Both support computed columns (views)
- Both use row-level security (RLS)
- Both optimize N+1 queries automatically

### Differences
| Aspect | PostGraphile | FraiseQL |
|--------|--------------|----------|
| Runtime | Node.js | Python + Rust |
| Schema source | Tables + smart comments | Views (trinity pattern) |
| Extensibility | Plugins | Python resolvers |
| JSON performance | Fast (Node.js) | Faster (Rust 7-10x) |

## Step-by-Step Migration

### Step 1: Map PostGraphile Smart Comments to Views

**Before (PostGraphile):**
```sql
-- Smart comments define GraphQL schema
COMMENT ON TABLE users IS '@name User';
COMMENT ON COLUMN users.email IS '@omit create,update';
```

**After (FraiseQL):**
```sql
-- Use views instead of comments
CREATE VIEW v_user AS
SELECT id, name, email FROM tb_user;

-- For computed fields, use tv_ views
CREATE VIEW tv_user_with_posts AS
SELECT
    u.id,
    jsonb_build_object(
        '__typename', 'User',
        'id', u.id,
        'name', u.name,
        'posts', (
            SELECT jsonb_agg(jsonb_build_object('id', p.id, 'title', p.title))
            FROM tb_post p WHERE p.author_id = u.id
        )
    ) as data
FROM tb_user u;
```

### Step 2: Convert Custom Resolvers

**Before (PostGraphile plugin):**
```javascript
// PostGraphile plugin
module.exports = makeExtendSchemaPlugin({
  typeDefs: gql`
    extend type User {
      fullName: String
    }
  `,
  resolvers: {
    User: {
      fullName(user) {
        return `${user.firstName} ${user.lastName}`;
      }
    }
  }
});
```

**After (FraiseQL resolver):**
```python
from fraiseql import fraise_type, query

@fraise_type
class User:
    id: int
    first_name: str
    last_name: str

    @property
    def full_name(self) -> str:
        return f"{self.first_name} {self.last_name}"
```

### Step 3: Migrate RLS Policies

**Good news:** PostgreSQL RLS works the same in both frameworks!

```sql
-- No changes needed
ALTER TABLE tb_user ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON tb_user
USING (tenant_id = current_setting('app.tenant_id')::UUID);
```

FraiseQL uses PostgreSQL session variables, just like PostGraphile.

### Step 4: Performance Testing

**Benchmark Node.js vs Python+Rust:**
```bash
# PostGraphile (Node.js)
bombardier -c 100 -n 10000 http://postgraphile-api/graphql

# FraiseQL (Python + Rust)
bombardier -c 100 -n 10000 http://fraiseql-api/graphql

# Expected: FraiseQL 2-3x faster overall, 7-10x faster JSON
```

## Migration Checklist

- [ ] Map all PostGraphile tables to FraiseQL views
- [ ] Convert smart comments to view definitions
- [ ] Migrate custom plugins to Python resolvers
- [ ] Verify RLS policies work (test multi-tenancy)
- [ ] Run performance benchmarks
- [ ] Deploy blue-green

## FAQ

**Q: Can I keep my PostGraphile database schema?**
A: Yes! Just add v_* and tv_* views, keep tb_* tables unchanged

**Q: What about PostGraphile subscriptions?**
A: FraiseQL supports GraphQL subscriptions (WebSocket-based)

**Q: Do I lose PostGraphile plugins?**
A: Rewrite as Python resolvers (usually simpler in Python)

## Need Help?

- [FraiseQL Discord](https://discord.gg/fraiseql)
- Enterprise support: contact@fraiseql.com
```

---

### 5. `docs/migration/migration-checklist.md` - Generic Migration Checklist

**Purpose:** Framework-agnostic migration checklist

**Content:**
```markdown
# Migration Checklist

Use this checklist for any GraphQL framework → FraiseQL migration.

## Pre-Migration

- [ ] Audit current GraphQL schema (types, queries, mutations)
- [ ] Document custom resolvers and business logic
- [ ] Identify N+1 query patterns
- [ ] Estimate migration effort (use tables in this guide)
- [ ] Set up test environment for FraiseQL

## Database Preparation

- [ ] Rename tables to tb_* pattern (or create aliases)
- [ ] Create v_* views for basic entities
- [ ] Create tv_* views for nested/computed data
- [ ] Add indexes on filter columns (id, tenant_id, status, etc.)
- [ ] Test views return correct data

## Code Migration

- [ ] Install FraiseQL (`pip install fraiseql`)
- [ ] Convert type definitions (@fraise_type)
- [ ] Convert queries (@query)
- [ ] Convert mutations (@mutation)
- [ ] Migrate custom resolvers
- [ ] Remove N+1 fixes (DataLoader, etc.) - views handle it

## Testing

- [ ] Unit tests: All resolvers return expected data
- [ ] Integration tests: Full GraphQL queries work
- [ ] Performance tests: FraiseQL ≥ old framework (should be 2-10x faster)
- [ ] Load tests: Handle production traffic
- [ ] Regression tests: Compare old API vs new API responses

## Deployment

- [ ] Deploy FraiseQL to staging
- [ ] Run smoke tests
- [ ] Blue-green deployment to production
- [ ] Monitor error rates, latency
- [ ] Gradually shift traffic (10% → 50% → 100%)
- [ ] Decommission old API

## Post-Migration

- [ ] Update documentation
- [ ] Train team on FraiseQL patterns
- [ ] Set up monitoring (Prometheus, Grafana)
- [ ] Optimize slow queries (if any)
- [ ] Celebrate! 🎉

## Rollback Plan

If migration fails:
- [ ] Revert traffic to old API (load balancer)
- [ ] Identify root cause
- [ ] Fix issues in staging
- [ ] Retry deployment

## Time Estimates

| Framework | Small API (10-20 resolvers) | Medium API (50 resolvers) | Large API (100+ resolvers) |
|-----------|------------------------------|---------------------------|----------------------------|
| Strawberry | 3-5 days | 2-3 weeks | 4-6 weeks |
| Graphene | 2-4 days | 1-2 weeks | 3-4 weeks |
| PostGraphile | 1-2 days | 3-4 days | 1-2 weeks |

**Note:** Estimates assume 2 engineers working full-time.
```

---

## Acceptance Criteria

### Content Requirements
- ✅ All 5 files created in `/docs/migration/` directory
- ✅ Each framework guide includes step-by-step instructions
- ✅ Code examples provided for common migration scenarios
- ✅ Time estimates documented (validated by manual testing)
- ✅ Migration checklist comprehensive (covers all phases)

### Quality Requirements
- ✅ Code examples tested (no syntax errors)
- ✅ SQL examples tested (run on PostgreSQL)
- ✅ Links to related documentation work
- ✅ Consistent style across all guides
- ✅ Reviewed by backend engineer persona (WP-024)

### User Experience Requirements
- ✅ Backend engineer can estimate migration effort in <20 minutes
- ✅ Clear decision tree (which guide to follow)
- ✅ Realistic time estimates (not marketing claims)
- ✅ Common pitfalls documented (prevent mistakes)

---

## Testing Plan

### Manual Testing
1. **Strawberry Migration Test:** Follow guide with sample Strawberry app, measure actual time
2. **Graphene Migration Test:** Follow guide with sample Graphene app, measure actual time
3. **PostGraphile Migration Test:** Follow guide with sample PostGraphile schema, measure actual time

### Validation
- Backend engineer persona review (WP-024): Can they complete migration using guide?
- Time estimates validated: Actual migration time within ±20% of documented estimate
- Code examples validated: All code runs without errors (WP-021)

---

## DO NOT

- ❌ Do not promise "zero downtime" (unrealistic for most teams)
- ❌ Do not underestimate migration effort (damages credibility)
- ❌ Do not skip rollback plan (teams need safety net)
- ❌ Do not assume users know trinity pattern (link to WP-003)
- ❌ Do not provide only happy path (document pitfalls)

---

## Success Metrics

### Technical
- All migration guides tested with sample apps (validate time estimates)
- Code examples run successfully (zero syntax errors)
- Backend engineer journey step now works (no broken links)

### User Experience
- Backend engineer can make migration decision in <30 minutes
- Migration effort clearly communicated (no surprises)
- Confidence in FraiseQL adoption increased (documented migration path)

---

## Related Work Packages

- **WP-003:** Trinity Migration Guide (prerequisite understanding)
- **WP-004:** Backend Engineer Journey (fixes broken migration guide links)
- **WP-021:** Code Validation (validates all code examples in migration guides)
- **WP-024:** Persona Reviews (backend engineer tests migration guides)

---

## Notes

**Why This Matters:**
- Missing migration guides are a **major adoption blocker**
- Backend engineers won't recommend FraiseQL without clear migration path
- Time estimates are critical for planning (engineering managers need this)
- Competing frameworks (Hasura, Postgraphile) have migration guides

**Alternatives Considered:**
1. Single generic migration guide → Too vague, not actionable
2. Only document PostGraphile (easiest) → Ignores Strawberry/Graphene users
3. External blog posts only → Not discoverable, not maintained

**Decision:** Create framework-specific guides (this WP)

---

**End of WP-028**
