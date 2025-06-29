# Changelog

All notable changes to FraiseQL will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0b1] - 2025-06-29

### 🎉 First Beta Release!

FraiseQL has reached beta status after 21 alpha releases. This release represents a stable, feature-complete GraphQL-to-PostgreSQL framework ready for production evaluation.

### Added
- **Beta stability guarantees** - API stability for core features
- **Comprehensive beta readiness assessments** documenting production readiness
- **Updated documentation** reflecting current version and features

### Changed
- **Version bump** from alpha (0.1.0a21) to beta (0.1.0b1)
- **Development status** updated to "4 - Beta" in package metadata
- **README version badge** updated to reflect beta status

### Fixed
- Auth0 test HTTP client mocking issue resolved
- Test collection warnings cleaned up
- All version references synchronized across documentation

### Summary
- **Security**: Zero vulnerabilities with comprehensive security features
- **Testing**: 315/316 non-database tests passing (99.7% pass rate)
- **Features**: Production-ready with auth, caching, monitoring, and performance optimizations
- **Documentation**: Extensive guides, examples, and API reference
- **Performance**: TurboRouter, Redis caching, and query optimization
- **Enterprise**: Multi-tenancy, Auth0 integration, OpenTelemetry support

## [0.1.0a21] - 2025-06-28

### Security
- **Resolved all security vulnerabilities** - 0 vulnerabilities (was 4)
  - Migrated benchmark containers from Debian to Alpine Linux
  - Resolved CVE-2025-27587 (Perl race condition) and CVE-2025-30258 (Kerberos memory leak)
  - Updated all benchmark dependencies to latest secure versions
  - Updated Java benchmark to Spring Boot 3.4.1 and GraphQL Java 22.3

### Added
- **Comprehensive test coverage** for critical modules
  - Extended tests for auth, repository, monitoring, security modules
  - New test utilities for introspection, IP utilities, common types
- **Project assessment framework** with date-based organization
  - 6 comprehensive assessments including quality (9.2/10) and security
  - Structured assessments/ directory for historical tracking
- **Enhanced documentation organization**
  - Centralized assessment tracking and methodology

### Fixed
- All linting issues in test files
- Re-enabled previously disabled comprehensive tests
- Test reliability and consistency improvements

### Changed
- Reorganized project assessments into structured directory
- Improved test file organization and naming

## [0.1.0a20] - 2025-06-28

### Added
- **Context Parameters for Mutations** - Native support for passing authentication context to PostgreSQL functions
  - Added `context_params` parameter to `@mutation` decorator
  - Automatic extraction of values from GraphQL context (tenant_id, user_id, etc.)
  - Support for multi-parameter PostgreSQL functions: `function(tenant_id, user_id, input_data)`
  - Enhanced database layer with `execute_function_with_context()` method
  - Works with both psycopg and asyncpg connection pools
  - Automatic handling of `UserContext` objects (extracts `user_id` when context key is "user")
  - Runtime validation of required context parameters
  - Backward compatible - existing single-parameter mutations continue to work

### Technical Details
- Enhanced `MutationDefinition` class to store and process context parameter mappings
- Added multi-parameter SQL generation with proper parameter placeholders
- Context parameters are validated at runtime with clear error messages
- Maintains type safety throughout the mutation execution process

### Example Usage
```python
@mutation(
    function="create_location",
    schema="app",
    context_params={
        "tenant_id": "input_pk_organization",
        "user": "input_created_by"
    }
)
class CreateLocation:
    input: CreateLocationInput
    success: CreateLocationSuccess
    failure: CreateLocationError
```

This addresses enterprise multi-tenant requirements where PostgreSQL functions need separate context parameters for security and audit purposes.

## [0.1.0a19] - 2025-06-28

### Fixed
- Import error in `debug.py` module - `DatabaseQuery` now imported from correct module (`fraiseql.db`)
- Import error for `get_db_pool` - now imported from `fraiseql.fastapi.dependencies`
- Boolean value comparison in SQL WHERE clauses now uses proper PostgreSQL boolean casting
- Test assertion updated to match correct boolean SQL generation

### Changed
- Test expectations for boolean values in SQL queries updated to use `::boolean` casting

## [0.1.0a18] - 2025-01-26

### Added
- **Partial object instantiation** for nested GraphQL queries
  - Allows requesting only needed fields from nested objects
  - Fixes "missing required argument" errors for unrequested fields
  - Maintains GraphQL principle of selective field querying
- New `partial_instantiation` module with smart field handling
- Support for dataclasses with `__post_init__` validation
- Marking of partial instances with `__fraiseql_partial__` attribute

### Fixed
- Nested object queries failing when not all required fields were requested
- Type instantiation attempting to validate unrequested fields

### Technical Details
- Partial instantiation only active in development mode
- Missing required fields set to `None` internally
- Works recursively for all nesting levels
- Maintains backward compatibility

## [0.1.0a17] - 2025-01-26

### Fixed
- Repository not receiving correct mode from config in dependency injection
- Type instantiation defaulting to production mode even when configured for development
- FraiseQL config now properly stored and accessed globally

