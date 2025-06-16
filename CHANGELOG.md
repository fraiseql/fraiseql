# Changelog

All notable changes to FraiseQL will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0a3] - 2025-01-16

### Added
- Query registration pattern with `@query` decorator for simple function-based queries
- `context_getter` parameter to `create_fraiseql_app()` for custom GraphQL context
- Full support for `dict[str, Any]` and JSON types in GraphQL schema
- `@query` and `@field` decorators for flexible query definition patterns
- Database URL format conversion utilities (supports both postgresql:// and psycopg2 formats)
- Custom `lifespan` support for application resource management
- Comprehensive documentation:
  - Complete task management API example demonstrating all features
  - Migration guide from Strawberry GraphQL
  - Advanced features documentation (context customization, lifecycle management)
  - Complete decorators reference
  - Database URL format documentation

### Changed
- Environment variables now require `FRAISEQL_` prefix to avoid conflicts with other applications
- Mutations support both `failure` and `error` attributes for backward compatibility
- FastAPI configuration now accepts `extra="ignore"` to handle non-FraiseQL env vars

### Fixed
- Query registration without requiring QueryRoot class
- Environment variable validation conflicts with common names (ENV, DEBUG, etc.)
- Field decorator now properly handles being called with parentheses
- JSON/dict type support in GraphQL schema generation
- Database URL normalization for both URL and psycopg2 connection string formats

### Security
- **BREAKING**: Fixed critical SQL injection vulnerability in WHERE clause generation
  - Replaced string concatenation with parameterized queries using psycopg's `Composed` and `Literal` classes
  - All query operators now use proper SQL parameterization
  - Boolean values correctly converted to strings for JSONB text comparisons
  - Comprehensive test suite added to verify SQL injection prevention
  - See [Security Documentation](./docs/advanced/security.md) for details

## [0.1.0] - 2025-06-10

### Added
- Initial release of FraiseQL
- GraphQL to PostgreSQL query translation with JSONB support
- Type-safe decorators: `@fraise_type`, `@fraise_input`, `@fraise_enum`, `@fraise_interface`
- Field metadata system with `fraise_field()`
- FastAPI integration with automatic GraphQL endpoint creation
- CQRS repository pattern for database operations
- Mutation support with result unions (@success/@failure pattern)
- Built-in scalar types: UUID, DateTime, Date, JSON, EmailAddress, IPAddress
- Authentication decorators: `@requires_auth`, `@requires_role`, `@requires_permission`
- Auth0 authentication provider
- Connection/Edge/PageInfo types for GraphQL pagination
- Automatic camelCase conversion for GraphQL compatibility
- Fragment resolution in GraphQL queries
- ORDER BY and GROUP BY support
- Complex WHERE clause generation with type-safe operators
- TestFoundry extension for automated test generation
- Comprehensive test suite with pytest
- Documentation and examples

### Security
- SQL injection protection through parameterized queries
- Input validation and type checking
- Authentication and authorization support

### Performance
- Optimized JSONB queries
- Efficient subquery generation for relationships
- Connection pooling with psycopg3
- Smart field selection to minimize data transfer

[0.1.0]: https://github.com/fraiseql/fraiseql/releases/tag/v0.1.0
