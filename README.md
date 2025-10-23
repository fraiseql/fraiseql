# FraiseQL

[![Quality Gate](https://github.com/fraiseql/fraiseql/actions/workflows/quality-gate.yml/badge.svg?branch=dev)](https://github.com/fraiseql/fraiseql/actions/workflows/quality-gate.yml)
[![Documentation](https://github.com/fraiseql/fraiseql/actions/workflows/docs.yml/badge.svg)](https://github.com/fraiseql/fraiseql/actions/workflows/docs.yml)
[![Release](https://img.shields.io/github/v/release/fraiseql/fraiseql)](https://github.com/fraiseql/fraiseql/releases/latest)
[![Python](https://img.shields.io/badge/Python-3.13+-blue.svg)](https://www.python.org/downloads/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Version Status](https://img.shields.io/badge/Status-Production%20Stable-green.svg)](./VERSION_STATUS.md)

**📍 You are here: Main FraiseQL Framework (v1.0.0) - Production Stable**

> **Version Status**: See [VERSION_STATUS.md](./VERSION_STATUS.md) for complete version roadmap and recommendations

**The fastest Python GraphQL framework. In PostgreSQL Everything.**

Pre-compiled queries, Automatic Persisted Queries (APQ), PostgreSQL-native caching, error tracking, and observability—all in one database.

> **2-4x faster** than traditional GraphQL frameworks • **In PostgreSQL Everything** • **$300-3,000/month savings** • **Zero external dependencies**

## 📋 Project Versions & Navigation

### Version Overview
| Version | Location | Status | Purpose | For Users? |
|---------|----------|--------|---------|------------|
| **v1.0.0** | Root level | Production Stable | Stable release | ✅ Recommended |
| **Rust Pipeline** | [`fraiseql_rs/`](fraiseql_rs/) | Integrated | Included in v1.0+ | ✅ Stable |
| **v0.11.5** | Superseded | Legacy | Use v1.0.0 | ⚠️ Migrate |

**New to FraiseQL?** → **[Getting Started](GETTING_STARTED.md)** • [Project Structure](PROJECT_STRUCTURE.md) • [Documentation](docs/)

---

## 👥 Is This For Me?

**FraiseQL is designed for production teams** building GraphQL APIs with PostgreSQL. Here's how to know if it's right for you:

### **✅ You Should Use FraiseQL If:**
- Building GraphQL APIs with PostgreSQL
- Need sub-millisecond query performance
- Want database-native caching and monitoring
- Prefer zero external dependencies
- Team size: 2-50 developers

### **❌ Consider Alternatives If:**
- Not using PostgreSQL as your primary database
- Need multi-database support
- Prefer traditional ORM approaches
- Building simple CRUD APIs (consider REST)

### **🏁 Choose Your Path**

**Prerequisites**: Python 3.13+, PostgreSQL 13+

#### 🆕 Brand New to FraiseQL?
**[📚 First Hour Guide](docs/FIRST_HOUR.md)** - 60 minutes, hands-on
- Progressive tutorial from zero to production
- Builds complete blog API
- Covers CQRS, types, mutations, testing
- **Recommended for**: Learning the framework thoroughly

#### ⚡ Want to See It Working Now?
**[⚡ 5-Minute Quickstart](docs/quickstart.md)** - Copy, paste, run
- Working API in 5 minutes
- Minimal explanation
- **Recommended for**: Evaluating the framework quickly

#### 🧠 Prefer to Understand First?
**[🧠 Understanding FraiseQL](docs/UNDERSTANDING.md)** - 10 minute read
- Conceptual overview with diagrams
- Architecture deep dive
- No code, just concepts
- **Recommended for**: Architects and decision-makers

#### 📖 Already Using FraiseQL?
**[📖 Quick Reference](docs/reference/quick-reference.md)** - Lookup syntax and patterns
**[📚 Full Documentation](docs/)** - Complete guides and references

---

**New here?** → Start with [First Hour Guide](docs/FIRST_HOUR.md)
**Need help?** → See [Troubleshooting](docs/TROUBLESHOOTING.md)

**For Contributors:**
```bash
git clone https://github.com/fraiseql/fraiseql
cd fraiseql && make setup-dev
```

**Learn more:** [Audiences Guide](AUDIENCES.md) • [Getting Started](GETTING_STARTED.md)

---

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
- **JSON passthrough optimization**: Sub-millisecond cached responses (0.5-2ms for simple queries)
- **Pre-compiled queries**: TurboRouter with intelligent caching (2-4x faster than standard GraphQL)
- **Real production benchmarks**: 85-95% cache hit rate for stable query patterns

**[📊 Performance Guide](docs/performance/index.md)** - Methodology, realistic expectations, and benchmark details

### **🏗️ Database-First Architecture**
- **CQRS by design**: Commands via PostgreSQL functions, queries via views
- **JSONB-powered**: Flexible schema evolution with full type safety
- **View-based queries**: `v_*` for real-time, `tv_*` for materialized performance
- **PostgreSQL does the heavy lifting**: Joins, aggregations, transformations in-database

### **🔧 Developer Experience**
- **Type-safe**: Full Python 3.13+ type hints with automatic GraphQL schema generation
- **Automatic documentation**: Python docstrings become GraphQL descriptions in Apollo Studio
- **One command setup**: `fraiseql init my-api && fraiseql dev`
- **Convention over Configuration**: Clean registration-based API with `@auto_nested_array_filters`
- **Intelligent WHERE clauses**: Automatic type-aware SQL optimization for network types, dates, and more
- **JSON-only views**: PostgreSQL JSONB views for optimal performance with optional hybrid tables for efficient filtering
- **Built-in security**: Field-level authorization, rate limiting, CSRF protection

## 🔄 How It Works

### **Request Flow: GraphQL → PostgreSQL → Rust → Response**

Every GraphQL request follows this optimized path:

```
┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│   GraphQL   │───▶│   FastAPI   │───▶│ PostgreSQL  │───▶│    Rust     │
│   Query     │    │  Resolver   │    │   View      │    │ Transform   │
│             │    │             │    │             │    │             │
│ { users {   │    │ @query      │    │ SELECT      │    │ jsonb →     │
│   name      │    │ def users:  │    │ jsonb_build_ │    │ GraphQL     │
│ } }         │    │   return db │    │ object     │    │ Response    │
└─────────────┘    └─────────────┘    └─────────────┘    └─────────────┘
```

1. **GraphQL Query** arrives at FastAPI
2. **Python Resolver** calls PostgreSQL view/function
3. **Database** returns pre-composed JSONB
4. **Rust Pipeline** transforms to GraphQL response

**[📊 Detailed Request Flow Diagram](docs/diagrams/request-flow.md)** - Complete lifecycle with examples
**[Deep dive: Understanding FraiseQL](docs/UNDERSTANDING.md)** - 10-minute visual guide to the architecture

### **CQRS Pattern: Reads vs Writes**

FraiseQL implements **Command Query Responsibility Segregation**:

```
┌─────────────────────────────────────┐
│         GraphQL API                 │
├──────────────────┬──────────────────┤
│   QUERIES        │   MUTATIONS      │
│   (Reads)        │   (Writes)       │
├──────────────────┼──────────────────┤
│  v_* views       │  fn_* functions  │
│  tv_* tables     │  tb_* tables     │
└──────────────────┴──────────────────┘
```

**Queries** use views for fast, fresh data. **Mutations** use functions for business logic.

**[🔄 CQRS Pattern Details](docs/diagrams/cqrs-pattern.md)** - Read vs write separation explained

## 🏁 Quick Start

**Prerequisites**: Python 3.13+, PostgreSQL 13+

```bash
# Install FraiseQL
pip install fraiseql

# Create your first project
fraiseql init my-api
cd my-api

# Start development server
fraiseql dev
```

Your GraphQL API is live at `http://localhost:8000/graphql` 🎉

**[📖 Detailed Installation Guide](INSTALLATION.md)** - Multiple installation options, troubleshooting, and platform-specific instructions

## 🧠 Core Concepts

FraiseQL uses innovative patterns that might be new if you're coming from traditional frameworks:

### **CQRS (Command Query Responsibility Segregation)**
Separate reading data from writing data - like having separate lines for ordering food vs. picking it up.
- **Commands** (writes): `tb_*` tables for data changes
- **Queries** (reads): `v_*`/`tv_*` views for data access

### **JSONB Views**
Pre-packaged data as JSONB objects for GraphQL - like meal kits ready to serve.
- **`v_*` views**: Real-time JSONB computation
- **`tv_*` tables**: Pre-computed JSONB for speed

### **Trinity Identifiers**
Three types of identifiers per entity for different purposes:
- **`pk_*`**: Internal fast joins (never exposed)
- **`id`**: Public API identifier (UUID)
- **`identifier`**: Human-readable slug (SEO-friendly)

### **Database-First Architecture**
Design your API starting from PostgreSQL, not the other way around. Business logic lives in database functions.

**[Learn more about these concepts →](docs/core/concepts-glossary.md)**

## 🔄 Automatic Persisted Queries (APQ)

FraiseQL provides enterprise-grade APQ support with pluggable storage backends:

### **Storage Backends**
```python
from fraiseql import FraiseQLConfig

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
- **85-95% cache hit rates** in production applications (99.9% for highly stable query patterns)
- **70% bandwidth reduction** with large queries
- **Multi-instance coordination** with PostgreSQL backend
- **Automatic cache warming** for frequently used queries

**[⚡ APQ Cache Flow Details](docs/diagrams/apq-cache-flow.md)** - How persisted queries work

## 🎯 Core Features

### **Enterprise Security & Compliance**
- **Unified Audit Logging with Cryptographic Chain**: Tamper-proof audit trails with SHA-256 hashing and HMAC signatures
- **PostgreSQL-native crypto**: No Python overhead for event creation and verification
- **Multi-tenant isolation**: Per-tenant cryptographic chains for SOX/HIPAA compliance
- **Field-level authorization**: Decorator-based access control with role inheritance
- **Row-level security**: PostgreSQL RLS integration for data isolation

### **Advanced Type System**
Specialized operators for network types, hierarchical data, ranges, and **comprehensive nested array filtering**:

```graphql
query {
  servers(where: {
    ipAddress: { eq: "192.168.1.1" }        # → ::inet casting
    port: { gt: 1024 }                      # → ::integer casting
    macAddress: { eq: "aa:bb:cc:dd:ee:ff" } # → ::macaddr casting
    location: { ancestor_of: "US.CA" }      # → ltree operations
    dateRange: { overlaps: "[2024-01-01,2024-12-31)" }
    # Nested array filtering with logical operators
    printServers(where: {
      AND: [
        { operatingSystem: { in: ["Linux", "Windows"] } }
        { OR: [
            { nTotalAllocations: { gte: 100 } }
            { hostname: { contains: "critical" } }
          ]
        }
        { NOT: { ipAddress: { isnull: true } } }
      ]
    }) {
      hostname operatingSystem nTotalAllocations
    }
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
- **Field Projection**: Rust processes JSON 7-10x faster than Python
- **Zero Python Overhead**: Direct RustResponseBytes to FastAPI

**Supported specialized types:**
- **Network**: `IPv4`, `IPv6`, `CIDR`, `MACAddress` with subnet/range operations
- **Hierarchical**: `LTree` with ancestor/descendant queries
- **Temporal**: `DateRange` with overlap/containment operations
- **Standard**: `EmailAddress`, `UUID`, `JSON` with validation
- **Nested Arrays**: Complete AND/OR/NOT logical operators for filtering array elements

> 📖 **[Complete Nested Array Filtering Guide →](docs/nested-array-filtering.md)**

### **Intelligent Mutations**
PostgreSQL functions handle business logic with structured error handling:

```python
from fraiseql import input, mutation
from typing import Optional

@input
class CreateUserInput:
    name: str
    email: str  # Email validation handled by PostgreSQL

@mutation
def create_user(input: CreateUserInput) -> Optional[User]:
    """Create a new user."""
    pass  # Implementation handled by framework
```

### **Multi-Tenant Architecture**
Built-in tenant isolation with per-tenant caching:

```python
from fraiseql import query
from typing import List

# Automatic tenant context
@query
def users() -> List[User]:
    """Get all users for current tenant."""
    pass  # Implementation handled by framework
```

### **Table Views (tv_*)**
Denormalized projection tables for instant GraphQL responses:

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
from fraiseql import type, query
from typing import List

# Type definition
@type(sql_source="tv_user", jsonb_column="data")
class User:
    id: int
    first_name: str      # Rust transforms to firstName
    user_posts: List[Post]  # Embedded relations!

# Query (0.05ms lookup + 0.5ms Rust transform)
@query
def user(id: int) -> User:
    """Get user by ID."""
    pass  # Implementation handled by framework
```

**Benefits:**
- **0.05-0.5ms database lookup time** (10-100x faster than complex JOINs for nested data)
- **Embedded relations** (no N+1 queries)
- **Always up-to-date** (generated columns + triggers)
- **Rust field projection** (7-10x faster than Python JSON processing)

## 📊 Performance Comparison

### Framework Comparison (Real Measurements)
| Framework | Simple Query | Complex Query | Cache Hit | APQ Support |
|-----------|-------------|---------------|-----------|-------------|
| **FraiseQL** | **1-5ms** | **5-25ms** | **85-95%** | **Native** |
| PostGraphile | 50-100ms | 200-400ms | N/A | Plugin |
| Strawberry | 100-200ms | 300-600ms | External | Manual |
| Hasura | 25-75ms | 150-300ms | External | Limited |

*Test conditions: PostgreSQL 15, 10k records, standard cloud instance. See [Performance Guide](docs/performance/index.md) for methodology.*

### FraiseQL Optimization Layers
| Optimization Stack | Response Time | Use Case |
|-------------------|---------------|----------|
| **Rust Pipeline + APQ** | **0.5-2ms** | Production applications |
| **Rust Pipeline only** | **1-5ms** | Development & testing |

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
│   APQ Hash      │ →  │   Storage        │ →  │   HTTP          │
│   (SHA-256)     │    │   Backend        │    │   Response      │
│                 │    │   Memory/PG      │    │   (0.5-2ms)     │
└─────────────────┘    └──────────────────┘    └─────────────────┘
    Optional Cache         FraiseQL Cache         Instant Response
```

### **Key Innovations**
1. **Exclusive Rust Pipeline**: PostgreSQL → Rust → HTTP (no Python overhead)
2. **Rust Field Projection**: 7-10x faster JSON transformation than Python
3. **Table Views**: `tv_*` tables with generated JSONB for instant queries
4. **APQ Storage Abstraction**: Pluggable backends (Memory/PostgreSQL) for query hash storage
5. **Zero-Copy Path**: Sub-millisecond responses with zero Python serialization

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
