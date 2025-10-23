# 🚀 Getting Started with FraiseQL

Welcome! This guide helps you find the right path based on your goals and experience level.

## 🏁 Quick Start Options

**New here?** Start with our progressive [First Hour Guide](docs/FIRST_HOUR.md) - from zero to production patterns in 60 minutes!

### Visual Learning Path
```
👶 ABSOLUTE BEGINNER (0-60 min)
├── 0-5 min: [5-Minute Quickstart](docs/quickstart.md)
├── 5-15 min: [Understanding FraiseQL](docs/UNDERSTANDING.md)
├── 15-30 min: Extend your API (add features)
├── 30-45 min: Add mutations (write operations)
└── 45-60 min: Production patterns (timestamps, etc.)

🏗️ PRODUCTION BUILDER (30-90 min)
├── [Performance Optimization](docs/performance/index.md)
├── [Database Patterns](docs/advanced/database-patterns.md)
└── [Production Deployment](docs/tutorials/production-deployment.md)

🤝 CONTRIBUTOR (varies)
└── [Contributing Guide](CONTRIBUTING.md)
```

## Who Are You?

Choose your path below based on what you're trying to accomplish:

### 👶 **New to FraiseQL?**
**Goal**: Build your first GraphQL API and learn progressively
**Time**: 5 minutes to 1 hour
**Experience**: Basic Python + SQL knowledge

**🎯 Recommended: Complete Learning Path**
**[📚 First Hour Guide](docs/FIRST_HOUR.md)** - Progressive 60-minute tutorial
- Start with 5-minute quickstart
- Learn core concepts as you build
- Add features, mutations, and production patterns
- Perfect for absolute beginners

**⚡ Just Want to Try It?**
**[5-Minute Quickstart](docs/quickstart.md)** - Instant working API
- Copy-paste commands
- Working GraphQL API in 5 minutes
- No assumptions about your knowledge

**📖 Want to Understand First?**
**[Understanding FraiseQL](docs/UNDERSTANDING.md)** - 10-minute architecture overview
- Visual diagrams of how it works
- Why database-first GraphQL matters
- CQRS pattern explanation

**Next Steps** → [Beginner Learning Path](docs/tutorials/beginner-path.md)
- Complete 2-3 hour deep dive
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
- [Version Migration Guides](docs/migration-guides/) - Upgrade guides and migrations
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
- [Examples Directory](examples/) - 20+ working applications
- [Blog API Tutorial](docs/tutorials/blog-api.md) - Complete application
- [Production Tutorial](docs/tutorials/production-deployment.md) - End-to-end deployment

### Reference
- [CLI Reference](docs/reference/cli.md) - Command-line tools
- [Configuration](docs/reference/config.md) - FraiseQLConfig options
- [Decorators](docs/reference/decorators.md) - @type, @query, @mutation

---

## 🆘 Need Help?

**Still not sure where to start?**
1. Try the [First Hour Guide](docs/FIRST_HOUR.md) - complete progressive path
2. Try the [5-Minute Quickstart](docs/quickstart.md) - instant working API
3. Browse [Examples](examples/) for patterns similar to your use case

**Having trouble?**
- 🔧 [Troubleshooting Guide](docs/TROUBLESHOOTING.md) - Common issues and solutions
- 📋 [Quick Reference](docs/QUICK_REFERENCE.md) - Copy-paste code patterns
- 📖 [Full Documentation](docs/README.md) - Complete reference

**Have questions?**
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
