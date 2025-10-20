# 🚀 Getting Started with FraiseQL

Welcome! This guide helps you find the right path based on your goals and experience level.

## Who Are You?

Choose your path below based on what you're trying to accomplish:

### 👶 **New to FraiseQL?**
**Goal**: Build your first GraphQL API quickly
**Time**: 5-15 minutes
**Experience**: Basic Python + SQL knowledge

**Start Here** → [5-Minute Quickstart](docs/quickstart.md)
- Simple todo app example
- See results immediately
- Understand the basics

**Next Steps** → [Beginner Learning Path](docs/tutorials/beginner-path.md)
- Complete 2-3 hour journey
- Learn all core concepts
- Build production-ready APIs

---

### 🏗️ **Building Production APIs?**
**Goal**: Deploy scalable GraphQL services
**Time**: 30-90 minutes
**Experience**: GraphQL + database experience

**Essential Reading**:
- [Performance Optimization](docs/performance/index.md) - 4-layer optimization stack
- [Database Patterns](docs/advanced/database-patterns.md) - Production view design
- [Production Deployment](docs/tutorials/production-deployment.md) - Docker + monitoring

**Quick Setup**:
```bash
pip install fraiseql fastapi uvicorn
fraiseql init my-production-api
cd my-production-api && fraiseql dev
```

---

### 🤝 **Contributing to FraiseQL?**
**Goal**: Help develop the framework
**Time**: Varies
**Experience**: Python + Rust development

**Developer Resources**:
- [Contributing Guide](CONTRIBUTING.md) - Development setup
- [Project Structure](PROJECT_STRUCTURE.md) - Code organization
- [Architecture Decisions](docs/architecture/decisions/) - Design rationale

**Quick Setup**:
```bash
git clone https://github.com/fraiseql/fraiseql.git
cd fraiseql
pip install -e .[dev]
make test  # Run full test suite
```

---

### 🔄 **Migrating from Other Frameworks?**
**Goal**: Switch to FraiseQL from existing GraphQL solutions
**Time**: 1-2 hours
**Experience**: Existing GraphQL knowledge

**Migration Guides**:
- [From PostGraphile/Hasura](docs/migration-guides/postgraphile-to-fraiseql.md) - Database-first migration
- [From Apollo Server](docs/migration-guides/apollo-to-fraiseql.md) - Server-side migration
- [Performance Comparison](README.md#performance-comparison) - Why FraiseQL is faster

---

## 📚 Documentation Index

### Core Concepts
- [FraiseQL Philosophy](docs/core/fraiseql-philosophy.md) - Design principles
- [Types & Schema](docs/core/types-and-schema.md) - GraphQL type system
- [Queries & Mutations](docs/core/queries-and-mutations.md) - Resolver patterns
- [Database API](docs/core/database-api.md) - Repository pattern

### Performance & Optimization
- [Performance Stack](docs/performance/index.md) - 4-layer optimization
- [Result Caching](docs/performance/caching.md) - PostgreSQL-based caching
- [Rust Acceleration](fraiseql_rs/) - JSON transformation engine

### Production & Deployment
- [Deployment Guide](docs/production/deployment.md) - Docker + Kubernetes
- [Monitoring](docs/production/monitoring.md) - PostgreSQL-native observability
- [Security](docs/production/security.md) - Production hardening

### Advanced Patterns
- [Multi-Tenancy](docs/advanced/multi-tenancy.md) - Tenant isolation
- [Authentication](docs/advanced/authentication.md) - Auth patterns
- [Database Patterns](docs/advanced/database-patterns.md) - View design

### Examples & Tutorials
- [Examples Directory](../examples/) - 20+ working applications
- [Blog API Tutorial](docs/tutorials/blog-api.md) - Complete application
- [Production Tutorial](docs/tutorials/production-deployment.md) - End-to-end deployment

### Reference
- [CLI Reference](docs/reference/cli.md) - Command-line tools
- [Configuration](docs/reference/config.md) - FraiseQLConfig options
- [Decorators](docs/reference/decorators.md) - @type, @query, @mutation

---

## 🆘 Need Help?

**Still not sure where to start?**
1. Try the [5-Minute Quickstart](docs/quickstart.md) - it's designed to work for everyone
2. Check the [Project Structure](PROJECT_STRUCTURE.md) guide
3. Browse [Examples](../examples/) for patterns similar to your use case

**Have questions?**
- 📖 [Full Documentation](docs/README.md) - Complete reference
- 💬 [GitHub Issues](https://github.com/fraiseql/fraiseql/issues) - Ask questions
- 📧 [Discussions](https://github.com/fraiseql/fraiseql/discussions) - Community help

---

## 🎯 Success Criteria

By the end of your chosen path, you should be able to:
- ✅ Understand FraiseQL's database-first architecture
- ✅ Build GraphQL APIs with sub-millisecond performance
- ✅ Deploy production applications with monitoring
- ✅ Use advanced patterns for complex applications

**Ready to start? Choose your path above!** 🚀
