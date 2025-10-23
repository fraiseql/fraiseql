# FraiseQL Project Structure

This document explains the purpose of every directory in the FraiseQL repository to help new users understand what belongs where and what they should care about.

## Visual Project Structure

```
fraiseql/                           # Root: Unified FraiseQL Framework
├── src/                           # 📦 Main library source code
├── examples/                      # 📚 20+ working examples
├── docs/                          # 📖 Complete documentation
├── tests/                         # 🧪 Test suite
├── scripts/                       # 🔧 Development tools
├── deploy/                        # 🚀 Production deployment
├── grafana/                       # 📊 Monitoring dashboards
├── migrations/                    # 🗄️ Database setup
├── fraiseql_rs/                   # ⚡ Core Rust pipeline engine
├── archive/                       # 📁 Historical reference
├── benchmark_submission/          # 📈 Performance testing
└── templates/                     # 🏗️ Project scaffolding
```

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│               FraiseQL Unified Architecture                │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────┐    │
│  │         Framework (Python + Rust Pipeline)         │    │
│  │  ┌─────────────────────────────────────────────────┐ │    │
│  │  │  Python: src/, examples/, docs/, tests/        │ │    │
│  │  │  Rust: fraiseql_rs/ (exclusive execution)      │ │    │
│  │  │  Production: deploy/, grafana/, migrations/    │ │    │
│  │  └─────────────────────────────────────────────────┘ │    │
│  └─────────────────────────────────────────────────────┘    │
│  All queries: PostgreSQL → Rust Pipeline → HTTP Response   │
└─────────────────────────────────────────────────────────────┘
```

## Directory Overview

| Directory | Purpose | For Users? | For Contributors? |
|-----------|---------|------------|-------------------|
| `src/` | Main FraiseQL library source code | ✅ Install via pip | ✅ Core development |
| `examples/` | 20+ working examples organized by complexity | ✅ Learning & reference | ✅ Testing patterns |
| `docs/` | Comprehensive documentation and guides | ✅ Learning & reference | ✅ Documentation |
| `tests/` | Complete test suite with 100% coverage | ❌ | ✅ Quality assurance |
| `scripts/` | Development and deployment automation | ❌ | ✅ Build & deploy |
| `deploy/` | Docker, Kubernetes, and production configs | ✅ Production deployment | ✅ Infrastructure |
| `grafana/` | Pre-built dashboards for monitoring | ✅ Production monitoring | ✅ Observability |
| `migrations/` | Database schema evolution scripts | ✅ Database setup | ✅ Schema changes |
| `fraiseql_rs/` | Core Rust pipeline engine (exclusive execution) | ✅ Required performance engine | ✅ Performance optimization |
| `archive/` | Historical planning and analysis | ❌ | ❌ Legacy reference |
| `benchmark_submission/` | Performance benchmarking tools | ❌ | ✅ Performance testing |
| `templates/` | Project scaffolding templates | ✅ New projects | ✅ Tooling |

## Architecture Components

FraiseQL uses a unified architecture with exclusive Rust pipeline execution:

### **Framework Core**
- **Location**: Root level (`src/`, `examples/`, `docs/`)
- **Status**: Production stable with Rust pipeline
- **Purpose**: Complete GraphQL framework for building APIs
- **Execution**: All queries use exclusive Rust pipeline (7-10x faster)

### **Rust Pipeline Engine**
- **`fraiseql_rs/`**: Exclusive query execution engine
- **Purpose**: Core performance component for all operations
- **Architecture**: PostgreSQL → Rust Transformation → HTTP Response
- **Installation**: Automatically included with `pip install fraiseql`

### **Supporting Components**
- **Examples**: 20+ production-ready application patterns
- **Documentation**: Comprehensive guides and tutorials
- **Deployment**: Docker, Kubernetes, and monitoring configs

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

**`fraiseql_rs/`** - Core Rust pipeline engine
- Exclusive query execution engine (7-10x performance)
- Transforms PostgreSQL JSONB → HTTP responses
- Automatically included in standard installation

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
- **Performance optimization?** → `docs/performance/` + `fraiseql_rs/` (Rust pipeline)
- **Contributing code?** → `src/` + `tests/` + `scripts/`
- **Understanding architecture?** → `docs/core/fraiseql-philosophy.md`

## Questions?

If you can't find what you're looking for:
1. Check the main `README.md` for overview
2. Browse `docs/README.md` for navigation
3. Look at `examples/` for working code
4. Ask in GitHub Issues if still unclear

This structure supports multiple audiences: application developers, production engineers, and framework contributors.
