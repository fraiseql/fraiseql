# FraiseQL v1 - Documentation Plan

**Goal**: Create documentation that impresses technical leaders and demonstrates architectural mastery

**Audience**:
- Senior/Staff/Principal Engineers (interview target)
- Tech leads evaluating frameworks
- Developers wanting sub-1ms GraphQL

---

## 📐 Documentation Principles

### 1. **Philosophy Before Implementation**
Start every doc with WHY, then WHAT, then HOW
- Why does this problem matter?
- What is our unique approach?
- How do you use it?

### 2. **Visual First**
Every architecture doc needs:
- ASCII diagrams for copy-paste
- Mermaid diagrams for GitHub rendering
- Performance charts

### 3. **Code-Heavy**
Every concept = working example
- No abstract explanations
- Show actual SQL + Python
- Benchmark numbers included

### 4. **Progressive Complexity**
- Quickstart: 5 minutes to hello world
- Guides: 15 minutes to understand pattern
- Advanced: Deep dives for experts

---

## 📚 Documentation Structure

```
docs/
├── README.md                     [500 lines]
├── philosophy/                   [4 docs, ~1000 lines total]
│   ├── WHY_FRAISEQL.md
│   ├── CQRS_FIRST.md
│   ├── RUST_ACCELERATION.md
│   └── CODEGEN_VISION.md
├── architecture/                 [5 docs, ~1500 lines total]
│   ├── OVERVIEW.md
│   ├── NAMING_CONVENTIONS.md
│   ├── COMMAND_QUERY_SEPARATION.md
│   ├── SYNC_STRATEGIES.md
│   └── RUST_INTEGRATION.md
├── guides/                       [6 docs, ~2000 lines total]
│   ├── QUICK_START.md
│   ├── DATABASE_SETUP.md
│   ├── WRITING_QUERIES.md
│   ├── WRITING_MUTATIONS.md
│   ├── TYPE_SYSTEM.md
│   └── PERFORMANCE.md
├── api/                          [4 docs, ~1500 lines total]
│   ├── DECORATORS.md
│   ├── REPOSITORY.md
│   ├── SYNC_FUNCTIONS.md
│   └── CLI.md
└── examples/                     [3 docs, ~2000 lines total]
    ├── BASIC_BLOG.md
    ├── ECOMMERCE_API.md
    └── SAAS_MULTI_TENANT.md
```

**Total**: ~8,500 lines of high-quality documentation

---

## 📄 Individual Document Specs

### **docs/README.md** [500 lines]

**Purpose**: Project overview, first impression

**Structure**:
```markdown
# FraiseQL - The Fastest Python GraphQL Framework

[Animated GIF of sub-1ms query demo]

## Performance First

| Framework | Query Time | Architecture | CQRS |
|-----------|-----------|-------------|------|
| **FraiseQL** | **0.6ms** | Rust + PostgreSQL | ✅ Built-in |
| Strawberry | 24ms | Python + DataLoader | ❌ Manual |
| Graphene | 45ms | Python + ORM | ❌ Manual |

**40x faster** than traditional Python GraphQL frameworks.

## Why FraiseQL?

GraphQL is slow in Python. We fixed it.

[3 bullet points on the solution]

## Quick Start

```python
[50-line working example]
```

## Philosophy

[Link to philosophy docs with 1-sentence summaries]

## Installation

```bash
pip install fraiseql
```

## Features

- 🚀 **Sub-1ms queries** with Rust acceleration
- 🏗️ **CQRS built-in** - Command/Query separation
- 📊 **PostgreSQL-optimized** - JSONB native support
- 🎯 **Zero N+1 queries** - Database-level joins
- 🔧 **CLI code generation** - Scaffold from DB schema

## Learn More

[Links to key docs]

## Built With FraiseQL

[3 showcase apps with screenshots]

## License

MIT
```

**Key Elements**:
- Performance numbers above the fold
- Working code in first 500 pixels
- Visual demo (GIF or screenshot)
- Links to deeper docs

---

### **philosophy/WHY_FRAISEQL.md** [300 lines]

**Purpose**: Explain the problem and our unique solution

