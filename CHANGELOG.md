# Changelog

All notable changes to FraiseQL will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[0.1.0a11]: https://github.com/fraiseql/fraiseql/compare/v0.1.0a10...v0.1.0a11
[0.1.0a10]: https://github.com/fraiseql/fraiseql/releases/tag/v0.1.0a10
