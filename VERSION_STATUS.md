# FraiseQL Version Status

**Last Updated**: 2025-10-29

## Current Production Version: v1.1.0

FraiseQL v1.1.0 is the stable, production-ready release suitable for all users.

## Version Overview

| Version | Status | Recommended For | Stability |
|---------|--------|----------------|-----------|
| **v1.1.0** | Production Stable | All users | ✅ Stable |
| v1.0.3 | Stable | All users | ✅ Stable |
| v1.0.2 | Stable | All users | ✅ Stable |
| v1.0.1 | Stable | All users | ✅ Stable |
| v1.0.0 | Stable | All users | ✅ Stable |
| v0.11.5 | Superseded | Legacy projects | ⚠️ Use v1.1.0 |
| Rust Pipeline | Integrated | Included in v1.0+ | ✅ Stable |

## What's New in v1.1.0

### 🎯 Enhanced PostgreSQL Filtering
- ✅ **38+ PostgreSQL operators** fully supported and documented
- ✅ **Dual-path intelligence** for native arrays vs JSONB optimization
- ✅ **Full-text search** with 12 operators including ranking and relevance
- ✅ **JSONB operators** for advanced JSON querying (10 operators)
- ✅ **Regex text matching** with POSIX regex support
- ✅ **Array operators** with length checking and element testing

### 🐛 Bug Fixes
- ✅ Fixed nested array filter registry not being wired to schema builder (#97, #100)
- ✅ Decorator-based API (`@register_nested_array_filter`) now fully functional
- ✅ Priority system: field attributes → nested_where_type → registry lookup

### 📚 Documentation
- ✅ **2,091 lines** of comprehensive filter operator documentation
- ✅ Complete filter operators reference with SQL examples and performance tips
- ✅ Real-world examples: E-commerce, CMS, user management, log analysis, SaaS
- ✅ GIN index recommendations and troubleshooting guides

### 🔒 Security
- ✅ Fixed PyO3 buffer overflow vulnerability (GHSA-pph8-gcv7-4qj5)

### ✅ Testing
- ✅ **3,650 tests passing** (100% pass rate)
- ✅ +34 new tests added
- ✅ All operators validated with comprehensive test coverage

**See [CHANGELOG.md](CHANGELOG.md#110---2025-10-29) for complete details.**

## What's New in v1.0.3

### Fixed
- ✅ Critical RustResponseBytes handling in GraphQL execution
- ✅ Direct HTTP response path now working as designed
- ✅ WHERE clause generation for JSONB tables enhanced

**See [CHANGELOG.md](CHANGELOG.md#103---2025-10-27) for complete details.**

## What's New in v1.0.2

### PyPI README Improvements
- ✅ Fixed Markdown rendering issues (proper spacing after headers)
- ✅ All documentation links now work on PyPI (20+ links converted to absolute URLs)
- ✅ Code examples show correct Rust pipeline usage (no manual Python instantiation)
- ✅ Modernized type hints (Python 3.10+ syntax: `T | None`, `UUID`)
- ✅ Added Coordinate geospatial type to specialized types list

**See [CHANGELOG.md](CHANGELOG.md#102---2025-10-25) for complete details.**

## What's New in v1.0.1

### Production Deployment Templates
- ✅ Complete Docker Compose production setup (app, PostgreSQL, PgBouncer, Grafana, Nginx)
- ✅ Kubernetes manifests with auto-scaling (HPA 3-10 replicas)
- ✅ PostgreSQL StatefulSet with persistent storage
- ✅ Production checklist (security, performance, infrastructure)
- ✅ Environment configuration templates

### Documentation Enhancements
- ✅ Feature discovery index (40+ capabilities cataloged)
- ✅ Troubleshooting decision tree (6 diagnostic categories)
- ✅ Reproducible benchmark methodology
- ✅ 47% cleaner documentation structure (15 → 8 root files)
- ✅ Archive and internal docs properly organized

### Professional Polish
- ✅ Cross-referenced troubleshooting guides
- ✅ Improved navigation and discoverability
- ✅ Repository cleanup (18 backup files removed)

**See [CHANGELOG.md](CHANGELOG.md#101---2025-10-24) for complete details.**

## What's in v1.0.0

### Core Features
- ✅ CQRS architecture with PostgreSQL
- ✅ Rust-accelerated JSON transformation (7-10x faster)
- ✅ Hybrid table support (regular + JSONB columns)
- ✅ Advanced type system (UUID, DateTime, IP, CIDR, LTree, MAC, etc.)
- ✅ Nested object filtering
- ✅ Trinity identifier pattern support
- ✅ Comprehensive GraphQL introspection

### Performance
- Sub-millisecond query latency (0.5-5ms typical)
- Rust pipeline: 7-10x faster than pure Python
- APQ (Automatic Persisted Queries) support
- PostgreSQL-native caching

### Test Coverage
- 3,556 tests passing (100% pass rate)
- 0 skipped tests
- 0 failing tests
- Comprehensive integration and regression testing

## Installation

### For New Projects (Recommended)
```bash
pip install fraiseql>=1.1.0
```

### For Existing Projects
```bash
pip install --upgrade fraiseql
```

### Get Deployment Templates
```bash
# Clone repository for deployment templates
git clone https://github.com/fraiseql/fraiseql
cd fraiseql

# Or download specific templates
curl -O https://raw.githubusercontent.com/fraiseql/fraiseql/v1.1.0/deployment/docker-compose.prod.yml
curl -O https://raw.githubusercontent.com/fraiseql/fraiseql/v1.1.0/deployment/.env.example
```

See [MIGRATION_GUIDE.md](docs/migration/v0-to-v1.md) for upgrade instructions.

## Version Support Policy

| Version | Status | Security Fixes | Bug Fixes | New Features |
|---------|--------|----------------|-----------|--------------|
| v1.0.x | Supported | ✅ Yes | ✅ Yes | ✅ Yes |
| v0.11.x | Limited | ✅ Critical only | ❌ No | ❌ No |
| < v0.11 | Unsupported | ❌ No | ❌ No | ❌ No |

## Experimental Features

None currently. All features in v1.0.0 are production-stable.

## Future Roadmap

### Planned for v1.2
- CLI code generation from database schema
- Enhanced multi-tenancy patterns
- Performance monitoring dashboard

### Planned for v1.3
- GraphQL federation support
- Real-time subscriptions
- Advanced caching strategies

## Getting Help

- **Documentation**: https://fraiseql.readthedocs.io
- **Issues**: https://github.com/fraiseql/fraiseql/issues
- **Discussions**: https://github.com/fraiseql/fraiseql/discussions
- **Email**: lionel.hamayon@evolution-digitale.fr

## Reporting Issues

If you encounter issues with v1.0.0, please:
1. Check [CHANGELOG.md](CHANGELOG.md) for known issues
2. Search existing [GitHub issues](https://github.com/fraiseql/fraiseql/issues)
3. Create a new issue with:
   - FraiseQL version (`pip show fraiseql`)
   - Python version
   - PostgreSQL version
   - Minimal reproduction example

---

**Note**: This project follows [Semantic Versioning](https://semver.org/).
