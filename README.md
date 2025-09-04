# FraiseQL

[![Quality Gate](https://github.com/fraiseql/fraiseql/actions/workflows/quality-gate.yml/badge.svg?branch=dev)](https://github.com/fraiseql/fraiseql/actions/workflows/quality-gate.yml)
[![Documentation](https://github.com/fraiseql/fraiseql/actions/workflows/docs.yml/badge.svg)](https://github.com/fraiseql/fraiseql/actions/workflows/docs.yml)
[![Release](https://img.shields.io/github/v/release/fraiseql/fraiseql)](https://github.com/fraiseql/fraiseql/releases/latest)
[![Python](https://img.shields.io/badge/Python-3.13+-blue.svg)](https://www.python.org/downloads/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

**The fastest Python GraphQL framework.** Pre-compiled queries, PostgreSQL-native caching, and sub-millisecond responses out of the box.

> **4-100x faster** than traditional GraphQL frameworks • **Database-first architecture** • **Zero external dependencies**

## 🚀 Why FraiseQL?

### **⚡ Blazing Fast Performance**
- **Pre-compiled queries**: SHA-256 hash lookup instead of parsing (4-10x faster)
- **PostgreSQL-native caching**: No Redis, no external dependencies
- **Sub-millisecond responses**: 2-5ms cached, 25-60ms uncached
- **Real production benchmarks**: 85-95% cache hit rate

### **🏗️ Database-First Architecture**
- **CQRS by design**: Commands via PostgreSQL functions, queries via views
- **JSONB-powered**: Flexible schema evolution with full type safety
- **View-based queries**: `v_*` for real-time, `tv_*` for materialized performance
- **PostgreSQL does the heavy lifting**: Joins, aggregations, transformations in-database

### **🔧 Developer Experience**
- **Type-safe**: Full Python 3.13+ type hints with automatic GraphQL schema generation
- **One command setup**: `fraiseql init my-api && fraiseql dev`
- **Intelligent WHERE clauses**: Automatic type-aware SQL optimization for network types, dates, and more
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
    repo = info.context["repo"]
    return await repo.find("v_user")
EOF

# Start development server
fraiseql dev
```

Your GraphQL API is live at `http://localhost:8000/graphql` 🎉

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

@fraiseql.success
class CreateUserSuccess:
    user: User
    message: str = "User created successfully"

@fraiseql.failure
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

## 📊 Performance Comparison

| Framework | Simple Query | Complex Query | Cache Hit |
|-----------|-------------|---------------|-----------|
| **FraiseQL** | **2-5ms** | **2-5ms** | **95%** |
| PostGraphile | 50-100ms | 200-400ms | N/A |
| Strawberry | 100-200ms | 300-600ms | External |
| Hasura | 25-75ms | 150-300ms | External |

*Real production benchmarks with PostgreSQL 15, 10k+ records*

## 🏗️ Architecture

FraiseQL's **storage-for-speed** philosophy trades disk space for exceptional performance:

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   GraphQL       │ →  │   Pre-compiled   │ →  │   PostgreSQL    │
│   Query         │    │   SHA-256 Hash   │    │   Cached Result │
│                 │    │   Lookup (O(1))  │    │   (JSONB)       │
└─────────────────┘    └──────────────────┘    └─────────────────┘
     100-300ms                1-2ms                   2-5ms
   Traditional              FraiseQL               FraiseQL + Cache
```

### **Key Innovations**
1. **TurboRouter**: Pre-compiles GraphQL queries into optimized SQL with hash-based lookup
2. **JSONB Views**: PostgreSQL returns GraphQL-ready JSON, eliminating serialization overhead
3. **Intelligent Caching**: Database-native caching with automatic invalidation on data changes
4. **Type-Aware SQL**: Automatic PostgreSQL type casting based on GraphQL field types

## 🚦 When to Choose FraiseQL

### **✅ Perfect For:**
- **High-performance APIs**: Sub-10ms response time requirements
- **Multi-tenant SaaS**: Per-tenant isolation and caching
- **PostgreSQL-first**: Teams already using PostgreSQL extensively
- **Enterprise applications**: ACID guarantees, no eventual consistency
- **Cost-sensitive projects**: 70% infrastructure cost reduction

### **❌ Consider Alternatives:**
- **Simple CRUD**: Basic applications without performance requirements
- **Non-PostgreSQL databases**: FraiseQL is PostgreSQL-specific
- **Microservices**: Better suited for monolithic or database-per-service architectures

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

## 📄 License

MIT License - see [LICENSE](LICENSE) for details.

---

**Ready to build the fastest GraphQL API in Python?**

```bash
pip install fraiseql && fraiseql init my-fast-api
```
