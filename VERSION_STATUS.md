# FraiseQL Version Status

**Last Updated**: 2025-10-25

## Current Production Version: v1.0.2

FraiseQL v1.0.2 is the stable, production-ready release suitable for all users. This patch release fixes README rendering and documentation links on PyPI.

## Version Overview

| Version | Status | Recommended For | Stability |
|---------|--------|----------------|-----------|
| **v1.0.2** | Production Stable | All users | ✅ Stable |
| v1.0.1 | Stable | All users | ✅ Stable |
| v1.0.0 | Stable | All users | ✅ Stable |
| v0.11.5 | Superseded | Legacy projects | ⚠️ Use v1.0.2 |
| Rust Pipeline | Integrated | Included in v1.0+ | ✅ Stable |

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
pip install fraiseql>=1.0.1
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
curl -O https://raw.githubusercontent.com/fraiseql/fraiseql/v1.0.1/deployment/docker-compose.prod.yml
curl -O https://raw.githubusercontent.com/fraiseql/fraiseql/v1.0.1/deployment/.env.example
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

See [docs/ROADMAP.md](docs/ROADMAP.md) for planned features in v1.1+.

### Planned for v1.1
- CLI code generation from database schema
- Enhanced multi-tenancy patterns
- Performance monitoring dashboard

### Planned for v1.2
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
