# FraiseQL Documentation Index

## 🚀 Getting Started

### First Time Users
1. **[Quick Start Guide](./getting-started/quickstart.md)** - Create your first API in 5 minutes
2. **[Installation](./getting-started/installation.md)** - Setup instructions
3. **[First API Tutorial](./getting-started/first-api.md)** - Step-by-step tutorial

### Understanding FraiseQL
1. **[Architecture Overview](./ARCHITECTURE.md)** - How FraiseQL works
2. **[Core Concepts](./core-concepts/index.md)** - Key ideas and patterns
3. **[Why FraiseQL?](./index.md)** - Benefits and use cases

## 📖 Core Documentation

### API Reference 🆕
- **[Complete Decorator Reference](./api-reference/decorators-complete.md)** - All decorators with examples
- **[Repository API](./api-reference/repository.md)** - Database operations guide  
- **[Context Reference](./api-reference/context.md)** - Understanding GraphQL context
- **[Quick API Reference](./API_REFERENCE_QUICK.md)** - Cheat sheet format

### Patterns & Best Practices 🆕
- **[Query Patterns](./patterns/queries.md)** - The one true query pattern
- **[Database Patterns](./patterns/database.md)** - JSONB data column explained
- **[Error Handling](./patterns/error-handling.md)** - Common errors and fixes
- **[Common Patterns](./COMMON_PATTERNS.md)** - Real-world patterns

## 🎓 Learning Path

### Beginner
1. [Hello World (No Database)](./tutorials/01-hello-world.md)
2. [First Database Query](./tutorials/02-database-basics.md)
3. [Adding Authentication](./tutorials/03-authentication.md)

### Intermediate
1. [Complex Queries](./tutorials/04-complex-queries.md)
2. [Mutations Guide](./mutations/index.md)
3. [Pagination Tutorial](./advanced/pagination.md)

### Advanced
1. [Production Patterns](./tutorials/05-production.md)
2. [Performance Optimization](./advanced/performance.md)
3. [Multi-Tenant Apps](./advanced/multi-tenant.md)

## 🔧 Topic Guides

### Database
- [JSONB Pattern Migration](./MIGRATION_TO_JSONB_PATTERN.md)
- [Database Views](./core-concepts/database-views.md)
- [PostgreSQL Functions](./mutations/postgresql-function-based.md)

### Authentication & Security
- [Authentication Setup](./advanced/authentication.md)
- [Security Best Practices](./advanced/security.md)
- [Context Customization](./advanced/context-customization.md)

### Performance
- [N+1 Query Prevention](./advanced/eliminating-n-plus-one.md)
- [DataLoader Integration](./DATALOADER.md)
- [TurboRouter](./advanced/turbo-router.md)

### Testing
- [Unified Container Testing](./testing/unified-container-testing.md)
- [Test Suite Setup](./testing/complete-test-suite-setup.md)
- [CI Configuration](./testing/ci-database-configuration.md)

## 🚨 Troubleshooting

### Quick Fixes
- **['NoneType' has no attribute 'context'](./patterns/error-handling.md#nonetype-object-has-no-attribute-context)** - Wrong query pattern
- **[View must have 'data' column](./patterns/error-handling.md#view-must-have-a-data-column)** - JSONB pattern issue
- **[Connection closed errors](./patterns/error-handling.md#connection-already-closed)** - Repository usage

### Common Issues
- [Query Returns None](./patterns/error-handling.md#query-returns-nonenull)
- [Import Errors](./patterns/error-handling.md#import-errors)
- [Authentication Errors](./patterns/error-handling.md#authentication-errors)

### General Help
- [Troubleshooting Guide](./TROUBLESHOOTING.md)
- [FAQ](./faq.md)
- [GitHub Issues](https://github.com/fraiseql/fraiseql/issues)

## 💡 Examples

### Complete Applications
- [Blog API](../examples/blog_api/) - Full-featured blog with auth
- [E-commerce API](../examples/ecommerce_api/) - Shopping cart and orders
- [Real-time Chat](../examples/real_time_chat/) - WebSocket subscriptions

### Code Patterns
- [Query Examples](../examples/query_patterns/)
- [Mutation Examples](../examples/mutations_demo/)
- [Authentication Examples](../examples/security/)

## 🔄 Migration Guides

- [From Strawberry](./migration/from-strawberry.md)
- [From Hasura/PostGraphile](./migration/from-hasura-postgraphile.md)
- [JSONB Migration](./MIGRATION_TO_JSONB_PATTERN.md)

## 📚 Reference

### Configuration
- [Environment Variables](./configuration.md)
- [Application Setup](./api-reference/application.md)
- [Advanced Configuration](./advanced/configuration.md)

### Types & Schema
- [Type System](./core-concepts/type-system.md)
- [Custom Scalars](./type-system.md)
- [Schema Building](./schema-building.md)

### Deployment
- [Docker Deployment](./deployment/docker.md)
- [Kubernetes Guide](./deployment/kubernetes.md)
- [Monitoring Setup](./deployment/monitoring.md)

## 🎯 Quick Links by Role

### For Backend Developers
1. [API Reference](./api-reference/index.md)
2. [Query Patterns](./patterns/queries.md)
3. [Database Patterns](./patterns/database.md)

### For Frontend Developers
1. [GraphQL Playground](./getting-started/graphql-playground.md)
2. [Query Examples](./QUERY_PATTERNS.md)
3. [Error Handling](./patterns/error-handling.md)

### For DevOps
1. [Deployment Guides](./deployment/)
2. [Configuration](./configuration.md)
3. [Monitoring](./deployment/monitoring.md)

### For Architects
1. [Architecture Overview](./ARCHITECTURE.md)
2. [Performance Analysis](./advanced/performance-comparison.md)
3. [LLM-Native Design](./advanced/llm-native-architecture.md)

## 📈 What's New

### Latest Features (v0.1.0a18)
- [Partial Object Instantiation](./PARTIAL_INSTANTIATION.md)
- [WHERE Types](./WHERE_TYPES.md)
- [Enhanced Error Messages](./patterns/error-handling.md)

### Recent Improvements
- Comprehensive API documentation
- Pattern-based guides
- Troubleshooting quick fixes
- Complete examples

## Need Help?

1. **Start Here**: [Query Patterns](./patterns/queries.md) - Most issues stem from not understanding this
2. **Check Errors**: [Error Guide](./patterns/error-handling.md) - Quick fixes for common problems
3. **Learn Patterns**: [Common Patterns](./COMMON_PATTERNS.md) - Real-world solutions
4. **Ask Community**: [GitHub Discussions](https://github.com/fraiseql/fraiseql/discussions)