**Structure**:
```markdown
# Why FraiseQL Exists

## The Problem

GraphQL in Python is **slow**.

### Typical GraphQL Request Flow

```
┌─────────────────────────────────────────────┐
│ 1. Parse GraphQL (Python)           ~5ms   │
│ 2. Resolve root query (Python)      ~10ms  │
│ 3. N+1 queries to database          ~30ms  │
│ 4. Transform DB → GraphQL (Python)  ~10ms  │
│ 5. Serialize response (Python)      ~5ms   │
├─────────────────────────────────────────────┤
│ TOTAL                                60ms   │
└─────────────────────────────────────────────┘
```

**Why it's slow**:
1. N+1 query problem
2. Python object creation overhead
3. ORM abstraction layers
4. No caching at right level

### Industry Workarounds

| Solution | Complexity | Performance Gain |
|----------|------------|------------------|
| DataLoader | High (manual batching) | 2-3x |
| GraphQL-JIT | Medium (complexity) | 2x |
| Database caching | Low (stale data issues) | Variable |

**None solve the fundamental problem.**

---

## The FraiseQL Solution

### Principle 1: CQRS at Database Level

Move complexity to where it belongs: **PostgreSQL**.

```sql
-- Command side: Normalized writes
CREATE TABLE tb_user (...);
CREATE TABLE tb_post (...);

-- Query side: Denormalized reads
CREATE TABLE tv_user (
    id UUID PRIMARY KEY,
    data JSONB NOT NULL  -- Pre-joined, ready for GraphQL
);
```

**Result**: Single query, no N+1, pre-computed joins.

### Principle 2: Rust for Transformation

The only remaining Python overhead: JSON transformation.

**Solution**: Rust does it 40x faster.

```python
# This happens in Rust, not Python
result = transform_json(
    db_result,    # snake_case JSONB
    schema,       # GraphQL schema
    selection     # Requested fields
)
# → Returns camelCase JSON in 0.5ms
```

### Principle 3: Explicit Over Magic

No hidden DataLoaders, no query builders, no magic.

```python
@mutation
async def create_user(info, name: str) -> User:
    # 1. Write to command side (explicit)
    user_id = await db.execute("INSERT INTO tb_user ...")

    # 2. Sync to query side (explicit)
    await sync_tv_user(db, user_id)

    # 3. Return from query side
    return await query_repo.find_one("tv_user", user_id)
```

**You always know what's happening.**

---

## The Result

| Metric | Traditional | FraiseQL | Improvement |
|--------|------------|----------|-------------|
| Query latency | 60ms | 0.6ms | **100x faster** |
| N+1 queries | Common | Impossible | **Architectural** |
| Code clarity | DataLoaders everywhere | Single query | **10x simpler** |
| Scalability | Vertical (more Python) | Horizontal (more Postgres) | **Cost effective** |

---

## When to Use FraiseQL

**Perfect for**:
- APIs with > 100 req/s
- Complex object graphs
- Real-time requirements (< 10ms)
- PostgreSQL users
- Teams that value explicitness

**Not ideal for**:
- MySQL/SQLite (needs JSONB)
- Simple CRUD (overkill)
- Microservices (use REST)

---

## Next Steps

- [CQRS First](./CQRS_FIRST.md) - Deep dive on CQRS pattern
- [Rust Acceleration](./RUST_ACCELERATION.md) - How Rust integration works
- [Quick Start](../guides/QUICK_START.md) - Build your first API
```

**Key Elements**:
- Clear problem statement with numbers
- Visual diagrams of architecture
- Comparison table
- When NOT to use (honest assessment)

---

### **philosophy/CQRS_FIRST.md** [400 lines]

**Purpose**: Deep dive on CQRS pattern and why it's foundational

**Outline**:
1. **What is CQRS?** (50 lines)
   - Command Query Responsibility Segregation
   - Diagram: Command side vs Query side

2. **CQRS in Traditional Apps** (100 lines)
   - Application-level CQRS
   - Why it's complex
   - Code example showing DataLoaders

3. **CQRS in FraiseQL** (150 lines)
   - Database-level CQRS
   - tb_* vs tv_* tables
   - SQL examples
   - Sync strategies

4. **Trade-offs** (50 lines)
   - Write amplification
   - Storage overhead
   - When it's worth it

5. **Advanced Patterns** (50 lines)
   - Materialized views (eventual consistency)
   - Real-time sync (immediate consistency)
   - Hybrid approaches

---

### **architecture/NAMING_CONVENTIONS.md** [200 lines]

**Purpose**: Clear, unambiguous naming across the codebase

**Structure**:
```markdown
# Naming Conventions

## Database Objects

### Tables (Command Side)

```
tb_*    Tables (normalized, source of truth)

Examples:
- tb_user
- tb_post
- tb_comment
- tb_post_tag (junction table)
```

**Rules**:
- Singular noun (tb_user, not tb_users)
- Snake case
- No abbreviations (tb_organization, not tb_org)

### Views (Query Side)

```
tv_*    Table views (denormalized, JSONB, synced from tb_*)

Examples:
- tv_user       (user with embedded posts)
- tv_post       (post with author + comments)
- tv_dashboard  (complex aggregations)
```

**Rules**:
- Singular noun
- Must have JSONB `data` column
- Must have `id` column (indexed)
- Must have `updated_at` for cache invalidation

### SQL Functions

```
fn_sync_tv_*    Sync functions (update tv_* from tb_*)
fn_*            Other functions (business logic)

