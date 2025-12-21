# FraiseQL

[![Quality Gate](https://github.com/fraiseql/fraiseql/actions/workflows/quality-gate.yml/badge.svg?branch=dev)](https://github.com/fraiseql/fraiseql/actions/workflows/quality-gate.yml)
[![Documentation](https://github.com/fraiseql/fraiseql/actions/workflows/docs.yml/badge.svg)](https://github.com/fraiseql/fraiseql/actions/workflows/docs.yml)
[![Release](https://img.shields.io/github/v/release/fraiseql/fraiseql)](https://github.com/fraiseql/fraiseql/releases/latest)
[![Python](https://img.shields.io/badge/Python-3.13+-blue.svg)](https://www.python.org/downloads/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

**v1.9.0** | **Stable** | **Rust-Powered GraphQL for PostgreSQL**

---

## GraphQL for the LLM era. Simple. Powerful. Rust-fast.

PostgreSQL returns JSONB. Rust transforms it. Zero Python overhead.

```python
# Complete GraphQL API in 15 lines
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

## Why FraiseQL?

- ⚡ **Rust Pipeline** - 7-10x faster JSON transformation, zero Python overhead
- 🔒 **Secure by Design** - Explicit field contracts prevent data leaks
- 🤖 **AI-Native** - LLMs generate correct code on first try
- 💰 **Save $5-48K/year** - Eliminate Redis, Sentry, APM tools
- 🔄 **GraphQL Cascade** - Automatic cache updates on mutations
- ✨ **50% Less Boilerplate** - Auto-populated mutations, auto-wired query params
- 🧠 **Vector Search** - pgvector integration for semantic search & RAG
- 📋 **GraphQL Compliant** - 85-90% GraphQL spec with advanced features

---

## Is This For You?

**✅ Perfect if you:**
- Build high-performance APIs with PostgreSQL
- Want 7-10x faster JSON processing
- Need enterprise security & compliance
- Prefer database-first architecture
- Use LLMs for code generation

**❌ Consider alternatives if:**
- You need multi-database support (PostgreSQL-only)
- Building your first GraphQL API (use simpler frameworks)
- Don't use JSONB columns in PostgreSQL

---

## How It Works

**Traditional GraphQL** (slow):
```
PostgreSQL → Rows → ORM deserialize → Python objects → GraphQL serialize → JSON → Response
            ╰─── Unnecessary roundtrips (2 conversions) ───╯
```

**FraiseQL** (fast):
```
PostgreSQL → JSONB → Rust field selection → HTTP Response
           ╰─ Zero Python overhead (1 conversion) ─╯
```

### Why This Is Better

1. **No ORM Overhead** - Database returns final JSONB, Rust transforms it
2. **No N+1 Queries** - PostgreSQL composes everything in one query
3. **Security Built-In** - View defines exactly what's exposed (impossible to leak)
4. **Recursion Safe** - View structure prevents depth attacks naturally
5. **AI-Friendly** - SQL + Python are massively trained; no magic frameworks

---

## Quick Start

```bash
pip install fraiseql
fraiseql init my-api
cd my-api
fraiseql dev
```

**Your GraphQL API is live at `http://localhost:8000/graphql`** 🎉

**Next steps:**
- [5-Minute Quickstart](docs/getting-started/quickstart.md)
- [First Hour Guide](docs/getting-started/first-hour.md) - Build a complete blog API
- [Understanding FraiseQL](docs/guides/understanding-fraiseql.md) - Architecture deep-dive

---

## Real Security, Not Theatre

### The Problem (ORM-based frameworks)

```python
class User(Base):  # SQLAlchemy
    id = Column(Integer)
    email = Column(String)
    password_hash = Column(String)  # ← Sensitive!
    api_key = Column(String)        # ← Sensitive!

@strawberry.type
class UserType:
    id: int
    email: str
    # Forgot to exclude password_hash and api_key!
```

**Result:** One mistake = data leak.

### The Solution (FraiseQL)

```sql
-- PostgreSQL view defines what's exposed
CREATE VIEW v_user AS
SELECT id,
  jsonb_build_object('id', id, 'email', email) as data
FROM tb_user;
-- password_hash and api_key aren't in JSONB = impossible to leak
```

**Result:** Structure defines the contract. No way to accidentally expose fields.

---

## Chaos Engineering & Resilience Testing

FraiseQL separates testing into two workflows:

| Aspect | Standard CI/CD | Chaos Engineering |
|--------|---|---|
| **Duration** | 15-20 min | 45-60 min |
| **Purpose** | Correctness | Resilience |
| **Trigger** | Every PR | Manual/Weekly |
| **Tests** | Unit + Integration | 71 chaos scenarios |
| **Blocks Merges** | Yes ✅ | No (informational) |
| **Environment** | Lightweight | Real PostgreSQL + Docker |

**Standard CI/CD:** Validates that features work correctly
**Chaos Tests:** Validates that system recovers from failures

[→ Learn about chaos engineering strategy](docs/testing/chaos-engineering-strategy.md)

---

## Advanced Features

### Specialized Type System (50+ scalar types)

```python
from fraiseql.types import EmailAddress, PhoneNumber, IPv4, Money, LTree

@fraiseql.type(sql_source="v_users")
class User:
    email: EmailAddress      # Validated emails
    phone: PhoneNumber       # International phone numbers
    ip: IPv4                 # IP addresses with subnet operations
    balance: Money           # Currency with precision
    location: LTree          # Hierarchical paths
```

### Trinity Identifiers

Three ID types for different purposes:
- **pk_user** (int): Internal DB key, not exposed
- **id** (UUID): Public API, stable, never changes
- **identifier** (str): Human-readable slug, SEO-friendly

### GraphQL Cascade

Automatic cache invalidation when mutations change related data:

```graphql
mutation {
  createPost(input: {...}) {
    post { id title }
    cascade {
      updated { __typename }     # What changed
      invalidations { queryName } # Which queries to invalidate
    }
  }
}
```

---

## Enterprise Security Features

- **KMS Integration:** Vault, AWS KMS, GCP Cloud KMS
- **Security Profiles:** STANDARD, REGULATED, RESTRICTED (government-grade)
- **SBOM Generation:** Automated compliance (FedRAMP, NIS2, HIPAA, PCI-DSS)
- **Audit Logging:** Cryptographic chain (SHA-256 + HMAC)
- **Row-Level Security:** PostgreSQL RLS integration
- **Rate Limiting:** Per-endpoint and per-GraphQL operation

[🔐 Security Configuration](docs/production/security.md)

---

## Cost Savings: Replace 4 Services with 1 Database

| Service | Cost | FraiseQL Approach | Savings |
|---------|------|------------------|---------|
| Redis (caching) | $50-500/mo | PostgreSQL UNLOGGED tables | $600-6,000/yr |
| Sentry (error tracking) | $300-3,000/mo | PostgreSQL error logging | $3,600-36,000/yr |
| APM Tool | $100-500/mo | PostgreSQL traces | $1,200-6,000/yr |
| **Total** | **$450-4,000/mo** | **PostgreSQL only ($50/mo)** | **$5,400-48,000/yr** |

All built-in to PostgreSQL. One database to backup.

---

## Code Examples

### Complete CRUD API

```python
from fraiseql import type, query, mutation, input, success

@fraiseql.type(sql_source="v_note", jsonb_column="data")
class Note:
    id: int
    title: str
    content: str | None

@fraiseql.query
async def notes(info) -> list[Note]:
    return await info.context["db"].find("v_note")

@input
class CreateNoteInput:
    title: str
    content: str | None = None

@fraiseql.mutation
class CreateNote:
    input: CreateNoteInput
    success: Note

app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[Note],
    queries=[notes],
    mutations=[CreateNote]
)
```

### Database-First Pattern

```sql
-- PostgreSQL view (composable, no N+1)
CREATE VIEW v_user AS
SELECT id,
  jsonb_build_object(
    'id', id,
    'name', name,
    'email', email,
    'posts', (
      SELECT jsonb_agg(...)
      FROM tb_post p
      WHERE p.user_id = tb_user.id
    )
  ) as data
FROM tb_user;
```

```python
# Python type mirrors the view
@fraiseql.type(sql_source="v_user", jsonb_column="data")
class User:
    id: int
    name: str
    email: str
    posts: list[Post]  # Nested! No N+1 queries!
```

---

## Learn More

- **[Full Documentation](https://github.com/fraiseql/fraiseql/tree/main/docs)** - Comprehensive guides
- **[Architecture Decisions](https://github.com/fraiseql/fraiseql/tree/main/docs/architecture)** - Why we built it this way
- **[Performance Guide](https://github.com/fraiseql/fraiseql/blob/main/docs/performance/index.md)** - Optimization strategies
- **[Examples](https://github.com/fraiseql/fraiseql/tree/main/examples)** - Real-world applications

---

## Contributing

```bash
git clone https://github.com/fraiseql/fraiseql
cd fraiseql && make setup-dev
prek install  # 7-10x faster than pre-commit
```

[→ Contributing Guide](CONTRIBUTING.md)

---

## About

FraiseQL is created by **Lionel Hamayon** ([@evoludigit](https://github.com/evoludigit)).

**The Idea:** What if PostgreSQL returned JSON directly instead of Python serializing it? No ORM. No N+1 queries. No Python overhead. Just Rust transforming JSONB to HTTP.

**The Result:** A GraphQL framework that's 7-10x faster and designed for the LLM era.

---

## License

MIT License - see [LICENSE](LICENSE)

---

**Ready to build efficient GraphQL APIs?**

```bash
pip install fraiseql && fraiseql init my-api
```

🚀 **PostgreSQL → Rust → Production**
