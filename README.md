# FraiseQL

[![Quality Gate](https://github.com/fraiseql/fraiseql/actions/workflows/quality-gate.yml/badge.svg?branch=dev)](https://github.com/fraiseql/fraiseql/actions/workflows/quality-gate.yml)
[![Documentation](https://github.com/fraiseql/fraiseql/actions/workflows/docs.yml/badge.svg)](https://github.com/fraiseql/fraiseql/actions/workflows/docs.yml)
[![Release](https://img.shields.io/github/v/release/fraiseql/fraiseql)](https://github.com/fraiseql/fraiseql/releases/latest)
[![Python](https://img.shields.io/badge/Python-3.13+-blue.svg)](https://www.python.org/downloads/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

**The fastest Python GraphQL framework. In PostgreSQL Everything.**

Pre-compiled queries, Automatic Persisted Queries (APQ), PostgreSQL-native caching, error tracking, and observability—all in one database.

> **4-100x faster** than traditional GraphQL frameworks • **In PostgreSQL Everything** • **$300-3,000/month savings** • **Zero external dependencies**

## 🚀 Why FraiseQL?

### **🏛️ In PostgreSQL Everything**
**One database to rule them all.** FraiseQL eliminates external dependencies by implementing caching, error tracking, and observability directly in PostgreSQL.

**Cost Savings:**
```
Traditional Stack:
- Sentry: $300-3,000/month
- Redis Cloud: $50-500/month
- Total: $350-3,500/month

FraiseQL Stack:
- PostgreSQL: Already running (no additional cost)
- Total: $0/month additional
```

**Operational Simplicity:**
```
Before: FastAPI + PostgreSQL + Redis + Sentry + Grafana = 5 services
After:  FastAPI + PostgreSQL + Grafana = 3 services
```

**PostgreSQL-Native Stack:**
- **Caching**: UNLOGGED tables (Redis-level performance, no WAL overhead)
- **Error Tracking**: Automatic fingerprinting, grouping, notifications (like Sentry)
- **Observability**: OpenTelemetry traces + metrics in PostgreSQL
- **Monitoring**: Grafana dashboards querying PostgreSQL directly

### **⚡ Blazing Fast Performance**
- **Automatic Persisted Queries (APQ)**: SHA-256 hash lookup with pluggable storage backends
- **Memory & PostgreSQL storage**: In-memory for simplicity, PostgreSQL for enterprise scale
- **JSON passthrough optimization**: Sub-millisecond cached responses (0.5-2ms)
- **Pre-compiled queries**: TurboRouter with intelligent caching (4-10x faster)
- **Real production benchmarks**: 85-95% cache hit rate

### **🏗️ Database-First Architecture**
- **CQRS by design**: Commands via PostgreSQL functions, queries via views
- **JSONB-powered**: Flexible schema evolution with full type safety
- **View-based queries**: `v_*` for real-time, `tv_*` for materialized performance
- **PostgreSQL does the heavy lifting**: Joins, aggregations, transformations in-database

### **🔧 Developer Experience**
- **Type-safe**: Full Python 3.13+ type hints with automatic GraphQL schema generation
- **Automatic documentation**: Python docstrings become GraphQL descriptions in Apollo Studio
- **One command setup**: `fraiseql init my-api && fraiseql dev`
- **Intelligent WHERE clauses**: Automatic type-aware SQL optimization for network types, dates, and more
- **Hybrid table support**: Seamless filtering across regular columns and JSONB fields
- **Built-in security**: Field-level authorization, rate limiting, CSRF protection

## 🏁 Quick Start

```bash
# Install and create project
pip install fraiseql
fraiseql init my-api && cd my-api

# Define your types
cat > src/types.py << 'EOF'
import fraiseql
from fraiseql import ID, EmailAddress

@fraiseql.type
class User:
    """A user account with authentication and profile information."""
    id: ID
    email: EmailAddress
    name: str
    created_at: str
EOF

# Create database view (returns JSONB)
cat > db/001_user_view.sql << 'EOF'
CREATE VIEW v_user AS
SELECT jsonb_build_object(
    'id', pk_user,
    'email', email,
    'name', name,
    'created_at', created_at::text
) AS data FROM tb_users;
EOF

# Define queries
cat > src/queries.py << 'EOF'
import fraiseql
from .types import User

@fraiseql.query
async def users(info) -> list[User]:
    """Get all users with their profile information."""
    repo = info.context["repo"]
    return await repo.find("tv_user", "users", info)
EOF

# Start development server
fraiseql dev
```

Your GraphQL API is live at `http://localhost:8000/graphql` 🎉

## 🔄 Automatic Persisted Queries (APQ)

FraiseQL provides enterprise-grade APQ support with pluggable storage backends:

### **Storage Backends**
```python
# Memory backend (default - zero configuration)
config = FraiseQLConfig(
    apq_storage_backend="memory"  # Perfect for development & simple apps
)

# PostgreSQL backend (enterprise scale)
config = FraiseQLConfig(
    apq_storage_backend="postgresql",  # Persistent, multi-instance ready
    apq_storage_schema="apq_cache"     # Custom schema for isolation
)
```

### **How APQ Works**
1. **Client sends query hash** instead of full query
2. **FraiseQL checks storage backend** for cached query
3. **JSON passthrough optimization** returns results in 0.5-2ms
4. **Fallback to normal execution** if query not found

### **Enterprise Benefits**
- **99.9% cache hit rates** in production applications
- **70% bandwidth reduction** with large queries
- **Multi-instance coordination** with PostgreSQL backend
- **Automatic cache warming** for frequently used queries

## 🎯 Core Features

### **Advanced Type System**
Specialized operators for network types, hierarchical data, and ranges:

```graphql
query {
  servers(where: {
    ipAddress: { eq: "192.168.1.1" }        # → ::inet casting
    port: { gt: 1024 }                      # → ::integer casting
    macAddress: { eq: "aa:bb:cc:dd:ee:ff" } # → ::macaddr casting
    location: { ancestor_of: "US.CA" }      # → ltree operations
    dateRange: { overlaps: "[2024-01-01,2024-12-31)" }
  }) {
    id name ipAddress port
  }
}
```

**Unified Rust-First Execution**
All queries follow the same high-performance path:
```
PostgreSQL → Rust → HTTP (0.5-5ms response time)
```

- **Always Fast**: No mode detection or branching logic
- **Field Projection**: Rust filters JSON fields 10-50x faster than PostgreSQL
- **Zero Python Overhead**: Direct RustResponseBytes to FastAPI

**Supported specialized types:**
- **Network**: `IPv4`, `IPv6`, `CIDR`, `MACAddress` with subnet/range operations
- **Hierarchical**: `LTree` with ancestor/descendant queries
- **Temporal**: `DateRange` with overlap/containment operations
- **Standard**: `EmailAddress`, `UUID`, `JSON` with validation

### **Intelligent Mutations**
PostgreSQL functions handle business logic with structured error handling:

```python
@fraiseql.input
class CreateUserInput:
    name: str
    email: EmailAddress

class CreateUserSuccess:
    user: User
    message: str = "User created successfully"

class CreateUserError:
    message: str
    error_code: str

class CreateUser(
    FraiseQLMutation,
    function="fn_create_user",  # PostgreSQL function
    validation_strict=True
):
    input: CreateUserInput
    success: CreateUserSuccess
    failure: CreateUserError
```

### **Multi-Tenant Architecture**
Built-in tenant isolation with per-tenant caching:

```python
# Automatic tenant context
@fraiseql.query
async def users(info) -> list[User]:
    repo = info.context["repo"]
    tenant_id = info.context["tenant_id"]  # Auto-injected
    return await repo.find("v_user", tenant_id=tenant_id)
```

### **Transform Tables (tv_*)**
Pre-computed JSONB tables for instant GraphQL responses:

```sql
-- Transform table (actually a TABLE, not a view!)
CREATE TABLE tv_user (
    id INT PRIMARY KEY,
    data JSONB GENERATED ALWAYS AS (
        jsonb_build_object(
            'id', id,
            'first_name', (SELECT first_name FROM tb_user WHERE tb_user.id = tv_user.id),
            'user_posts', (SELECT jsonb_agg(...) FROM tb_post WHERE user_id = tv_user.id LIMIT 10)
        )
    ) STORED
);
```

```python
# Type definition
@fraiseql.type(sql_source="tv_user", jsonb_column="data")
class User:
    id: int
    first_name: str      # Rust transforms to firstName
    user_posts: list[Post]  # Embedded relations!

# Query (0.05ms lookup + 0.5ms Rust transform)
@fraiseql.query
async def user(info, id: int) -> User:
    repo = info.context["repo"]
    return await repo.find("tv_user", "user", info, id=id)
```

**Benefits:**
- **0.55ms total response time** (100-200x faster than JOINs)
- **Embedded relations** (no N+1 queries)
- **Always up-to-date** (generated columns + triggers)
- **Rust field projection** (10-50x faster than PostgreSQL)

## 📊 Performance Comparison

### Framework Comparison
| Framework | Simple Query | Complex Query | Cache Hit | APQ Support |
|-----------|-------------|---------------|-----------|-------------|
| **FraiseQL** | **0.5-5ms** | **0.5-5ms** | **95%** | **Native** |
| PostGraphile | 50-100ms | 200-400ms | N/A | Plugin |
| Strawberry | 100-200ms | 300-600ms | External | Manual |
| Hasura | 25-75ms | 150-300ms | External | Limited |

### FraiseQL Optimization Layers
| Optimization Stack | Response Time | Use Case |
|-------------------|---------------|----------|
| **All 3 Layers** (APQ + TurboRouter + Passthrough) | **0.5-2ms** | High-performance production |
| **APQ + TurboRouter** | 2-5ms | Enterprise applications |
| **APQ + Passthrough** | 1-10ms | Modern web applications |
| **TurboRouter Only** | 5-25ms | API-focused applications |
| **Standard Mode** | 25-100ms | Development & complex queries |

*Real production benchmarks with PostgreSQL 15, 10k+ records*

## 🏗️ Architecture

FraiseQL's **Rust-first** architecture delivers exceptional performance through unified execution:

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   GraphQL       │ →  │   PostgreSQL     │ →  │   Rust          │
│   Request       │    │   JSONB Query    │    │   Transform     │
│                 │    │   (0.05-0.5ms)  │    │   (0.5ms)       │
└─────────────────┘    └──────────────────┘    └─────────────────┘
                                 ↓
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   APQ Hash      │ →  │   Storage        │ →  │   JSON          │
│   (SHA-256)     │    │   Backend        │    │   Passthrough   │
│                 │    │   Memory/PG      │    │   (0.5-2ms)     │
└─────────────────┘    └──────────────────┘    └─────────────────┘
    Optional Cache         FraiseQL Cache         Instant Response
```

### **Key Innovations**
1. **Unified Execution Path**: PostgreSQL → Rust → HTTP (no branching logic)
2. **Rust Field Projection**: 10-50x faster JSON field filtering than PostgreSQL
3. **Transform Tables**: `tv_*` tables with generated JSONB for instant queries
4. **APQ Storage Abstraction**: Pluggable backends (Memory/PostgreSQL) for query hash storage
5. **JSON Passthrough**: Sub-millisecond responses for cached queries with zero serialization

## 🚦 When to Choose FraiseQL

### **✅ Perfect For:**
- **Cost-conscious teams**: Save $300-3,000/month vs Redis + Sentry
- **High-performance APIs**: Sub-10ms response time requirements
- **Multi-tenant SaaS**: Per-tenant isolation and caching
- **PostgreSQL-first teams**: Already using PostgreSQL extensively
- **Operational simplicity**: One database for everything
- **Enterprise applications**: ACID guarantees, no eventual consistency
- **Self-hosted infrastructure**: Full control, no SaaS vendor lock-in

### **❌ Consider Alternatives:**
- **Simple CRUD**: Basic applications without performance requirements
- **Non-PostgreSQL databases**: FraiseQL is PostgreSQL-specific
- **Microservices**: Better suited for monolithic or database-per-service architectures

## 📊 PostgreSQL-Native Observability

FraiseQL includes a complete observability stack built directly into PostgreSQL—eliminating the need for external services like Sentry, Redis, or third-party APM tools.

### **Error Tracking** (Alternative to Sentry)
```python
from fraiseql.monitoring import init_error_tracker

tracker = init_error_tracker(db_pool, environment="production")
await tracker.capture_exception(error, context={...})

# Features:
# - Automatic error fingerprinting and grouping
# - Full stack trace capture
# - Request/user context preservation
# - OpenTelemetry trace correlation
# - Issue management (resolve, ignore, assign)
# - Custom notification triggers (Email, Slack, Webhook)
```

### **Caching** (Alternative to Redis)
```python
from fraiseql.caching import PostgresCache

cache = PostgresCache(db_pool)
await cache.set("key", value, ttl=3600)

# Features:
# - UNLOGGED tables for Redis-level performance
# - No WAL overhead = fast writes
# - Shared across instances
# - TTL-based expiration
# - Pattern-based deletion
```

### **OpenTelemetry Integration**
```python
# All traces and metrics stored in PostgreSQL
# Query for debugging:
SELECT * FROM monitoring.traces
WHERE error_id = 'error-123'  -- Full correlation
  AND trace_id = 'trace-xyz';
```

### **Grafana Dashboards**
Pre-built dashboards included in `grafana/`:
- Error monitoring dashboard
- OpenTelemetry traces dashboard
- Performance metrics dashboard
- All querying PostgreSQL directly

**Migration Guides**:
- [v1 to v2 Migration](./docs/migration/v1-to-v2.md) - Unified Rust-first architecture
- [Monitoring Migration](./docs/production/monitoring.md) - From Redis and Sentry

## 🛠️ CLI Commands

```bash
# Project management
fraiseql init <name>           # Create new project
fraiseql dev                   # Development server with hot reload
fraiseql check                 # Validate schema and configuration

# Code generation
fraiseql generate schema       # Export GraphQL schema
fraiseql generate types        # Generate TypeScript definitions

# Database utilities
fraiseql sql analyze <query>   # Analyze query performance
fraiseql sql explain <query>   # Show PostgreSQL execution plan
```

## 🤝 Contributing

We welcome contributions! See [CONTRIBUTING.md](CONTRIBUTING.md) for:
- Development setup and testing
- Architecture decisions and patterns
- Code style and review process

## 📚 Learn More

- **[Documentation](https://fraiseql.dev)** - Complete guides and API reference
- **[Examples](./examples/)** - Real-world applications and patterns
- **[Architecture](./docs/architecture/)** - Design decisions and trade-offs

## 🙏 Acknowledgments

FraiseQL draws inspiration from:
- **[Strawberry GraphQL](https://strawberry.rocks/)** - Excellent Python GraphQL library ("Fraise" = French for strawberry)
- **Harry Percival's "Architecture Patterns with Python"** - Clean architecture and repository patterns
- **Eric Evans' "Domain-Driven Design"** - Database-centric domain modeling
- **PostgreSQL community** - For building the world's most advanced open source database

## 👨‍💻 About

FraiseQL is created by **Lionel Hamayon** ([@evoludigit](https://github.com/evoludigit)), a self-taught developer and founder of [Évolution digitale](https://evolution-digitale.fr).

**Started: April 2025**

I built FraiseQL out of frustration with a stupid inefficiency: PostgreSQL returns JSON → Python deserializes to objects → GraphQL serializes back to JSON. Why are we doing this roundtrip?

After years moving through Django, Flask, FastAPI, and Strawberry GraphQL with SQLAlchemy, I realized the entire approach was wrong. Just let PostgreSQL return the JSON directly. Skip the ORM. Skip the object mapping.

But I also wanted something designed for the LLM era. SQL and Python are two of the most massively trained languages—LLMs understand them natively. Why not make a framework where AI can easily get context and generate correct code?

FraiseQL is the result: database-first CQRS where PostgreSQL does what it does best, Python stays minimal, and the whole architecture is LLM-readable by design.

Full disclosure: I built this while compulsively preparing for scale I didn't have. But that obsession led somewhere real—sub-millisecond responses, zero N+1 queries, and a framework that both humans and AI can understand.

**Connect:**
- 💼 GitHub: [@evoludigit](https://github.com/evoludigit)
- 📧 lionel.hamayon@evolution-digitale.fr
- 🏢 [Évolution digitale](https://evolution-digitale.fr)

**Support FraiseQL:**
- ⭐ Star [fraiseql/fraiseql](https://github.com/fraiseql/fraiseql)
- 💬 Join discussions and share feedback
- 🤝 Contribute to the project

## 📄 License

MIT License - see [LICENSE](LICENSE) for details.

---

**Ready to build the fastest GraphQL API in Python?**

```bash
pip install fraiseql && fraiseql init my-fast-api
```