Examples:
- fn_sync_tv_user(p_user_id UUID)
- fn_sync_tv_post(p_post_id UUID)
- fn_calculate_user_stats(p_user_id UUID)
```

### Indexes

```
idx_*           Regular indexes
idx_*_gin       GIN indexes (for JSONB)
idx_*_unique    Unique indexes

Examples:
- idx_tv_user_email
- idx_tv_post_data_gin
- idx_tb_user_email_unique
```

---

## Python Objects

### Types

```python
@type
class User:  # PascalCase, singular
    id: UUID
    first_name: str  # snake_case
    userPosts: list[Post]  # camelCase for GraphQL
```

**Rules**:
- Class name: PascalCase, singular
- Fields: snake_case in Python, camelCase in GraphQL (auto-converted)

### Repositories

```python
CommandRepository  # For writes
QueryRepository    # For reads
```

### Functions

```python
async def sync_tv_user()  # snake_case
async def find_one()      # snake_case
```

---

## GraphQL Schema

### Types

```graphql
type User {         # PascalCase, singular
  id: UUID!
  firstName: String # camelCase
  userPosts: [Post!]!
}
```

### Queries

```graphql
type Query {
  user(id: UUID!): User              # singular for single
  users(limit: Int): [User!]!        # plural for list
  usersByRole(role: String!): [User!]!
}
```

### Mutations

```graphql
type Mutation {
  createUser(input: CreateUserInput!): User!    # create*
  updateUser(input: UpdateUserInput!): User!    # update*
  deleteUser(id: UUID!): DeleteResult!          # delete*
}
```

---

## File Structure

```
src/fraiseql/
├── types/              # lowercase, plural
│   ├── user.py        # lowercase, singular
│   └── post.py
├── repositories/
│   ├── command.py     # lowercase
│   └── query.py
└── ...
```

---

## Examples

### Blog Post with CQRS

**Command side**:
```sql
CREATE TABLE tb_user (...);
CREATE TABLE tb_post (...);
```

**Query side**:
```sql
CREATE TABLE tv_post (
    id UUID PRIMARY KEY,
    data JSONB NOT NULL
);
```

**Sync function**:
```sql
CREATE FUNCTION fn_sync_tv_post(p_post_id UUID) ...
```

**Python**:
```python
@type
class Post:
    id: UUID
    title: str
    author: User
```
```

---

### **guides/QUICK_START.md** [400 lines]

**Purpose**: Get from zero to working API in 5 minutes

