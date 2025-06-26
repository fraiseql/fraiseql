# Changelog

All notable changes to FraiseQL will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
- **Where type support** in FraiseQLRepository with automatic SQL type casting
  - Support for all comparison operators: `eq`, `neq`, `gt`, `gte`, `lt`, `lte`, `in`, `contains`, `startswith`, `matches`, `isnull`
  - Automatic type casting for JSONB comparisons (numeric, timestamp, date, boolean)
  - Integration with PrintOptim's elegant filter pattern
- Enhanced `_build_find_query()` to handle where objects with `to_sql()` method
- Type registry with `register_type_for_view()` for development mode

### Fixed
- SQL type errors when comparing JSONB text fields with numeric/boolean values
- Boolean comparisons now properly cast to `::boolean`

### Technical Details
- Where objects can mix with regular kwargs filters
- Supports complex nested conditions via where types
- Maintains SQL injection safety with parameterized queries

## [0.1.0a14] - 2025-01-25

### Changed
- **BREAKING**: Repository instantiation now exclusively uses JSONB `data` column pattern
  - All type instantiation comes from a `data` JSONB column in the database
  - Other columns (id, tenant_id, etc.) are used only for filtering and access control
  - Removed backward compatibility with row-level field instantiation
- Simplified `_instantiate_from_row()` to always expect a `data` column
- Aligned with PrintOptim architecture for consistency

### Technical Details
- Tables/views must have a `data` column (JSONB) containing the complete object representation
- Single type definition per entity (no more dual-type pattern)
- Cleaner separation between data storage and access control
- KISS principle: one pattern, no fallbacks

## [0.1.0a13] - 2025-01-25

### Added
- Dual-mode repository instantiation feature for development/production environments
  - Development mode: Full recursive instantiation of typed objects for better DX
  - Production mode: Raw dictionary data with zero overhead for maximum performance
- New repository methods: `find()` and `find_one()` with mode-aware returns
- Automatic type conversion for UUID and datetime fields in development mode
- Circular reference detection and caching to handle complex object graphs
- Maximum recursion depth protection (10 levels)
- CamelCase to snake_case field name conversion
- Environment-based mode detection via `FRAISEQL_ENV` variable
- Per-request mode override through context

### Changed
- `FraiseQLRepository` constructor now accepts optional context parameter
- Repository can operate in two modes based on environment or context settings

### Technical Details
- Mode detection priority: context override > environment variable > default (production)
- Comprehensive test coverage with 11 unit tests
- Zero breaking changes - existing code continues to work unchanged

## [0.1.0a12] - 2025-01-24

### Added
- Comprehensive test suite for GET /graphql endpoint behavior
- Tests for playground serving, query execution, error handling, and security

### Fixed
- Apollo Sandbox now properly served at GET /graphql when no query parameter is provided
- GET /graphql endpoint now serves configured playground (GraphiQL or Apollo Sandbox) when accessed from browser without query
- Removed deprecated /playground endpoint

### Changed
- Unified GraphQL endpoint behavior - GET /graphql serves dual purpose:
  - Without query parameter: serves configured playground UI
  - With query parameter: executes GraphQL query
- Follows standard GraphQL server conventions for better developer experience

## [0.1.0a11] - 2025-06-23

### Added
- OpenTelemetry tracing support as an optional feature (`pip install fraiseql[tracing]`)
- Unified container testing system with Docker and Podman support
- Security policy (SECURITY.md) with clear scope definition
- Dependabot configuration for automated dependency updates
- Test script for Podman users (`scripts/test_with_podman.sh`)

### Fixed
- Critical SQL injection prevention in WHERE clause generation (security fix)
- Python 3.11+ compatibility issue with wrapt dependency
- OpenTelemetry tracing robustness when spans are missing
- Black formatting compliance in all test files
- PostgreSQL container fixture yielding issue in tests
- CI/CD pipeline stability issues

### Changed
- Improved error handling in tracing modules
- Enhanced test isolation with unified container approach
- Updated GitHub Actions to install optional dependencies

### Security
- Fixed SQL injection vulnerabilities in query generation
- All queries now use proper parameterization
- Comprehensive test coverage for injection attacks

## [0.1.0a10] - 2025-01-20

### Added
- Initial alpha release
- GraphQL schema generation from Python type annotations
- Automatic SQL query generation with JSONB support
- FastAPI integration
- Basic authentication support
- PostgreSQL connection pooling
- Query optimization with TurboRouter
- Subscription support (experimental)

[0.1.0a18]: https://github.com/fraiseql/fraiseql/compare/v0.1.0a17...v0.1.0a18
[0.1.0a17]: https://github.com/fraiseql/fraiseql/compare/v0.1.0a16...v0.1.0a17
[0.1.0a16]: https://github.com/fraiseql/fraiseql/compare/v0.1.0a15...v0.1.0a16
[0.1.0a15]: https://github.com/fraiseql/fraiseql/compare/v0.1.0a14...v0.1.0a15
[0.1.0a14]: https://github.com/fraiseql/fraiseql/compare/v0.1.0a13...v0.1.0a14
[0.1.0a13]: https://github.com/fraiseql/fraiseql/compare/v0.1.0a12...v0.1.0a13
[0.1.0a12]: https://github.com/fraiseql/fraiseql/compare/v0.1.0a11...v0.1.0a12
[0.1.0a11]: https://github.com/fraiseql/fraiseql/compare/v0.1.0a10...v0.1.0a11
[0.1.0a10]: https://github.com/fraiseql/fraiseql/releases/tag/v0.1.0a10