### Changed
- Enhanced `build_graphql_context` to include mode for debugging
- Repository creation in `get_db()` now reads mode from stored config

## [0.1.0a16] - 2025-01-26

### Fixed
- Critical bug where custom `context_getter` was replacing default context instead of merging
- Database context was missing when using custom context getter

### Changed
- Custom context now properly merges with default context (database, user, auth)
- Custom values override defaults when keys conflict

## [0.1.0a15] - 2025-01-26

### Added
- Context getter support for FastAPI integration
  - Custom context can be provided via `context_getter` parameter
  - Async function that receives FastAPI Request object
  - Allows injecting custom data into GraphQL resolvers
- GraphQL context now includes `db`, `user`, `auth`, and custom fields

### Fixed
- Database pool initialization in development mode
- Type registry passing between modules

## [0.1.0a14] - 2025-01-26

### Added
- Development mode with automatic type instantiation
  - Set `mode="development"` in FraiseQL config for rich object responses
  - Production mode (default) returns raw dicts for performance
- TurboRouter for optimized production queries
  - Caches parsed queries and execution plans
  - Reduces overhead for repeated queries
- Type registry system for view-to-type mapping
- Mode-specific query execution paths

### Changed
- Repository now supports both dict and typed object returns based on mode
- FastAPI integration automatically configures mode from FraiseQL settings

### Performance
- Production mode optimized for minimal overhead
- Development mode provides better DX with automatic type instantiation

## [0.1.0a13] - 2025-01-25

### Added
- UUID support in SQL query generation
  - Automatic handling of UUID types in WHERE clauses
  - Proper comparison operators for UUID fields
- Enhanced type detection for UUID fields
- Test coverage for UUID-based queries

### Fixed
- UUID fields now properly cast in SQL comparisons
- Type hints correctly identify UUID fields

## [0.1.0a12] - 2025-01-22

### Added
- Asyncpg support for high-performance async PostgreSQL connections
  - Automatic detection of asyncpg vs psycopg pools
  - Seamless compatibility with existing code
  - JSON codec configuration for asyncpg
- Enhanced connection pool compatibility layer

### Changed
- `execute_function` now works with both psycopg and asyncpg pools
- Database operations automatically adapt to pool type

### Performance
- Asyncpg provides significant performance improvements for high-concurrency scenarios
- Connection pooling optimized for both drivers

## [0.1.0a11] - 2025-01-21

### Added
- CQRS (Command Query Responsibility Segregation) pattern support
  - New `CQRSRepository` base class for clean separation of commands and queries
  - `CQRSExecutor` for executing SQL functions and view queries
  - Built-in pagination support for queries
  - Automatic view and function name generation based on entity types

### Changed
- Repository pattern now supports CQRS architecture
- Enhanced support for PostgreSQL views and functions

### Example
```python
from fraiseql.cqrs import CQRSRepository

# Commands use SQL functions
result = await repo.create("user", {"name": "John", "email": "john@example.com"})

# Queries use SQL views  
users = await repo.find_by_view("vw_active_users", limit=10)
```

## [0.1.0a10] - 2025-01-16

### Added
- FastAPI factory pattern with `create_app()` function
  - Clean separation of app creation and configuration
  - Support for custom routers and middleware
  - Built-in CORS, compression, and security headers

### Changed
- FastAPI integration now uses factory pattern instead of direct instantiation
- Configuration is passed to factory function

### Example
```python
from fraiseql.fastapi import create_app

app = create_app(
    title="My GraphQL API",
    cors_origins=["https://myapp.com"],
    graphql_path="/graphql"
)
```

## [0.1.0a9] - 2025-01-14

### Added
- Query complexity analysis and limiting
  - Prevent expensive queries from overloading the database
  - Configurable complexity limits
  - Automatic complexity calculation based on query depth and breadth
- `ComplexityValidator` with customizable scoring rules
- Integration with FastAPI error handling

### Changed
- GraphQL execution now validates query complexity before execution
- Default complexity limit of 100 (configurable)

## [0.1.0a8] - 2025-01-13

### Added
- Production-ready features:
  - Built-in authentication with Auth0 integration
  - Comprehensive error handling and logging
  - Health checks and monitoring endpoints
  - Request ID tracking and correlation
  - Security headers and CORS configuration
- Prometheus metrics integration
- Structured logging with contextual information

### Changed
- FastAPI integration now includes production middleware by default
- Error responses follow consistent JSON structure

## [0.1.0a7] - 2025-01-10

### Added
- Relationship loading with automatic JSON aggregation
  - Support for one-to-many relationships via `json_agg()`
  - Efficient single-query relationship loading
  - Proper NULL handling in aggregations
- Enhanced SQL builder for complex JOINs
- Support for nested relationship queries

### Fixed
- NULL handling in JSON aggregations
- Relationship loading for empty collections

## [0.1.0a6] - 2025-01-08

### Added
- Data validation framework
  - Decorator-based validation rules
  - Built-in validators (required, string length, email, UUID, etc.)
  - Custom validator support
  - Async validation support
  - Integration with GraphQL mutations
