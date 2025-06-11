# FraiseQL vs Alternatives

This guide compares FraiseQL with other GraphQL and API solutions to help you choose the right tool for your project.

## Overview

FraiseQL occupies a unique position in the GraphQL ecosystem:
- **Python-native** with type safety
- **Database-centric** architecture (business logic in PostgreSQL)
- **Code-first** approach with decorators
- **Performance-optimized** with production mode and TurboRouter
- **PostgreSQL-only** for maximum optimization

## Direct Competitors

### Hasura

**What it is**: Auto-generated GraphQL API from database schema with real-time subscriptions.

**Similarities**:
- PostgreSQL-first approach
- Excellent performance
- Production-ready
- Handles relationships well

**Differences**:
- Written in Haskell (vs Python)
- Configuration-based (vs code-based)
- Business logic via webhooks/actions (vs PostgreSQL functions)
- More features (subscriptions, federation, caching)

**Choose Hasura when**:
- You need GraphQL subscriptions
- You prefer configuration over code
- You want automatic CRUD generation
- You need multi-database support
- Enterprise features are important

**Choose FraiseQL when**:
- You have a Python team
- You want business logic in PostgreSQL
- You prefer code-first development
- You need maximum performance with minimal overhead

### PostGraphile

**What it is**: Instant GraphQL API from PostgreSQL schema with powerful plugin system.

**Similarities**:
- PostgreSQL-centric
- Uses database functions and views
- Excellent performance
- RLS (Row Level Security) support

**Differences**:
- Written in Node.js/TypeScript (vs Python)
- Schema introspection-based (vs explicit types)
- Plugin architecture (vs built-in features)
- Automatic CRUD generation

**Choose PostGraphile when**:
- You have a Node.js team
- You want automatic API from database schema
- You need a rich plugin ecosystem
- You prefer database-first design

**Choose FraiseQL when**:
- Python is your primary language
- You want explicit type definitions
- You prefer simpler architecture
- You need Python-specific integrations

### Prisma + Nexus/Pothos

**What it is**: Type-safe ORM with GraphQL schema builders.

**Similarities**:
- Type-safe development
- Excellent developer experience
- Code-first approach

**Differences**:
- ORM-based (vs direct SQL)
- Business logic in application (vs database)
- Multi-database support (vs PostgreSQL-only)
- Requires schema migrations

**Choose Prisma when**:
- You need multiple database support
- You prefer ORM abstractions
- You're already in the Node.js ecosystem
- Database portability is important

**Choose FraiseQL when**:
- You want to leverage PostgreSQL features
- Performance is critical
- You prefer SQL over ORM abstractions
- You want business logic in the database

## Python GraphQL Frameworks

### Strawberry GraphQL

**What it is**: Modern Python GraphQL framework with type hints.

**Similarities**:
- Python with type safety
- Decorator-based API
- Async support
- Great developer experience

**Differences**:
- No database integration
- Business logic in Python
- More flexible/general purpose
- No production optimizations

**Choose Strawberry when**:
- You need maximum flexibility
- You have complex Python-specific logic
- You're integrating multiple data sources
- You want full control over execution

**Choose FraiseQL when**:
- PostgreSQL is your primary data source
- You want integrated database access
- Performance is a priority
- You prefer conventions over configuration

### Graphene-Django

**What it is**: GraphQL framework integrated with Django ORM.

**Similarities**:
- Python ecosystem
- Database integration
- Decent documentation

**Differences**:
- Django-specific
- ORM-based with N+1 problems
- Less performant
- More boilerplate

**Choose Graphene-Django when**:
- You're already using Django
- You need Django admin integration
- You want to reuse Django models
- Team knows Django well

**Choose FraiseQL when**:
- You want better performance
- You're not tied to Django
- You prefer PostgreSQL features
- You want cleaner architecture

### Ariadne

**What it is**: Schema-first Python GraphQL framework.

**Similarities**:
- Python with async support
- Clean API
- Good performance

**Differences**:
- Schema-first (vs code-first)
- No database integration
- More manual work
- SDL-based development

**Choose Ariadne when**:
- You prefer schema-first development
- You have existing GraphQL schemas
- You need maximum control
- SDL is important to your workflow

**Choose FraiseQL when**:
- You prefer code-first with decorators
- You want integrated database access
- You value conventions
- Type hints are sufficient

## Database-Centric Alternatives

### Supabase

**What it is**: Open-source Firebase alternative built on PostgreSQL.

**Similarities**:
- PostgreSQL-based
- Uses views and functions
- Good performance
- Real-time capabilities

**Differences**:
- REST API (not GraphQL)
- Full BaaS platform
- Includes auth, storage, etc.
- JavaScript-focused

**Choose Supabase when**:
- You need a complete backend platform
- REST API is sufficient
- You want built-in auth/storage
- Real-time is critical

**Choose FraiseQL when**:
- You specifically need GraphQL
- You want Python ecosystem
- You need custom business logic
- You prefer focused tools

### PostgREST

**What it is**: REST API from PostgreSQL database.

**Similarities**:
- PostgreSQL views/functions as API
- Excellent performance
- Minimal architecture
- Database-driven

