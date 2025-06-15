# FraiseQL Complete Integration Roadmap

## Overview

Transform FraiseQL into a complete drop-in replacement for Strawberry + FastAPI + psycopg + Auth0, where users only need to configure database access and authentication.

## Current State Analysis (Updated: January 2025)

### ✅ Completed Features

1. **GraphQL Type System** ✅
   - `@fraise_type` and `@fraise_input` decorators
   - Custom scalars (UUID, DateTime, DateRange, IPAddress, JSON, LTree, EmailAddress)
   - Automatic GraphQL schema generation
   - Interface and generic type support
   - Auto camelCase conversion

2. **SQL Generation** ✅
   - GraphQL to PostgreSQL query translation
   - JSONB-based data model with optimized field extraction
   - WHERE clause generation with type-safe filters
   - ORDER BY and pagination support
   - PostgreSQL function-based mutations with standardized results

3. **FastAPI Integration** ✅
   - Complete FastAPI app factory (`create_fraiseql_app`)
   - Development authentication middleware
   - Pydantic settings configuration
   - Environment variable support

4. **Database Layer** ✅
   - FraiseQLRepository with async connection pool support
   - Transaction management
   - Real PostgreSQL integration testing with testcontainers
   - Podman support for containerized testing

5. **Authentication Foundation** ✅
   - Base authentication provider interface
   - Development authentication for testing
   - Auth decorators (`@requires_auth`)
   - JWT-ready architecture

6. **Testing Infrastructure** ✅
   - 443 tests all passing
   - Real PostgreSQL integration tests
   - Type checking with pyright
   - Linting with ruff
   - Pre-commit hooks configured

### 🚧 In Progress / Partially Complete

1. **Production Auth0 Integration** - Base auth structure exists, needs Auth0 provider implementation
2. **Query Compilation** - Runtime compilation works, needs pre-compilation support
3. **Production Router** - Development router exists, needs optimized production variant

### 📋 TODO / Not Started

## 1. Complete Auth0 Integration

### Current State

- ✅ Base AuthProvider interface defined
- ✅ Development auth middleware for testing
- ✅ Auth decorators implemented
- ❌ Auth0Provider implementation
- ❌ JWT token validation
- ❌ Permission/role checking

### Implementation Needed

```python
# src/fraiseql/auth/auth0.py
class Auth0Provider(AuthProvider):
    def __init__(self, domain: str, api_identifier: str):
        self.domain = domain
        self.api_identifier = api_identifier
        self.jwks_client = JWKSClient(f"https://{domain}/.well-known/jwks.json")

    async def validate_token(self, token: str) -> dict:
        # Implement JWT validation with JWKS
        pass

    async def get_user_context(self, token_payload: dict) -> UserContext:
        # Extract user info, permissions, roles
        pass
```

## 2. Production Router with Query Compilation

### Requirements

- Bypass GraphQL validation for known queries
- Pre-compiled SQL templates
- Query whitelisting
- Minimal error exposure
- Performance monitoring

### Implementation Plan

```python
# src/fraiseql/fastapi/prod_router.py
class ProductionRouter:
    def __init__(self, compiled_queries: dict[str, CompiledQuery]):
        self.compiled_queries = compiled_queries

    async def execute_compiled_query(self, query_id: str, variables: dict):
        compiled = self.compiled_queries.get(query_id)
        if not compiled:
            raise QueryNotFoundError(f"Query {query_id} not found")

        # Direct SQL execution without GraphQL parsing
        sql = compiled.bind_variables(variables)
        return await self.db.execute(sql)
```

## 3. Query Compilation System

### Requirements

- Pre-compile GraphQL queries to SQL templates
- CLI tool for compilation
- Query versioning and caching
- Runtime fallback for unknown queries

### Implementation

```python
# src/fraiseql/compiler/cli.py
@click.command()
@click.option('--queries-dir', default='./queries')
@click.option('--output', default='./compiled_queries.json')
def compile_queries(queries_dir: str, output: str):
    """Pre-compile GraphQL queries to SQL."""
    compiler = QueryCompiler(schema)

    for query_file in Path(queries_dir).glob('*.graphql'):
        query = query_file.read_text()
        compiled = compiler.compile(query)

    # Output compiled queries with metadata
    save_compiled_queries(output, compiled_queries)
```

## 4. Migration System

### Requirements

- Automatic migration generation from type changes
- Version control for database schema
- Rollback support
- JSONB schema evolution

### Implementation

```python
# src/fraiseql/migrations/generator.py
class MigrationGenerator:
    def generate_from_types(self, types: list[type]) -> Migration:
        # Analyze type definitions
        # Compare with current DB schema
        # Generate ALTER TABLE statements for JSONB schema changes
        pass
```

## 5. CLI Tools

### Requirements

- Query compilation command
- Migration generation
- Type generation for frontend
- Development server

### Implementation

```python
# src/fraiseql/cli/__init__.py
@click.group()
def cli():
    """FraiseQL CLI tools."""
    pass

@cli.command()
def compile():
    """Compile GraphQL queries to SQL."""
    pass

@cli.command()
def migrate():
    """Generate database migrations."""
    pass

@cli.command()
def dev():
    """Start development server with hot reload."""
    pass
```

## 6. Monitoring & Performance

### Requirements

- Query execution metrics
- Connection pool monitoring
- Error tracking
- Performance profiling

### Implementation

```python
# src/fraiseql/monitoring.py
class MetricsCollector:
    def record_query_execution(self, query_name: str, duration: float):
        # Record to Prometheus/StatsD
        pass

    def record_pool_stats(self, active: int, idle: int):
        # Monitor connection pool health
        pass
```

## Implementation Priority

### ✅ Phase 1: Core Framework (COMPLETED)

- GraphQL type system
- SQL generation
- Basic FastAPI integration
- Database layer

### 🚧 Phase 2: Production Readiness (IN PROGRESS)

- Complete Auth0 integration
- Production router with query compilation
- Migration system

### 📋 Phase 3: Developer Experience (TODO)

- CLI tools
- Frontend type generation
- Hot reload development server

### 📋 Phase 4: Advanced Features (TODO)

- Monitoring and metrics
- Query complexity analysis
- Rate limiting
- Caching layer

## Quick Start (Current State)

```python
# Currently working example:
from fraiseql.fastapi import create_fraiseql_app
from fraiseql.types import fraise_type
from fraiseql.fields import fraise_field

@fraise_type
class User:
    id: UUID = fraise_field(purpose="Unique identifier")
    name: str = fraise_field(purpose="User's display name")
    email: str = fraise_field(purpose="Contact email")

app = create_fraiseql_app(
    title="My API",
    types=[User],
    # Development auth enabled by default
    dev_auth_enabled=True,
    dev_auth_username="admin",
    dev_auth_password="secret"
)

# Run with: uvicorn app:app --reload
```

## Next Steps

1. **Complete Auth0 Provider** - Implement JWT validation and user context
2. **Production Router** - Build optimized query execution path
3. **Query Compiler** - Create CLI tool for pre-compilation
4. **Migration Generator** - Auto-generate DB migrations from types
5. **Documentation** - Comprehensive guides and API reference
