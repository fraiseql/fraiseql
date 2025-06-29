<p align="center">
  <img src="./docs/assets/logo.png" alt="FraiseQL Logo" width="200" />
</p>

<p align="center">
  <strong>A GraphQL-to-PostgreSQL translator with a CQRS architecture</strong><br>
  <em>Views for queries. Functions for mutations. GraphQL for developers.</em>
</p>

[![CI](https://github.com/fraiseql/fraiseql/actions/workflows/ci.yml/badge.svg)](https://github.com/fraiseql/fraiseql/actions/workflows/ci.yml)
[![Test Suite](https://github.com/fraiseql/fraiseql/actions/workflows/test.yml/badge.svg)](https://github.com/fraiseql/fraiseql/actions/workflows/test.yml)
[![Security](https://github.com/fraiseql/fraiseql/actions/workflows/security.yml/badge.svg)](https://github.com/fraiseql/fraiseql/actions/workflows/security.yml)
[![Documentation](https://github.com/fraiseql/fraiseql/actions/workflows/docs.yml/badge.svg)](https://github.com/fraiseql/fraiseql/actions/workflows/docs.yml)
[![codecov](https://codecov.io/gh/fraiseql/fraiseql/branch/main/graph/badge.svg)](https://codecov.io/gh/fraiseql/fraiseql)
[![Python](https://img.shields.io/badge/python-3.11+-blue.svg)](https://www.python.org/downloads/)
[![PyPI version](https://img.shields.io/badge/pypi-v0.1.0b1-blue.svg)](https://badge.fury.io/py/fraiseql)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![pre-commit](https://img.shields.io/badge/pre--commit-enabled-brightgreen?logo=pre-commit)](https://github.com/pre-commit/pre-commit)

**FraiseQL** is a Python framework that translates GraphQL queries directly into PostgreSQL queries, embracing a CQRS (Command Query Responsibility Segregation) architecture where database views handle queries and PostgreSQL functions handle mutations.

## ⚡ Quick Links

| Getting Started | Reference | Learn More |
|----------------|-----------|------------|
| 🚀 [Getting Started Guide](docs/GETTING_STARTED.md) | 📖 [API Reference](docs/API_REFERENCE.md) | 🏗️ [Architecture](docs/ARCHITECTURE.md) |
| 📚 [Query Patterns](docs/QUERY_PATTERNS.md) | 🔧 [Common Patterns](docs/COMMON_PATTERNS.md) | 🔄 [Migration Guide](docs/MIGRATION_TO_JSONB_PATTERN.md) |
| 🔍 [Filtering Patterns](docs/FILTERING_PATTERNS.md) | 💡 [Examples](examples/) | 📝 [Contributing](CONTRIBUTING.md) |
| 🔍 [WHERE Types Guide](docs/WHERE_TYPES.md) | 🆕 [Partial Instantiation](docs/PARTIAL_INSTANTIATION.md) | |
| ❓ [Troubleshooting](docs/TROUBLESHOOTING.md) | | |

## 🎯 Core Concepts

FraiseQL has four fundamental patterns:

### 1. Types are Python Classes
```python
@fraise_type
class User:
    id: UUID
    name: str
    email: str
```

### 2. Queries are Functions (Not Resolvers!)
```python
@fraiseql.query
async def get_user(info, id: UUID) -> User:
    # 'info' is ALWAYS first parameter
    db = info.context["db"]
    return await db.find_one("user_view", id=id)
```

### 3. All Data in JSONB Column (v0.1.0a14+)
```sql
CREATE VIEW user_view AS
SELECT 
    id,              -- For filtering
    tenant_id,       -- For access control
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email
    ) as data        -- REQUIRED: All object data here
FROM users;
```

### 4. Repository Handles Database
```python
# No manual connection management needed
db = info.context["db"]  # FraiseQLRepository
user = await db.find_one("user_view", id=user_id)
```

## ✨ Key Features

| Feature | Description |
|---------|-------------|
| 🚀 **Simple Patterns** | Decorator-based API with no resolver classes |
| 📊 **JSONB-First** | All data flows through JSONB columns for consistency |
| 🔐 **Type-Safe** | Full type hints with Python 3.11+ |
| 🛡️ **SQL Injection Safe** | Parameterized queries throughout |
| 🔌 **Pluggable Auth** | Built-in Auth0, easy to add others |
| ⚡ **FastAPI Integration** | Production-ready ASGI application |
| 🏎️ **High Performance** | Direct SQL queries, no N+1 problems |
| 🎯 **CQRS Architecture** | Views for queries, functions for mutations |

## 📦 Installation

```bash
pip install fraiseql

# With optional features:
pip install "fraiseql[auth0]"      # Auth0 authentication
pip install "fraiseql[tracing]"    # OpenTelemetry tracing
pip install "fraiseql[dev]"        # Development dependencies
```

> ⚠️ **Breaking Changes**: 
> - **v0.1.0a14**: All database views must now return data in a JSONB `data` column. See the [Migration Guide](docs/MIGRATION_TO_JSONB_PATTERN.md)
> - **v0.1.0a18**: Partial object instantiation is now supported in development mode, allowing nested queries to request only specific fields

## 🚀 Quick Start

### 1. Hello World (No Database)

```python
import fraiseql
from datetime import datetime
from uuid import UUID, uuid4

# Define a type
@fraise_type
class Book:
    id: UUID
    title: str
    author: str
    published: datetime

# Create a query (NOT a resolver!)
@fraiseql.query
async def books(info) -> list[Book]:
    """Get all books."""
    # 'info' is ALWAYS the first parameter
    return [
        Book(
            id=uuid4(),
            title="The Great Gatsby",
            author="F. Scott Fitzgerald",
            published=datetime(1925, 4, 10)
        )
    ]

# Create the app
app = fraiseql.create_fraiseql_app(
    types=[Book],
    production=False  # Enables GraphQL Playground
)

# Run with: uvicorn app:app --reload
# Visit: http://localhost:8000/graphql
```

### 2. With Database (The Right Way)

```python
# First, create your database view with JSONB data column:
"""
CREATE VIEW book_view AS
SELECT 
    id,              -- For filtering
    author,          -- For author queries
    published,       -- For date filtering
    jsonb_build_object(
        'id', id,
        'title', title,
        'author', author,
        'published', published
    ) as data        -- REQUIRED: All object data here!
FROM books;
"""

# Then create your query:
@fraiseql.query
async def books(info, author: str | None = None) -> list[Book]:
    """Get books, optionally filtered by author."""
    db = info.context["db"]  # FraiseQLRepository
    
    if author:
        return await db.find("book_view", author=author)
    return await db.find("book_view")

# Create app with database
app = fraiseql.create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[Book],
    production=False
)
```

### 3. Common Mistakes to Avoid

```python
# ❌ WRONG: Don't use resolver classes
class Query:
    async def resolve_users(self, info):
        pass

# ✅ CORRECT: Use @fraiseql.query decorator
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("user_view")

# ❌ WRONG: Don't forget the data column
CREATE VIEW bad_view AS
SELECT id, name, email FROM users;

# ✅ CORRECT: Always include JSONB data column
CREATE VIEW good_view AS
SELECT id, jsonb_build_object(
    'id', id, 'name', name, 'email', email
) as data FROM users;
```

👉 **See the [Getting Started Guide](docs/GETTING_STARTED.md) for a complete walkthrough**

## 🆕 New in v0.1.0a18: Partial Object Instantiation

FraiseQL now supports partial object instantiation for nested queries in development mode. This means you can request only the fields you need from nested objects without errors:

```graphql
query GetUsers {
  users {
    id
    name
    profile {
      avatar  # Only request avatar, not all profile fields
    }
  }
}
```

```python
@fraise_type
class Profile:
    id: UUID
    avatar: str
    email: str      # Required but not requested - no error!
    bio: str        # Required but not requested - no error!
```

This brings FraiseQL closer to GraphQL's promise of "ask for what you need, get exactly that". See the [Partial Instantiation Guide](docs/PARTIAL_INSTANTIATION.md) for details.

### Complex Nested Query Example

With partial instantiation, you can now build efficient queries that traverse multiple levels of relationships:

```graphql
query BlogDashboard {
  posts(where: { published_at: { neq: null } }) {
    id
    title
    published_at
    author {
      name
      profile {
        avatar  # Only need avatar for display
      }
    }
    comments {
      id
      content
      author {
        name  # Only need commenter's name
      }
    }
  }
}
```

All nested objects will be properly instantiated with only the requested fields, avoiding errors from missing required fields in the type definitions.

## 🎯 Why FraiseQL?

### The Problem
Traditional GraphQL servers require complex resolver hierarchies, N+1 query problems, and lots of boilerplate.

### The FraiseQL Solution
- **Direct SQL queries** from GraphQL queries
- **JSONB pattern** for consistent data access
- **No resolver classes** - just functions
- **Type-safe** with full IDE support
- **Production-ready** with auth, caching, and monitoring
- **Partial field selection** (v0.1.0a18+) for optimal queries

## 📚 Learn More

| Topic | Description |
|-------|-------------|
| [Query Patterns](docs/QUERY_PATTERNS.md) | How to write queries the FraiseQL way |
| [JSONB Pattern](docs/ARCHITECTURE.md#the-jsonb-data-column-pattern) | Why all data goes in a JSONB column |
| [Multi-Tenancy](docs/COMMON_PATTERNS.md#multi-tenant-applications) | Building SaaS applications |
| [Authentication](docs/COMMON_PATTERNS.md#authentication--authorization) | Adding auth to your API |
| [Testing](docs/testing/unified-container-testing.md) | Our unified container approach |

## Real-World Example

Here's how you might structure a blog application:

```python
@fraise_type
class Post:
    id: UUID
    title: str
    content: str
    author: User
    comments: list['Comment']
    tags: list[str]
    published_at: datetime | None

@fraise_type
class Comment:
    id: UUID
    content: str
    author: User
    created_at: datetime
```

With a corresponding view:

```sql
CREATE VIEW post_details AS
SELECT
    p.id,                    -- For filtering
    p.author_id,             -- For joins
    p.published_at,          -- For filtering by date
    jsonb_build_object(
        'id', p.id,
        'title', p.title,
        'content', p.content,
        'author', (
            SELECT data FROM user_profile
            WHERE id = p.author_id  -- Use id column for filtering
        ),
        'comments', (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'id', c.id,
                    'content', c.content,
                    'author', (
                        SELECT data FROM user_profile
                        WHERE id = c.author_id  -- Use id column for filtering
                    ),
                    'created_at', c.created_at
                )
                ORDER BY c.created_at
            )
            FROM comments c
            WHERE c.post_id = p.id
        ),
        'tags', p.tags,
        'published_at', p.published_at
    ) as data                -- All object data in 'data' column
FROM posts p;
```

## Authentication

FraiseQL includes a pluggable authentication system:

```python
from fraiseql.auth.decorators import requires_auth
from fraiseql.auth.auth0 import Auth0Config

@fraise_type
class Query:
    @requires_auth
    async def me(self, info) -> User:
        # info.context["user"] contains authenticated user info
        user_id = info.context["user"].user_id
        # Fetch and return user...

app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    auth=Auth0Config(
        domain="your-domain.auth0.com",
        api_identifier="https://api.example.com"
    ),
    types=[User, Query],
)
```

## LLM-Friendly Architecture

FraiseQL's design makes it exceptionally well-suited for AI-assisted development:

### Clear Contracts
- **Explicit type definitions** with decorators (`@fraise_type`, `@fraise_input`)
- **Structured mutations** with success/failure unions
- **Well-defined boundaries** between queries (views) and mutations (functions)
- **No hidden magic** - what you define is what you get

### Simple, Common Languages
- **Just Python and SQL** - no proprietary DSLs or complex configurations
- **Standard PostgreSQL** - 40+ years of documentation and examples
- **Familiar patterns** - decorators and dataclasses that LLMs understand well

### Predictable Code Generation
When you ask an LLM to generate a FraiseQL API, it can reliably produce:

```python
# LLMs can easily generate this pattern
@fraise_type
class Product:
    id: UUID
    name: str
    price: Decimal
    in_stock: bool

# And the corresponding SQL view
"""
CREATE VIEW product_catalog AS
SELECT 
    id,              -- For filtering
    category_id,     -- For joins
    jsonb_build_object(
        'id', id,
        'name', name,
        'price', price,
        'in_stock', quantity > 0
    ) as data        -- All product data in 'data' column
FROM products;
"""
```

This simplicity means:
- **Lower token costs** - concise, standard patterns
- **Higher accuracy** - LLMs trained on Python/SQL perform better
- **Faster iteration** - generate, test, and refine quickly
- **Maintainable output** - generated code looks like human-written code

## Development

### Prerequisites

- Python 3.11+
- PostgreSQL 13+
- Podman or Docker (optional, for integration tests)

### Setting Up

```bash
# Clone the repo
git clone https://github.com/fraiseql/fraiseql.git
cd fraiseql

# Create virtual environment
python -m venv .venv
source .venv/bin/activate

# Install in development mode
pip install -e ".[dev]"

# Run tests (uses unified container for performance)
pytest  # Automatically detects available container runtime

# Or explicitly with Podman (recommended for socket performance)
TESTCONTAINERS_PODMAN=true pytest

# Skip container-based tests if no runtime available
pytest -m "not docker"
```

### Container Runtime

FraiseQL uses a **unified container approach** for testing - a single PostgreSQL container runs for the entire test session with socket-based communication, providing 5-10x faster test execution.

- **Podman** (recommended): Rootless, daemonless, uses Unix domain sockets
- **Docker**: Traditional container runtime

Tests requiring containers are automatically skipped if neither is available. See [docs/testing/unified-container-testing.md](docs/testing/unified-container-testing.md) for architecture details.

### Code Quality

```bash
# Linting
ruff check src/

# Type checking
pyright

# Format code
ruff format src/ tests/
```

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## Why "FraiseQL"?

"Fraise" is French for strawberry. This project was heavily inspired by the excellent [Strawberry GraphQL](https://strawberry.rocks/) library, whose elegant API design showed us how delightful Python GraphQL development could be. While we take a different architectural approach, we aim to preserve that same developer-friendly experience.

## Current Status

FraiseQL is in active development. We're working on:

- Performance benchmarks and optimization
- Additional authentication providers
- Enhanced query compilation for production
- More comprehensive documentation
- Real-world example applications

## License

MIT License - see [LICENSE](LICENSE) for details.

---

**FraiseQL**: Where GraphQL meets PostgreSQL. 🍓
