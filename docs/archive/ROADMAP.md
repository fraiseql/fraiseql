# FraiseQL Roadmap

This document outlines the planned development roadmap for FraiseQL.

## Current Release: v1.0.0 (2025 Q1)

### Goals
- Production-ready GraphQL framework
- High-performance Rust JSON processing
- Comprehensive PostgreSQL type support
- Enterprise-grade security features
- Full test coverage

### Status
- ✅ Core framework
- ✅ Rust JSON pipeline
- ✅ PostgreSQL type system
- ✅ Authentication & authorization
- ✅ Testing & documentation
- 🚧 Final release preparations

## v1.1.0 (2025 Q2)

### Planned Features
- Enhanced caching with `pg_fraiseql_cache` v2
- Improved DataLoader performance
- Additional PostgreSQL type support
- Better error messages and debugging
- Performance monitoring dashboard

### Community Requests
- Federation support exploration
- GraphQL Yoga compatibility
- Additional authentication providers
- Batch mutations

## v1.2.0 (2025 Q3)

### Planned Features
- WebSocket subscriptions improvements
- Real-time query updates
- Advanced CQRS patterns
- Multi-database support (read replicas)
- Enhanced audit logging

## v2.0.0 (2025 Q4)

### Major Goals
- Multi-database backend support
- Advanced federation capabilities
- Enhanced type system with custom scalars
- Improved tooling and CLI
- GraphQL Code-First enhancements

### Breaking Changes
- API refinements based on v1.x feedback
- Improved configuration system
- Enhanced type safety

## Long-term Vision

### Performance
- Continue Rust integration for critical paths
- Advanced query optimization
- Intelligent query caching
- Predictive prefetching

### Developer Experience
- Visual query builder
- Interactive schema explorer
- Better error messages
- Enhanced IDE support

### Enterprise Features
- Advanced RBAC
- Compliance tools (GDPR, SOC 2, HIPAA)
- Multi-tenancy patterns
- Advanced monitoring

### Ecosystem
- Framework integrations (Django, Flask, etc.)
- ORM compatibility
- Cloud platform support
- Managed hosting options

## Contributing

We welcome contributions! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

### Priority Areas
- Performance benchmarks
- Documentation improvements
- Real-world use cases
- Test coverage
- Bug reports

## Feedback

- [GitHub Discussions](../discussions)
- [Feature Requests](../issues/new?template=feature_request.md)
- [Roadmap Discussions](../discussions/categories/roadmap)

## Version Support

| Version | Status | Support Until |
|---------|--------|---------------|
| 1.x     | Active | TBD           |
| 0.11.x  | Maintenance | 2025-06-30 |
| < 0.11  | Unsupported | -           |

## Updates

This roadmap is reviewed quarterly and updated based on:
- Community feedback
- Performance benchmarks
- Production use cases
- Emerging GraphQL standards

Last Updated: 2025-01-15

---

For the latest updates, watch the repository and follow our [blog](https://blog.fraiseql.com).
