# FraiseQL

[![Quality Gate](https://github.com/fraiseql/fraiseql/actions/workflows/quality-gate.yml/badge.svg?branch=dev)](https://github.com/fraiseql/fraiseql/actions/workflows/quality-gate.yml)
[![Documentation](https://github.com/fraiseql/fraiseql/actions/workflows/docs.yml/badge.svg)](https://github.com/fraiseql/fraiseql/actions/workflows/docs.yml)
[![Release](https://img.shields.io/github/v/release/fraiseql/fraiseql)](https://github.com/fraiseql/fraiseql/releases/latest)
[![Python](https://img.shields.io/badge/Python-3.13+-blue.svg)](https://www.python.org/downloads/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Version Status](https://img.shields.io/badge/Status-Stable-brightgreen.svg)](https://github.com/fraiseql/fraiseql/blob/main/dev/audits/version-status.md)

**📍 You are here: Main FraiseQL Framework (v1.8.5) - Stable Release**

**Current Version**: v1.8.5 | **Status**: Stable | **Python**: 3.13+ | **PostgreSQL**: 13+

---

## **The Fastest GraphQL Framework That's Still a Joy to Use**

**0.83ms latency on standard cloud hardware. Sub-millisecond. Proven.**

FraiseQL is the only GraphQL framework in Tier 1 performance (< 2ms) that maintains a developer-friendly Python API.

```python
# Complete GraphQL API in ~15 lines
from fraiseql import type, query
from fraiseql.fastapi import create_fraiseql_app

@fraiseql.type(sql_source="v_user", jsonb_column="data")
class User:
    id: int
    name: str
    email: str

@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("v_user")

app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[User],
    queries=[users]
)
```

---

## 🏆 Unique Value Proposition

**Rust Performance + Python Productivity**

### Performance That Matters

| Metric | FraiseQL | Strawberry | Graphene | Hasura |
|--------|----------|-----------|----------|--------|
| Single Row Latency | **0.83ms** | ~45ms | ~50ms | 64-824ms |
| 100 Rows Latency | **2.59ms** | 2,500 RPS plateau | 1,200 RPS plateau | Variable |
| Complex Nested | **0.70ms** | Resolver overhead | Resolver overhead | Variable |
| Scaling Pattern | **Linear** | Plateau | Plateau | Non-deterministic |

**42-991x faster than alternatives.** Measured on standard AWS t3.large (2 vCPU, 8GB RAM).

### Exclusive Rust Pipeline

Only GraphQL framework with Rust-based JSON transformation:

```
Traditional:  PostgreSQL → ORM → Python JSON → Response (slow!)
FraiseQL:     PostgreSQL → Rust → Response (7-10x faster)
```

**Why this works:**
- PostgreSQL returns pre-composed JSONB (already structured data)
- Rust selects fields based on GraphQL query (compiled, fast)
- Zero Python serialization overhead in hot path
- Direct HTTP response (zero-copy)

### Database-First Architecture

**No N+1 queries. Ever.**

PostgreSQL views compose data once. Rust pipeline respects GraphQL selection. Single query per request.

```sql
-- PostgreSQL defines what's exposed
CREATE VIEW v_user AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'email', email,
        'posts', (SELECT jsonb_agg(...) FROM posts)  -- Nested!
    ) as data
FROM users;
```

```python
# Python type mirrors exact view structure
@fraiseql.type(sql_source="v_user", jsonb_column="data")
class User:
    id: int
    email: str
    posts: list[Post]  # No resolver, no N+1!
```

---

## 🎯 What Makes FraiseQL Different

### Three Core Advantages

**1. Performance Without Compromise**
- Sub-millisecond single queries (0.83ms proven)
- Linear scaling to 1000+ rows (not exponential)
- P99 latency only 1.7x average (excellent under load)
- No performance cliff on large result sets

**2. Security by Architecture**
- Explicit field contracts prevent data leaks
- JSONB views define max recursion depth (no middleware needed)
- Fixed data structure = impossible to over-fetch
- PostgreSQL RLS integration
- Clear audit trail by design

**3. Developer Experience**
- Type-safe Python API
- No new query languages to learn
- SQL functions are readable business logic
- AI-friendly (Claude/Copilot generate correct code first try)
- Clear CQRS pattern (reads vs writes)

### Comparison with Major Frameworks

| Aspect | FraiseQL | Strawberry | Graphene | Hasura | PostGraphile |
|--------|----------|-----------|----------|--------|--------------|
| Latency | ✅ **0.83ms** | ⚠️ 45ms | ⚠️ 50ms | ❌ 64-824ms | ⚠️ 30-50ms |
| Scaling | ✅ Linear | ❌ Plateau | ❌ Plateau | ⚠️ Variable | ⚠️ Variable |
| N+1 Protection | ✅ Built-in | ⚠️ DataLoader | ⚠️ DataLoader | ✅ Built-in | ✅ Built-in |
| Security | ✅ Explicit | ⚠️ ORM risk | ⚠️ ORM risk | ⚠️ Complex | ⚠️ Plugin-based |
| AI-Friendly | ✅ Yes | ⚠️ Partial | ⚠️ Partial | ⚠️ Config-driven | ⚠️ Config-driven |
| Python | ✅ Native | ✅ Native | ✅ Native | ❌ Haskell | ⚠️ JavaScript |

---

## ⚡ Why Choose FraiseQL?

### For Performance Teams
```
Problem: "Our GraphQL API is slow"
Solution: FraiseQL delivers 0.83ms latency
Compared to: Strawberry (54x slower), Hasura (77-991x slower)
```

### For Python Teams
```
Problem: "We want speed without learning new languages"
Solution: Type-safe Python API + Rust pipeline
Benefit: 45-60x faster than pure Python alternatives
```

### For Startups
```
Problem: "We can't afford Redis, Sentry, APM tools"
Solution: Everything in PostgreSQL
Savings: $5,400-48,000 per year vs traditional stack
```

### For Enterprise
```
Problem: "We need security, scaling, observability"
Solution: Built into architecture (not bolted on)
Result: Explicit contracts, linear scaling, audit trails
```

---

## 🔒 Security by Design (Not Configuration)

**Traditional ORM Problem:**
```python
# Developer forgets to exclude sensitive fields
class User(Base):
    password_hash = Column(String)  # Exposed!
    api_key = Column(String)        # Exposed!
```

**FraiseQL's Explicit Approach:**
```sql
-- PostgreSQL view defines exactly what's exposed
CREATE VIEW v_user AS
SELECT jsonb_build_object(
    'id', id,
    'email', email
    -- password_hash, api_key NOT INCLUDED
) as data;
```

**Recursion Depth Protection:**
- No infinite depth attacks possible
- View structure defines maximum recursion
- No middleware needed, no bypasses

**Field-Level Authorization:**
```python
@fraiseql.authorized(roles=["admin", "editor"])
@fraiseql.mutation
class DeletePost:
    """Only admins can delete."""
```

**Row-Level Security:**
- PostgreSQL RLS integrates directly
- Multi-tenant isolation built-in
- Cryptographic audit logging (SHA-256 + HMAC)

---

## 💰 $5-48K Annual Savings

Replace 4 services with PostgreSQL:

| Service | Cost | FraiseQL | Savings |
|---------|------|----------|---------|
| Redis Cache | $50-500/mo | ✅ In PostgreSQL | $600-6,000/yr |
| Sentry (Error) | $300-3,000/mo | ✅ In PostgreSQL | $3,600-36,000/yr |
| APM Tool | $100-500/mo | ✅ In PostgreSQL | $1,200-6,000/yr |
| **Total** | **$450-4,000/mo** | **$50/mo** | **$5,400-48,000/yr** |

**How it works:**
- Built-in error tracking with stack traces
- OpenTelemetry tracing to PostgreSQL
- Query performance monitoring
- Grafana dashboards included

---

## 🎯 Target Users

### ✅ Perfect For

- **PostgreSQL-first teams** already using JSONB extensively
- **Performance-critical APIs** (FinTech, real-time analytics, trading)
- **Python developers** who want best-in-class performance
- **Cost-conscious startups** ($5-48K savings)
- **AI-assisted development** teams (Claude, Copilot, ChatGPT)
- **High-traffic applications** requiring linear scaling
- **Self-hosted infrastructure** (full control, no vendor lock-in)

### ❌ Consider Alternatives

- Need multi-database support (FraiseQL is PostgreSQL-only)
- Building your first API (simpler frameworks available)
- Don't use JSONB columns
- Require Apollo Federation (coming in roadmap)

---

## ✨ Key Features

### Advanced Type System
- 50+ specialized scalar types (Money, IPv4, LTree, etc.)
- Full-text search, JSONB queries, array operations, regex
- Vector search via pgvector (semantic search, RAG, recommendations)

### Enterprise-Grade
- OpenTelemetry tracing with sensitive data sanitization
- Software Bill of Materials (SBOM) generation
- HashiCorp Vault, AWS KMS, GCP Cloud KMS integration
- Compliance: FedRAMP, HIPAA, PCI-DSS, SOC 2, NIS2

### Developer Experience
- **Automatic Persisted Queries (APQ)** - bandwidth optimization
- **GraphQL Cascade** - automatic cache updates and side effects
- **Auto-populated mutations** - status, message, errors handled (50-60% less code)
- **Auto-wired query params** - `where`, `orderBy`, `limit`, `offset` automatic
- **Trinity Identifiers** - pk_* (internal), id (API), identifier (human-readable)

### CQRS Pattern Built-In
- **Reads:** `v_*` views (real-time, JSONB-composed)
- **Reads:** `tv_*` tables (denormalized, explicitly synced)
- **Writes:** `fn_*` functions (business logic, validation)

---

## 📖 Complete CRUD Example

```python
from uuid import UUID
from fraiseql import type, query, mutation, input, success
from fraiseql.fastapi import create_fraiseql_app

# Step 1: Map view to type
@fraiseql.type(sql_source="v_note", jsonb_column="data")
class Note:
    id: UUID
    title: str
    content: str | None

# Step 2: Queries
@fraiseql.query
async def notes(info) -> list[Note]:
    db = info.context["db"]
    return await db.find("v_note")

@fraiseql.query
async def note(info, id: UUID) -> Note | None:
    db = info.context["db"]
    return await db.find_one("v_note", id=id)

# Step 3: Mutations
@input
class CreateNoteInput:
    title: str
    content: str | None = None

@fraiseql.mutation
class CreateNote:
    input: CreateNoteInput
    success: Note

# Step 4: App
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[Note],
    queries=[notes, note],
    mutations=[CreateNote]
)
```

**That's everything.** No resolvers, no N+1 queries, no performance cliffs.

---

## 🚀 Get Started in 5 Minutes

```bash
# Install
pip install fraiseql

# Create project
fraiseql init my-api
cd my-api

# Setup database
createdb my_api
psql my_api < schema.sql

# Start server
fraiseql dev
```

**Your GraphQL API is live at http://localhost:8000/graphql**

### Next Steps

- **[First Hour Guide](docs/getting-started/first-hour.md)** - Build a complete blog API (60 minutes)
- **[Understanding FraiseQL](docs/guides/understanding-fraiseql.md)** - Architecture deep dive (10 minutes)
- **[Performance Benchmarks](docs/performance-testing/MEDIUM_VPS_BENCHMARKS.md)** - Detailed measurements
- **[Full Documentation](docs/)** - Complete guides and references

### Prerequisites

- **Python 3.13+** (required for Rust pipeline)
- **PostgreSQL 13+**

---

## 📚 Learn More

- **[Documentation](https://fraiseql.dev)** - Complete guides and API reference
- **[Performance Benchmarks](docs/performance-testing/MEDIUM_VPS_BENCHMARKS.md)** - 0.83ms proven
- **[Competitive Positioning](docs/COMPETITIVE_POSITIONING.md)** - How FraiseQL compares
- **[Examples](examples/)** - Real-world applications
- **[Architecture Decisions](docs/architecture/)** - Design trade-offs

---

## 🏗️ Architecture

```
GraphQL Request
    ↓
Python Resolver (in FastAPI)
    ↓
PostgreSQL View/Function (returns JSONB)
    ↓
Rust Pipeline (field selection, compiled fast)
    ↓
HTTP Response (zero Python overhead)
```

**Why this is fast:**
1. PostgreSQL composes data once (no N+1)
2. Rust selects fields (compiled, no GIL)
3. Direct HTTP response (no Python serialization)

**Why this is secure:**
1. Views define what's exposed (explicit contracts)
2. Rust respects GraphQL schema (no over-fetching)
3. Fixed recursion depth (no depth bombs)

**Why this is simple:**
1. SQL functions = business logic (readable, auditable)
2. Python decorators = schema definition (not boilerplate)
3. CQRS pattern = clear separation (reads vs writes)

---

## 🛠️ CLI Commands

```bash
# Project management
fraiseql init <name>           # Create new project
fraiseql dev                   # Development server
fraiseql check                 # Validate schema

# Code generation
fraiseql generate schema       # Export GraphQL schema
fraiseql generate types        # Generate TypeScript definitions

# Database utilities
fraiseql sql analyze <query>   # Analyze query performance
fraiseql sql explain <query>   # Show execution plan
```

---

## 🤝 Contributing

We welcome contributions! See [CONTRIBUTING.md](CONTRIBUTING.md) for:

- Development setup and testing
- Architecture decisions
- Code style and review

**Quick start:**
```bash
git clone https://github.com/fraiseql/fraiseql
cd fraiseql && make setup-dev
```

**Pre-commit with prek (7-10x faster than pre-commit):**
```bash
brew install j178/tap/prek  # macOS
prek install                # Setup hooks
prek run --all              # Run before commit
```

---

## 📄 License

MIT License - see [LICENSE](LICENSE) for details.

---

## 👨‍💻 About

FraiseQL was created by **Lionel Hamayon** ([@evoludigit](https://github.com/evoludigit)), founder of [Évolution digitale](https://evolution-digitale.fr).

**Started: April 2025**

### The Origin Story

I built FraiseQL to fix a fundamental inefficiency: **PostgreSQL returns JSON → Python deserializes → GraphQL serializes → Response.**

Why the roundtrip?

After years with Django, Flask, FastAPI, and Strawberry GraphQL, I realized the approach was backwards. Let PostgreSQL return the final JSON. Skip the ORM. Skip object mapping. Use a compiled language (Rust) for the fast path.

Also, I wanted a framework built for the LLM era. SQL and Python are massively trained—LLMs understand them natively. Clear contracts, explicit logic, full context visible.

FraiseQL is the result:
- **Database-first CQRS** (PostgreSQL does what it does best)
- **Rust pipeline** (7-10x faster than Python JSON)
- **Python stays minimal** (decorators + type hints)
- **LLM-readable by design** (clear contracts, explicit logic)

**Connect:**
- 💼 GitHub: [@evoludigit](https://github.com/evoludigit)
- 📧 lionel.hamayon@evolution-digitale.fr
- 🏢 [Évolution digitale](https://evolution-digitale.fr)

**Support FraiseQL:**
- ⭐ Star [fraiseql/fraiseql](https://github.com/fraiseql/fraiseql)
- 💬 Join discussions
- 🤝 Contribute

---

**Ready to build the fastest GraphQL API in Python?**

```bash
pip install fraiseql && fraiseql init my-api
```

🚀 **PostgreSQL → Rust → Production**
