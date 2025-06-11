<p align="center">
  <img src="./docs/assets/logo.png" alt="FraiseQL Logo" width="200" />
</p>

<p align="center">
  <strong>A GraphQL-to-PostgreSQL translator with a CQRS architecture</strong><br>
  <em>Views for queries. Functions for mutations. GraphQL for developers.</em>
</p>

[![CI](https://github.com/fraiseql/fraiseql/actions/workflows/ci.yml/badge.svg)](https://github.com/fraiseql/fraiseql/actions/workflows/ci.yml)
[![Test Suite](https://github.com/fraiseql/fraiseql/actions/workflows/test.yml/badge.svg)](https://github.com/fraiseql/fraiseql/actions/workflows/test.yml)
[![codecov](https://codecov.io/gh/fraiseql/fraiseql/branch/main/graph/badge.svg?token=YOUR_TOKEN)](https://codecov.io/gh/fraiseql/fraiseql)
[![Python](https://img.shields.io/badge/python-3.11+-blue.svg)](https://www.python.org/downloads/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![pre-commit](https://img.shields.io/badge/pre--commit-enabled-brightgreen?logo=pre-commit)](https://github.com/pre-commit/pre-commit)

**FraiseQL** is a Python framework that translates GraphQL queries directly into PostgreSQL queries, embracing a CQRS (Command Query Responsibility Segregation) architecture where database views handle queries and PostgreSQL functions handle mutations.

## Core Architecture

FraiseQL takes a different approach to GraphQL:

- **Queries** → PostgreSQL Views (optimized reads)
- **Mutations** → PostgreSQL Functions (business logic in the database)
- **Types** → Python dataclasses (with GraphQL decorators)

This means your GraphQL queries become simple `SELECT` statements from views, while mutations call PostgreSQL functions that return structured results.

## Key Features

- 🏗️ **CQRS Architecture** - Clear separation between reads (views) and writes (functions)
- 🍓 **Strawberry-Inspired API** - Familiar decorator patterns for Python developers
- 🔐 **Type-Safe** - Full type hints with Python 3.11+ and runtime validation
- 🛡️ **SQL Injection Safe** - All queries use parameterized SQL
- 🔌 **Pluggable Auth** - Modular authentication system (Auth0 provider included)
- 🧪 **TestFoundry** - Generate pgTAP tests for your database operations
- ⚡ **FastAPI Integration** - Production-ready ASGI application

## Installation

```bash
pip install fraiseql

# With Auth0 support:
pip install "fraiseql[auth0]"
```

## Quick Start

### 1. Define Your Types

```python
from uuid import UUID
from fraiseql import fraise_type, fraise_input, field

@fraise_type
class User:
    id: UUID
    email: str = field(description="User's email address")
    name: str = field(description="Display name")
    post_count: int = field(description="Number of posts by this user")
```

### 2. Set Up Your Database

Create a view that returns JSONB matching your GraphQL type:

```sql
CREATE VIEW user_profile AS
SELECT
    jsonb_build_object(
        'id', u.id,
        'email', u.email,
        'name', u.name,
        'post_count', COUNT(p.id)
    ) as data
FROM users u
LEFT JOIN posts p ON p.author_id = u.id
GROUP BY u.id;
```

### 3. Create Mutations with PostgreSQL Functions

```python
@fraise_input
class CreateUserInput:
    email: str
    name: str

@success
class CreateUserSuccess:
    user: User
    message: str = "User created successfully"

@failure
class CreateUserError:
    message: str
    code: str = field(description="Error code for client handling")

@mutation
class CreateUser:
    input: CreateUserInput
    success: CreateUserSuccess
    error: CreateUserError
```

The corresponding PostgreSQL function:

```sql
CREATE FUNCTION graphql.create_user(input jsonb)
RETURNS jsonb AS $$
DECLARE
    new_user_id uuid;
BEGIN
    -- Validate email uniqueness
    IF EXISTS (SELECT 1 FROM users WHERE email = input->>'email') THEN
        RETURN jsonb_build_object(
            'type', 'error',
            'message', 'Email already exists',
            'code', 'EMAIL_EXISTS'
        );
    END IF;

    -- Create user
    INSERT INTO users (email, name)
    VALUES (input->>'email', input->>'name')
    RETURNING id INTO new_user_id;

    -- Return success with the created user
    RETURN jsonb_build_object(
        'type', 'success',
        'user', (
            SELECT data FROM user_profile
            WHERE data->>'id' = new_user_id::text
        ),
        'message', 'User created successfully'
    );
END;
$$ LANGUAGE plpgsql;
```

### 4. Create Your App

```python
from fraiseql import create_fraiseql_app

app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[User],
    mutations=[CreateUser],
)

# Run with: uvicorn app:app --reload
```

### 5. Query Your API

Visit `http://localhost:8000/playground` for the GraphQL Playground, or send queries to `/graphql`:

```graphql
mutation {
  createUser(input: { email: "alice@example.com", name: "Alice" }) {
    ... on CreateUserSuccess {
      user {
        id
        email
        name
        postCount
      }
      message
    }
    ... on CreateUserError {
      message
      code
    }
  }
}
```

## CQRS Philosophy

FraiseQL embraces CQRS by clearly separating:

### Queries (Read Side)

- Use PostgreSQL views for data composition
- Leverage database query optimization
- Support materialized views or projection tables for performance
- Enable complex aggregations at the database level

### Mutations (Write Side)

- PostgreSQL functions contain business logic
- Transactional consistency guaranteed
- Return structured success/error responses
- Support complex workflows and validations

This separation provides several benefits:

- **Performance**: Optimized read models without affecting write logic
- **Scalability**: Read and write sides can be scaled independently
- **Maintainability**: Business logic lives in one place (database functions)
- **Flexibility**: Views can be optimized without changing mutation logic

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
    jsonb_build_object(
        'id', p.id,
        'title', p.title,
        'content', p.content,
        'author', (
            SELECT data FROM user_profile
            WHERE data->>'id' = p.author_id::text
        ),
        'comments', (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'id', c.id,
                    'content', c.content,
                    'author', (
                        SELECT data FROM user_profile
                        WHERE data->>'id' = c.author_id::text
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
    ) as data
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

## TestFoundry: Database Test Generation

TestFoundry helps generate comprehensive database tests:

```python
from fraiseql.extensions.testfoundry import FoundryGenerator

# Generate pgTAP tests for your mutations
generator = FoundryGenerator(repository)
tests = await generator.generate_tests_for_entity(
    entity_name="users",
    table_name="tb_users",
    input_type_name="user_input"
)
```

This generates tests for:

- Valid operations (happy path)
- Constraint violations (unique, foreign key, check)
- Edge cases and boundary conditions
- Authorization rules

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
SELECT jsonb_build_object(
    'id', id,
    'name', name,
    'price', price,
    'in_stock', quantity > 0
) as data
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
- Podman or Docker (for testing)

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

# Run tests
TESTCONTAINERS_PODMAN=true pytest
```

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
