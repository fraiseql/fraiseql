# Changelog

All notable changes to FraiseQL will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Default Schema Configuration** - Configure default PostgreSQL schemas for mutations and queries once in FraiseQLConfig
  - Added `default_mutation_schema` and `default_query_schema` configuration options
  - Eliminates repetitive `schema="app"` parameters on every decorator
  - Maintains full backward compatibility with explicit schema overrides
  - Reduces boilerplate in mutation-heavy applications by 90%

### Changed
- Default schema for mutations changed from "graphql" to "public" when no config is provided
  - This aligns with PostgreSQL conventions and simplifies getting started
  - Existing code with explicit schema parameters is unaffected

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