**Structure**:
```markdown
# Quick Start

Get a working GraphQL API in **5 minutes**.

## Prerequisites

- Python 3.11+
- PostgreSQL 14+ with JSONB support
- 5 minutes

---

## Step 1: Install

```bash
pip install fraiseql
```

---

## Step 2: Database Setup

```sql
-- Command side (tables)
CREATE TABLE tb_user (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    email TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Query side (view)
CREATE TABLE tv_user (
    id UUID PRIMARY KEY,
    data JSONB NOT NULL,
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Sync function
CREATE OR REPLACE FUNCTION fn_sync_tv_user(p_user_id UUID)
RETURNS void AS $$
BEGIN
    INSERT INTO tv_user (id, data, updated_at)
    SELECT
        id,
        jsonb_build_object(
            'id', id::text,
            'name', name,
            'email', email,
            'createdAt', created_at
        ),
        NOW()
    FROM tb_user
    WHERE id = p_user_id
    ON CONFLICT (id) DO UPDATE
    SET data = EXCLUDED.data, updated_at = NOW();
END;
$$ LANGUAGE plpgsql;
```

---

## Step 3: Define Types

```python
# app.py
from fraiseql import FraiseQL, type, query, mutation
from uuid import UUID
from datetime import datetime

@type
class User:
    id: UUID
    name: str
    email: str
    created_at: datetime
```

---

## Step 4: Write Queries

```python
from fraiseql.repositories import QueryRepository

@query
async def user(info, id: UUID) -> User:
    """Get user by ID"""
    repo = QueryRepository(info.context["db"])
    return await repo.find_one("tv_user", id=id)

@query
async def users(info, limit: int = 10) -> list[User]:
    """List users"""
    repo = QueryRepository(info.context["db"])
    return await repo.find("tv_user", limit=limit)
```

---

## Step 5: Write Mutations

```python
from fraiseql.repositories import CommandRepository, sync_tv_user

@mutation
async def create_user(info, name: str, email: str) -> User:
    """Create a new user"""
    db = info.context["db"]

    # 1. Write to command side
    user_id = await db.fetchval(
        "INSERT INTO tb_user (name, email) VALUES ($1, $2) RETURNING id",
        name, email
    )

    # 2. Sync to query side (explicit!)
    await sync_tv_user(db, user_id)

    # 3. Return from query side
    repo = QueryRepository(db)
    return await repo.find_one("tv_user", id=user_id)
```

---

## Step 6: Create App

```python
from fastapi import FastAPI
from fraiseql.fastapi import create_app

app = FastAPI()
fraiseql_app = create_app(
    db_url="postgresql://user:pass@localhost/db"
)
app.include_router(fraiseql_app)
```

---

## Step 7: Run

```bash
uvicorn app:app --reload
```

Visit http://localhost:8000/graphql

---

## Step 8: Test Query

```graphql
query {
  user(id: "...") {
    id
    name
    email
    createdAt
  }
}
```

**Response time**: ~0.6ms ⚡

---

## Step 9: Test Mutation

```graphql
mutation {
  createUser(name: "Alice", email: "alice@example.com") {
    id
    name
  }
}
```

---

## What Just Happened?

1. **Command side** (`tb_user`) stores normalized data
2. **Query side** (`tv_user`) stores denormalized JSONB
3. **Sync function** keeps them in sync
4. **Rust transformer** converts snake_case → camelCase
5. **Result**: Sub-1ms queries with no N+1

---

## Next Steps

- [Database Setup](./DATABASE_SETUP.md) - Advanced schema patterns
- [Writing Queries](./WRITING_QUERIES.md) - Filters, pagination, etc.
- [Writing Mutations](./WRITING_MUTATIONS.md) - Transactions, validation
- [Performance Guide](./PERFORMANCE.md) - Benchmarking, optimization

---

## Troubleshooting

**Q: I get "table tv_user does not exist"**
A: Run the SQL from Step 2

**Q: Query returns null**
A: Did you call `sync_tv_user()` after INSERT?

**Q: How do I add relationships?**
A: See [Database Setup](./DATABASE_SETUP.md)
```

---

## 🎯 Documentation Writing Strategy

### Week 1: Philosophy Docs (4 docs)
Write the "why" before the "how":
1. WHY_FRAISEQL.md
2. CQRS_FIRST.md
3. RUST_ACCELERATION.md
4. CODEGEN_VISION.md

**Goal**: Convince reader FraiseQL is worth learning

### Week 2: Architecture Docs (5 docs)
Explain the system:
1. OVERVIEW.md (with diagrams!)
2. NAMING_CONVENTIONS.md
3. COMMAND_QUERY_SEPARATION.md
4. SYNC_STRATEGIES.md
5. RUST_INTEGRATION.md

**Goal**: Reader understands how it works

### Week 3: Guides (6 docs)
Enable the reader:
1. QUICK_START.md (5 min to working API)
2. DATABASE_SETUP.md
3. WRITING_QUERIES.md
4. WRITING_MUTATIONS.md
5. TYPE_SYSTEM.md
6. PERFORMANCE.md

**Goal**: Reader can build production APIs

### Week 4: API Reference & Examples
Complete the picture:
1. DECORATORS.md
2. REPOSITORY.md
3. SYNC_FUNCTIONS.md
4. CLI.md (future)
5. BASIC_BLOG.md
6. ECOMMERCE_API.md
7. SAAS_MULTI_TENANT.md

**Goal**: Reader has all reference material

---

## 📊 Quality Metrics

### Per Document:
- [ ] Clear h1 title
- [ ] TL;DR at top
- [ ] At least 1 diagram
- [ ] At least 3 code examples
- [ ] "Next Steps" links
- [ ] Runs through Grammarly (no typos)
- [ ] Reviewed by ChatGPT (clarity check)

### Overall:
- [ ] Every doc links to related docs
- [ ] No broken internal links
- [ ] Consistent terminology
- [ ] Progressive complexity (simple → advanced)
- [ ] All code examples tested

---

## 🚀 Priority Order

1. **CRITICAL** (Do first):
   - README.md
   - WHY_FRAISEQL.md
   - QUICK_START.md

2. **HIGH** (Do second):
   - CQRS_FIRST.md
   - NAMING_CONVENTIONS.md
   - WRITING_QUERIES.md
   - WRITING_MUTATIONS.md

3. **MEDIUM** (Do third):
   - All architecture docs
   - TYPE_SYSTEM.md
   - PERFORMANCE.md

4. **LOW** (Do last):
   - API reference docs
   - Advanced examples

---

## Next Action

Start with the **Philosophy Trilogy**:
1. WHY_FRAISEQL.md - The problem
2. CQRS_FIRST.md - The solution (CQRS)
3. RUST_ACCELERATION.md - The speed (Rust)

These three docs will be your "interview talk track" - memorize them!