**Differences**:
- REST only (no GraphQL)
- No application layer
- Configuration-based
- Limited customization

**Choose PostgREST when**:
- REST is sufficient
- You want zero application code
- Simple CRUD is enough
- Minimal architecture is priority

**Choose FraiseQL when**:
- You need GraphQL features
- You want Python extensibility
- You need complex query patterns
- Type safety is important

## Feature Comparison Matrix

| Feature | FraiseQL | Hasura | PostGraphile | Strawberry | Prisma | Supabase |
|---------|----------|--------|--------------|------------|---------|----------|
| **Language** | Python | Haskell | Node.js | Python | TypeScript | JavaScript |
| **API Type** | GraphQL | GraphQL | GraphQL | GraphQL | GraphQL | REST |
| **Business Logic** | PostgreSQL | Webhooks | PostgreSQL | Python | App Layer | PostgreSQL |
| **Performance** | Excellent* | Excellent | Excellent | Good | Good | Excellent |
| **Setup Complexity** | Low | Medium | Low | Low | Medium | Medium |
| **Database Support** | PostgreSQL | Multi-DB | PostgreSQL | Any | Multi-DB | PostgreSQL |
| **Type Safety** | Yes | Partial | Yes | Yes | Yes | Partial |
| **CRUD Generation** | No | Yes | Yes | No | Yes | Yes |
| **Subscriptions** | No | Yes | Yes | Yes | Varies | Yes |
| **Production Mode** | Yes | Yes | Yes | No | No | N/A |
| **Auth Built-in** | Pluggable | Yes | Plugin | No | No | Yes |
| **File Storage** | No | No | No | No | No | Yes |
| **Real-time** | No | Yes | Yes | Possible | No | Yes |
| **Multi-tenancy** | Manual | Yes | Manual | Manual | Manual | Yes |

*With TurboRouter enabled

## Performance Characteristics

### Request Overhead Comparison

| Framework | GraphQL Parse | SQL Generation | Execution | Total Overhead |
|-----------|---------------|----------------|-----------|----------------|
| FraiseQL + TurboRouter | 0ms | 0ms | DB only | **0.06ms** |
| FraiseQL Standard | 0.5ms | 0.1ms | DB only | **0.8ms** |
| Hasura | 0.3ms | 0.2ms | DB only | **0.5ms** |
| PostGraphile | 0.4ms | 0.3ms | DB only | **0.7ms** |
| Strawberry + ORM | 0.5ms | N/A | DB + ORM | **2-5ms** |
| Prisma | 0.5ms | N/A | DB + ORM | **3-6ms** |

### Scalability Patterns

| Framework | Scale Strategy | Bottleneck | Cost |
|-----------|---------------|------------|------|
| FraiseQL | Scale PostgreSQL | Database | Low |
| Hasura | Scale Hasura + DB | Both | Medium |
| PostGraphile | Scale Node + DB | Both | Medium |
| Strawberry | Scale Python + DB | App server | High |
| Prisma | Scale Node + DB | App server | High |

## Decision Framework

### Choose FraiseQL when you have:

✅ **Technical Requirements**:
- Python as primary language
- PostgreSQL as main database
- Performance requirements
- Complex business logic
- Type safety needs

✅ **Team Characteristics**:
- Strong SQL skills
- PostgreSQL expertise
- Preference for simplicity
- Code-first mindset

✅ **Project Needs**:
- High-performance APIs
- Complex queries
- Database-centric logic
- Minimal infrastructure

### Consider Alternatives when you need:

❌ **Hasura**: Real-time subscriptions, multi-database, visual console

❌ **PostGraphile**: Node.js team, plugin ecosystem, automatic CRUD

❌ **Strawberry**: Maximum flexibility, complex Python logic, multiple data sources

❌ **Prisma**: Database portability, ORM patterns, existing Node.js code

❌ **Supabase**: Complete backend platform, auth/storage, REST is enough

## Migration Paths

### From Strawberry to FraiseQL

```python
# Strawberry
@strawberry.type
class User:
    id: UUID
    name: str

    @strawberry.field
    async def posts(self) -> List[Post]:
        return await load_posts(self.id)

# FraiseQL
@fraiseql.type
class User:
    id: UUID
    name: str
    posts: Optional[List[Post]] = None  # Loaded via view
```

### From Hasura to FraiseQL

1. Export PostgreSQL schema
2. Create Python types matching tables
3. Replace webhooks with PostgreSQL functions
4. Migrate permissions to RLS or functions

### From Django + Graphene to FraiseQL

1. Generate PostgreSQL views from Django models
2. Create FraiseQL types
3. Move business logic to PostgreSQL functions
4. Replace Django ORM with direct queries

## Conclusion

FraiseQL is ideal for teams that:
- Use Python and PostgreSQL
- Value performance and simplicity
- Prefer database-centric architecture
- Want type safety without complexity

It may not be the best choice if you:
- Need GraphQL subscriptions
- Require multiple database support
- Want automatic CRUD generation
- Need a full backend platform

The key is matching the tool to your specific needs, team skills, and architectural preferences.
