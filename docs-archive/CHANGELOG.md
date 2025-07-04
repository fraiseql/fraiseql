# Changelog

All notable changes to FraiseQL will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0a11] - 2025-06-22

### Added
- **Unified Container Testing Architecture**: Revolutionary approach to database testing
  - Single PostgreSQL container shared across entire test session
  - 5-10x faster test execution compared to per-test containers
  - Automatic Podman/Docker runtime detection
  - Socket-based communication for optimal performance
  - Tests automatically skip when no container runtime available

### Changed
- **GraphQL Interface Updates**: Enhanced developer experience
  - Replaced GraphQL Playground with GraphiQL 2.0 or Apollo Sandbox options
  - CamelCase field conversion now enabled by default for better GraphQL compatibility
  - Improved SQL helper with cleaner dictionary comprehension

### Fixed
- **SQL WHERE Generation**: Critical fixes for complex queries
  - Fixed array containment operator for list fields (e.g., `tags_contains` now uses JSONB `@>`)
  - Fixed nested field access with double underscore notation (e.g., `profile__city`)
  - Improved type detection for Union types in Python 3.10+ syntax
- **Testing Infrastructure**: Enhanced reliability
  - Fixed security tests for CSRF protection and rate limiting
  - Updated Prometheus metrics tests to use proper API
  - Fixed async event loop handling in tests
  - Achieved 100% linting compliance across codebase

### Developer Experience
- Comprehensive container runtime documentation
- Improved test isolation and reliability
- Better error messages for container-related issues
- Simplified test execution with automatic runtime detection

## [0.1.0a10] - 2025-06-21

### Changed
- Major test infrastructure improvements
- Enhanced CI/CD pipeline reliability

## [0.1.0a7] - 2025-06-19

### Added
- N+1 query detection in development mode
- Comprehensive Strawberry migration support
- Full JSON/dict type support in GraphQL schemas

## [0.1.0a6] - 2025-06-18

### Added
- WebSocket subscriptions support
- Query registration patterns documentation
- Enhanced examples and migration guides

## [0.1.0a5] - 2025-06-17

### Added
- **Comprehensive Documentation**: Based on user feedback from pgGit demo integration
  - Quick Start Guide with 5-minute getting started tutorial
  - Complete API Reference documenting all decorators and functions
  - Troubleshooting Guide addressing common issues
  - Working examples including full pgGit demo implementation

### Fixed
- **Documentation Gaps**: Addressed critical DX issues reported by users
  - Clarified that `fraiseql.build_schema()` doesn't exist (use `create_fraiseql_app()`)
  - Added clear examples showing correct API usage
  - Documented how to enable GraphQL Playground
  - Provided immediately runnable example code

### Developer Experience
- Added documentation links to README for better discoverability
- Created minimal 50-line example for quick testing
- Improved error messages guidance in troubleshooting docs
- Added common patterns and integration examples

## [0.1.0a4] - 2025-06-17

### Added
- **@dataloader_field Decorator**: Production-ready decorator for automatic DataLoader integration
  - Eliminates N+1 queries with zero boilerplate code
  - Auto-implements DataLoader-based field resolution
  - Full type safety with return type conversion
  - Seamless GraphQL schema integration
  - Comprehensive input validation and security hardening
  - Support for `Optional[Type]`, `Type.from_dict()`, and `Type(**data)` patterns
  - Production-grade error handling with sanitized error messages

### Security
- **Enhanced DataLoader Security**: Comprehensive security improvements
  - Input validation to prevent attribute injection attacks
  - Field existence validation before access
  - Type validation for hashable keys only
  - Sanitized error messages to prevent information disclosure
  - Safe exception handling with internal logging
  - Memory leak prevention in LoaderRegistry cleanup

### Performance
- **DataLoader Concurrency Fixes**: Improved batch processing reliability
  - Fixed race conditions in high-concurrency scenarios
  - Safer event loop context switching
  - Proper queue state management
  - Memory leak prevention with forced cleanup

### Fixed
- DataLoader error handling now uses sanitized exceptions for security
- Type construction safety improvements
- Proper memory management in long-running applications

## [0.1.0a3] - 2025-06-17

### Added
- **WebSocket Subscriptions**: Complete production-ready implementation
  - Support for both `graphql-ws` and `graphql-transport-ws` protocols
  - Full connection lifecycle management with proper state transitions
  - Keep-alive mechanism with configurable ping/pong intervals
  - Error handling with appropriate WebSocket close codes
  - Broadcasting capability for multi-connection scenarios
  - FastAPI integration with working HTML/JavaScript example
- **Query Registration Patterns**: Comprehensive documentation and examples
  - Clarified that `@fraiseql.query` decorator already works perfectly
  - Added migration guide showing all three registration approaches
  - Created examples demonstrating decorator, QueryRoot, and explicit patterns
  - Comprehensive test coverage proving all patterns work together
- **Enhanced Examples**:
  - `examples/websocket_fastapi.py` - Working WebSocket subscription demo
  - `examples/query_patterns/` - All query registration patterns demonstrated
  - Updated blog API example to use clean `@query` decorator pattern
- **Documentation**:
  - `docs/migration/query-registration.md` - Complete migration guide
  - Enhanced user exploration notes with Grumpy's assessment

### Fixed
- Query registration confusion - documented that auto-registration works at import time
- WebSocket connection cleanup and proper task cancellation
- Import patterns for query modules to ensure decorator registration

### Technical Details
- Added `WebSocketError` exception class
- Enhanced `SubscriptionManager` with connection broadcasting
- Complete WebSocket message handling for GraphQL protocols
- 16 comprehensive WebSocket tests covering all scenarios
- 6 query registration tests proving pattern compatibility

## [0.1.0a2] - 2025-01-16

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