- Comprehensive error messages with field-level details

### Example
```python
@validate({
    "email": [required(), email()],
    "age": [required(), min_value(0), max_value(120)]
})
async def create_user(input: dict) -> dict:
    # Validation runs automatically
    return await db.create("user", input)
```

## [0.1.0a5] - 2025-01-06

### Added
- GraphQL subscription support
  - Real-time updates via WebSocket
  - PostgreSQL LISTEN/NOTIFY integration  
  - Automatic reconnection handling
  - Subscription filtering and transformation
- WebSocket endpoint for GraphQL subscriptions
- Connection lifecycle management

### Changed
- FastAPI integration now includes WebSocket support
- Database connection enhanced with pub/sub capabilities

## [0.1.0a4] - 2025-01-04

### Added
- Advanced SQL generation features:
  - Complex nested boolean logic (AND/OR combinations)
  - BETWEEN operator for range queries
  - Date/time filtering with timezone support
  - Case-insensitive pattern matching
  - NULL handling with `is_null` operator
- SQL injection protection via parameterized queries
- Comprehensive test coverage for all SQL operations

### Fixed
- Boolean value handling in WHERE clauses
- Timezone-aware datetime comparisons

## [0.1.0a3] - 2025-01-02

### Added
- FastAPI integration with automatic GraphQL endpoint setup
- Connection pooling with configurable settings
- Health check endpoints
- GraphQL playground (GraphiQL) in development mode
- Automatic schema introspection
- Context injection for database access in resolvers

### Changed
- Simplified API for FastAPI integration
- Database configuration now supports connection pooling

### Example
```python
from fraiseql.fastapi import get_app
app = get_app()  # FastAPI app with GraphQL endpoint at /graphql
```

## [0.1.0a2] - 2024-12-30

### Added
- Pagination support with limit/offset
- Sorting capabilities with multiple sort fields
- Filtering with various operators (eq, ne, gt, lt, gte, lte, like, in)
- Nested field selection in GraphQL queries
- Fragment support for query reuse
- Custom SQL table mapping via `sql_source` parameter

### Changed
- Enhanced SQL builder with support for complex WHERE clauses
- Improved type system with better error messages

### Fixed
- Edge cases in nested query resolution
- SQL injection vulnerabilities in query building

## [0.1.0a1] - 2024-12-28

### Added
- Initial alpha release
- Core GraphQL to SQL translation engine
- `@fraise_type` decorator for GraphQL type definition
- Automatic SQL query generation from GraphQL queries
- JSONB-based data storage support
- Basic CRUD operations via GraphQL
- PostgreSQL integration with psycopg3
- Type-safe query building

### Example
```python
@fraise_type
@dataclass
class User:
    id: str
    name: str
    email: str

# Generates SQL: SELECT jsonb_build_object('id', data->>'id', 'name', data->>'name') 
# FROM users WHERE data->>'email' = 'user@example.com'
```

[0.1.0a19]: https://github.com/fraiseql/fraiseql/releases/tag/v0.1.0a19
[0.1.0a18]: https://github.com/fraiseql/fraiseql/releases/tag/v0.1.0a18
[0.1.0a17]: https://github.com/fraiseql/fraiseql/releases/tag/v0.1.0a17
[0.1.0a16]: https://github.com/fraiseql/fraiseql/releases/tag/v0.1.0a16
[0.1.0a15]: https://github.com/fraiseql/fraiseql/releases/tag/v0.1.0a15
[0.1.0a14]: https://github.com/fraiseql/fraiseql/releases/tag/v0.1.0a14
[0.1.0a13]: https://github.com/fraiseql/fraiseql/releases/tag/v0.1.0a13
[0.1.0a12]: https://github.com/fraiseql/fraiseql/releases/tag/v0.1.0a12
[0.1.0a11]: https://github.com/fraiseql/fraiseql/releases/tag/v0.1.0a11
[0.1.0a10]: https://github.com/fraiseql/fraiseql/releases/tag/v0.1.0a10
[0.1.0a9]: https://github.com/fraiseql/fraiseql/releases/tag/v0.1.0a9
[0.1.0a8]: https://github.com/fraiseql/fraiseql/releases/tag/v0.1.0a8
[0.1.0a7]: https://github.com/fraiseql/fraiseql/releases/tag/v0.1.0a7
[0.1.0a6]: https://github.com/fraiseql/fraiseql/releases/tag/v0.1.0a6
[0.1.0a5]: https://github.com/fraiseql/fraiseql/releases/tag/v0.1.0a5
[0.1.0a4]: https://github.com/fraiseql/fraiseql/releases/tag/v0.1.0a4
[0.1.0a3]: https://github.com/fraiseql/fraiseql/releases/tag/v0.1.0a3
[0.1.0a2]: https://github.com/fraiseql/fraiseql/releases/tag/v0.1.0a2
[0.1.0a1]: https://github.com/fraiseql/fraiseql/releases/tag/v0.1.0a1