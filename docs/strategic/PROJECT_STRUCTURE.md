# FraiseQL Project Structure

This document explains the purpose of every directory in the FraiseQL repository to help new users understand what belongs where and what they should care about.

## Visual Project Structure

```
fraiseql/                           # Root: Main FraiseQL Framework (v0.11.5)
├── src/                           # 📦 Main library source code
├── examples/                      # 📚 20+ working examples
├── docs/                          # 📖 Complete documentation
├── tests/                         # 🧪 Test suite
├── scripts/                       # 🔧 Development tools
├── deploy/                        # 🚀 Production deployment
├── grafana/                       # 📊 Monitoring dashboards
├── migrations/                    # 🗄️ Database setup
├── fraiseql/                      # 🔄 v1 rebuild (experimental)
├── fraiseql_rs/                   # ⚡ Rust performance extension
├── fraiseql-v1/                   # 🎯 Portfolio showcase
├── archive/                       # 📁 Historical reference
├── benchmark_submission/          # 📈 Performance testing
└── templates/                     # 🏗️ Project scaffolding
```

## Version Relationships Map

```
┌─────────────────────────────────────────────────────────────┐
│                    FraiseQL Ecosystem                       │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────┐    │
│  │              Main Framework (v0.11.5)              │    │
│  │  ┌─────────────────────────────────────────────────┐ │    │
│  │  │  Core: src/, examples/, docs/, tests/          │ │    │
│  │  │  Rust: fraiseql_rs/ (base implementation)      │ │    │
│  │  │  Production: deploy/, grafana/, migrations/    │ │    │
│  │  └─────────────────────────────────────────────────┘ │    │
│  └─────────────────────────────────────────────────────┘    │
│         │                                                        │
│         └─ Future: fraiseql/ (clean v1 rebuild)                 │
│         └─ Portfolio: fraiseql-v1/ (interview showcase)         │
└─────────────────────────────────────────────────────────────┘
```

## Directory Overview

| Directory | Purpose | For Users? | For Contributors? |
|-----------|---------|------------|-------------------|
| `src/` | Main FraiseQL library source code (v0.11.5) | ✅ Install via pip | ✅ Core development |
| `examples/` | 20+ working examples organized by complexity | ✅ Learning & reference | ✅ Testing patterns |
| `docs/` | Comprehensive documentation and guides | ✅ Learning & reference | ✅ Documentation |
| `tests/` | Complete test suite with 100% coverage | ❌ | ✅ Quality assurance |
| `scripts/` | Development and deployment automation | ❌ | ✅ Build & deploy |
| `deploy/` | Docker, Kubernetes, and production configs | ✅ Production deployment | ✅ Infrastructure |
| `grafana/` | Pre-built dashboards for monitoring | ✅ Production monitoring | ✅ Observability |
| `migrations/` | Database schema evolution scripts | ✅ Database setup | ✅ Schema changes |
| `fraiseql/` | v1 production rebuild (15-week timeline) | ❌ Experimental | ✅ Future development |
| `fraiseql_rs/` | Core Rust implementation | ✅ Required base engine | ✅ Performance work |
| `fraiseql-v1/` | Hiring portfolio showcase (8-week timeline) | ❌ Portfolio | ✅ Interview prep |
| `archive/` | Historical planning and analysis | ❌ | ❌ Legacy reference |
| `benchmark_submission/` | Performance benchmarking tools | ❌ | ✅ Performance testing |
| `templates/` | Project scaffolding templates | ✅ New projects | ✅ Tooling |

## Version Relationships

FraiseQL has multiple implementations with different purposes:

### **Main Version (Recommended for Users)**
- **Location**: Root level (`src/`, `examples/`, `docs/`)
- **Status**: v0.11.5 - Production stable
- **Purpose**: Current production-ready framework
- **Use when**: Building applications today

### **Core Components**
- **`fraiseql_rs/`**: Core Rust implementation (base JSON transformation engine)
- **Purpose**: Required performance foundation for all FraiseQL operations
- **Use when**: Always included (automatically installed)

### **Future Versions (Not for Production)**
- **`fraiseql/`**: v1 production rebuild (Week 1/15)
- **Purpose**: Clean architecture rebuild for enterprise adoption
- **Use when**: Contributing to v1 development

- **`fraiseql-v1/`**: Hiring portfolio (8 weeks to showcase)
- **Purpose**: Interview-quality demonstration project
- **Use when**: Preparing for Staff+ engineering interviews

## Quick Start Guide

**For new users building applications:**
1. Read `README.md` for overview
2. Follow `docs/quickstart.md` for first API
3. Browse `examples/` for patterns
4. Check `docs/` for detailed guides

**For production deployment:**
1. Use `deploy/` for Docker/Kubernetes configs
2. Check `grafana/` for monitoring dashboards
3. Run `migrations/` for database setup

**For contributors:**
1. Core development happens in `src/`
2. Tests are in `tests/`
3. Build scripts in `scripts/`

## Directory Details

### User-Focused Directories

**`examples/`** - Learning by example
- 20+ complete applications from simple to enterprise
- Organized by use case (blog, ecommerce, auth, etc.)
- Each includes README with setup instructions
- Start with `examples/todo_xs/` for simplest example

**`docs/`** - Complete documentation
- Tutorials, guides, and API reference
- Performance optimization guides
- Production deployment instructions
- Architecture explanations

**`deploy/`** - Production deployment
- Docker Compose for development
- Kubernetes manifests for production
- Nginx configs for load balancing
- Security hardening scripts

**`grafana/`** - Monitoring dashboards
- Pre-built dashboards for performance metrics
- Error tracking visualizations
- Cache hit rate monitoring
- Database pool monitoring

**`migrations/`** - Database setup
- Schema creation scripts
- Data seeding for examples
- Migration patterns for production

### Developer-Focused Directories

**`src/`** - Main codebase
- FraiseQL library source code
- Type definitions, decorators, repositories
- Database integration and GraphQL schema generation

**`tests/`** - Quality assurance
- Unit tests for all components
- Integration tests for full workflows
- Performance regression tests
- Example validation tests

**`scripts/`** - Development tools
- CI/CD automation
- Code generation scripts
- Deployment helpers
- Maintenance utilities

### Specialized Directories

**`fraiseql_rs/`** - Core Rust implementation
- Base JSON transformation engine
- Required for FraiseQL's performance
- Automatically included in standard installation

**`fraiseql/`** - v1 rebuild
- Clean architecture rewrite
- Production features built-in
- 15-week development timeline

**`fraiseql-v1/`** - Portfolio project
- Interview showcase implementation
- Trinity identifiers and function-based mutations
- 8-week timeline to demo-ready

**`archive/`** - Historical reference
- Old planning documents
- Analysis from early development
- Reference for architectural decisions

**`benchmark_submission/`** - Performance testing
- Benchmarking tools and results
- Performance comparison data
- Submission artifacts for competitions

## Navigation Tips

- **Building your first API?** → `docs/quickstart.md` + `examples/todo_xs/`
- **Learning patterns?** → `examples/` directory with README index
- **Production deployment?** → `deploy/` + `docs/production/`
- **Performance optimization?** → `docs/performance/` + `fraiseql_rs/`
- **Contributing code?** → `src/` + `tests/` + `scripts/`
- **Understanding architecture?** → `docs/core/fraiseql-philosophy.md`

## Questions?

If you can't find what you're looking for:
1. Check the main `README.md` for overview
2. Browse `docs/README.md` for navigation
3. Look at `examples/` for working code
4. Ask in GitHub Issues if still unclear

This structure supports multiple audiences: application developers, production engineers, and framework contributors.
