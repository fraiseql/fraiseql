# Changelog

All notable changes to FraiseQL will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] - 2025-01-17

### Security
- **Breaking Change**: Authentication is now properly enforced when an auth provider is configured
  - Previously, configuring `auth_enabled=True` did not block unauthenticated requests (vulnerability)
  - Now, when an auth provider is passed to `create_fraiseql_app()`, authentication is automatically enforced
  - All GraphQL requests require valid authentication tokens (401 returned for unauthenticated requests)
  - Exception: Introspection queries (`__schema`) are still allowed without auth in development mode
  - This fixes a critical security vulnerability where sensitive data could be accessed without authentication

### Changed
- Passing an `auth` parameter to `create_fraiseql_app()` now automatically sets `auth_enabled=True`
- Authentication enforcement is now consistent across all GraphQL endpoints

### Fixed
- Fixed authentication bypass vulnerability where `auth_enabled=True` didn't actually enforce authentication
- Fixed inconsistent authentication behavior between different query types

### Documentation
- Added comprehensive Authentication Enforcement section to authentication guide
- Updated API reference to clarify auth parameter behavior
- Added security notices about authentication enforcement

## [0.2.1] - 2025-01-16

### Fixed
- Fixed version synchronization across all Python modules
- Updated CLI version numbers to match package version
- Updated generated project dependencies to use correct version range

## [0.2.0] - 2025-01-16

### Changed
- **Breaking Change**: CORS is now disabled by default to prevent conflicts with reverse proxies
  - `cors_enabled` now defaults to `False` instead of `True`
  - `cors_origins` now defaults to `[]` (empty list) instead of `["*"]`
  - This prevents duplicate CORS headers when using reverse proxies like Nginx, Apache, or Cloudflare
  - Applications serving browsers directly must explicitly enable CORS with `cors_enabled=True`
  - Production deployments should configure CORS at the reverse proxy level for better security

### Added
- Production warning when wildcard CORS origins are used in production environment
- Comprehensive CORS configuration examples for both reverse proxy and application-level setups
- Detailed migration guidance in documentation for existing applications

### Fixed
- Eliminated CORS header conflicts in reverse proxy environments
- Improved security by requiring explicit CORS configuration

### Documentation
- Complete rewrite of CORS documentation across all guides
- Added reverse proxy configuration examples (Nginx, Apache)
- Updated security documentation with CORS best practices
- Updated all tutorials and examples to reflect new CORS defaults
- Added migration guide for upgrading from v0.1.x

## [0.1.5] - 2025-01-15

### Added
- **Nested Object Resolution Control** - Added `resolve_nested` parameter to `@type` decorator for explicit control over nested field resolution behavior
  - `resolve_nested=False` (default): Assumes embedded data in parent object, optimal for PostgreSQL JSONB queries
  - `resolve_nested=True`: Makes separate queries to nested type's sql_source, useful for truly relational data
  - Replaces previous automatic "smart resolver" behavior with explicit developer control
  - Improves performance by avoiding N+1 queries when data is pre-embedded
  - Maintains full backward compatibility

### Changed
- **Breaking Change**: Default nested object resolution behavior now assumes embedded data
  - Previous versions automatically queried nested objects from their sql_source
  - New default behavior assumes nested data is embedded in parent JSONB for better performance
  - Use `resolve_nested=True` to restore previous automatic querying behavior
  - This change aligns with PostgreSQL-first design and JSONB optimization patterns

### Fixed
- Fixed test import errors that were causing CI failures
- Fixed duplicate GraphQL type name conflicts in test suite
- Updated schema building API usage throughout codebase

### Documentation
- Added comprehensive guide to nested object resolution patterns
- Updated examples to demonstrate both embedded and relational approaches
- Added migration guide for developers upgrading from v0.1.4

## [0.1.4] - 2025-01-12

### Added
- **Default Schema Configuration** - Configure default PostgreSQL schemas for mutations and queries once in FraiseQLConfig
  - Added `default_mutation_schema` and `default_query_schema` configuration options
  - Eliminates repetitive `schema="app"` parameters on every decorator
  - Maintains full backward compatibility with explicit schema overrides
  - Reduces boilerplate in mutation-heavy applications by 90%
  - Lazy schema resolution ensures configuration can be set after decorators are applied

### Changed
- Default schema for mutations changed from "graphql" to "public" when no config is provided
  - This aligns with PostgreSQL conventions and simplifies getting started
  - Existing code with explicit schema parameters is unaffected

