# Changelog

All notable changes to FraiseQL will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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