### Fixed
- Fixed timing issue where mutations would resolve schema before configuration was set
  - Schema resolution is now lazy, only happening when the GraphQL schema is built
  - This ensures the feature works correctly in production environments

## [0.1.3] - 2025-01-12

### Changed
- Renamed exported error configuration constants for consistency:
  - `PrintOptimConfig` → `STRICT_STATUS_CONFIG`
  - `AlwaysDataConfig` → `ALWAYS_DATA_CONFIG`
  - `DefaultErrorConfig` → `DEFAULT_ERROR_CONFIG`
- Improved project description to better reflect its production-ready status

## [0.1.2] - 2025-01-08

### Security
- Fixed CVE-2025-4565 by pinning `protobuf>=4.25.8,<5.0`
- Fixed CVE-2025-54121 by updating `starlette>=0.47.2`
- Removed `opentelemetry-exporter-zipkin` due to incompatibility with secure protobuf versions

### Documentation
- **Major documentation overhaul** - quality score improved from 7.8/10 to 9+/10
- Fixed 15 broken internal links across documentation
- Added comprehensive guides for CQRS, Event Sourcing, Multi-tenancy, and Bounded Contexts
- Added production readiness checklist with security, performance, and deployment guidance
- Created complete deployment documentation (Docker, Kubernetes, AWS, GCP, Heroku)
- Added testing documentation covering unit, integration, GraphQL, and performance testing
- Created error handling guides with codes, patterns, and debugging strategies
- Added learning paths for different developer backgrounds
- Added acknowledgments to Harry Percival and DDD influences in README
- Fixed all table-views to database-views references for consistency
- Added missing anchor targets for deep links
- Clarified package installation instructions with optional dependencies

### Changed
- Made Redis an optional dependency (moved from core to `[redis]` extra)
- Made Zipkin exporter optional with graceful fallback and warning messages
- Fixed pyproject.toml inline comments that caused ReadTheDocs build failures

### Fixed
- Removed unnecessary docs-deploy workflow that caused CI failures
- Fixed TOML parsing issues in dependency declarations
- Added proper error handling for missing Zipkin exporter

## [0.1.1] - 2025-01-06

### Added
- Initial stable release with all beta features consolidated
- Comprehensive documentation and examples

## [0.1.0] - 2025-08-06

### Initial Public Release

FraiseQL is a lightweight, high-performance GraphQL-to-PostgreSQL query builder that uses PostgreSQL's native jsonb capabilities for maximum efficiency.

This release consolidates features developed during the beta phase (0.1.0b1 through 0.1.0b49).

#### Core Features

- **GraphQL to SQL Translation**: Automatic conversion of GraphQL queries to optimized PostgreSQL queries
- **JSONB-based Architecture**: Leverages PostgreSQL's native JSON capabilities for efficient data handling
- **Type-safe Queries**: Full Python type safety with automatic schema generation
- **Advanced Where/OrderBy Types**: Automatic generation of GraphQL input types for filtering and sorting, with support for comparison operators (_eq, _neq, _gt, _lt, _like, _in, etc.) and nested conditions (_and, _or, _not)
- **FastAPI Integration**: Seamless integration with FastAPI for building GraphQL APIs
- **Authentication Support**: Built-in Auth0 and native authentication support
- **Subscription Support**: Real-time subscriptions via WebSockets
- **Query Optimization**: Automatic N+1 query detection and dataloader integration
- **Mutation Framework**: Declarative mutation definitions with error handling
- **Field-level Authorization**: Fine-grained access control at the field level

#### Performance

- Sub-millisecond query translation
- Efficient connection pooling with psycopg3
- Automatic query batching and caching
- Production-ready with built-in monitoring

#### Developer Experience

- CLI tools for scaffolding and development
- Comprehensive test suite (2,400+ tests)
- Extensive documentation and examples
- Python code generation

#### Examples Included

- Blog API with comments and authors
- E-commerce API with products and orders
- Real-time chat application with WebSocket support
- Native authentication UI (Vue.js components)
- Security best practices implementation
- Analytics dashboard
- Query patterns and caching examples

For migration from beta versions, please refer to the documentation.

---

[0.1.2]: https://github.com/fraiseql/fraiseql/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/fraiseql/fraiseql/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/fraiseql/fraiseql/releases/tag/v0.1.0
