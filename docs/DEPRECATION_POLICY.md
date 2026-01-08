# FraiseQL Deprecation Policy

## Overview

This document defines the deprecation process for features, APIs, and implementations in FraiseQL. Clear deprecation guidance helps users plan migrations while giving the project maintainers a roadmap for feature evolution.

## Deprecation Lifecycle

All deprecations follow a 3-phase lifecycle:

### Phase 1: Announcement (Current + 1 Minor)
- Feature marked as deprecated in code
- Documentation updated with deprecation notice
- Migration guide provided
- Deprecation warning logged when used
- Tests remain active (no breaking changes)

**Duration**: Minimum 1 minor version cycle (e.g., 1.8.x → 1.9.0)

### Phase 2: Maintenance Mode (Current + 2-3 Minors)
- Feature fully functional but not enhanced
- Documentation clearly marks as "end-of-life"
- Support limited to critical bugs
- New features not added

**Duration**: 2-3 minor version cycles (e.g., 1.9.0 → 2.0.0)

### Phase 3: Removal (Major Version +1)
- Feature removed from codebase
- Code moved to `.archive/deprecated/`
- CHANGELOG documents removal
- Final version with feature noted for reference

**Duration**: Next major version release

## Current Deprecations

### HTTP Server Architecture (v2.0: Multi-Framework Support)

v2.0 introduces a **flexible, modular HTTP architecture** supporting both native Rust servers and traditional Python servers.

#### 🟢 HTTP Server Options - **ALL SUPPORTED (v2.0+)**

v2.0 supports multiple HTTP server implementations through a unified, modular approach:

**Architecture**:
```
HTTP Request
    ↓
Framework Choice
├─ Rust Native (High Performance)
│  ├─ Axum (modern, recommended Rust default)
│  ├─ Actix-web (proven, battle-tested)
│  └─ Hyper (low-level HTTP control)
│
└─ Python Traditional (Compatibility)
   ├─ FastAPI (Python, v1.8.x pattern)
   ├─ Starlette (lightweight Python ASGI)
   └─ Custom (user-implemented)
    ↓
Modular GraphQL Core (Rust)
  ├─ Parser, validator, executor
  ├─ Type system, field resolution
  └─ Cache, auth, enterprise features
    ↓
HTTP Response
```

**Design Philosophy**:
- ✅ Framework-agnostic core (not tied to any implementation)
- ✅ Multiple language options (Rust for performance, Python for compatibility)
- ✅ Composable middleware (auth, caching, rate limiting)
- ✅ Easy to add new frameworks
- ✅ Users choose based on their needs

---

#### **Rust Servers (Native Performance)**

**Status**: Primary focus for new v2.0 applications

**Support Level**: Full support, all features, production-ready

##### Axum (Recommended Rust Server)

- **Performance**: 7-10x faster than Python servers
- **Best for**: New v2.0 applications, performance-critical deployments
- **Ecosystem**: Modern async Rust, growing community
- **Recommendation**: **Start here** for new projects

##### Actix-web (Proven Rust Server)

- **Performance**: Excellent, mature performance
- **Best for**: Migrating from v1.8.x FastAPI, proven track record
- **Ecosystem**: Mature, excellent integrations
- **Recommendation**: Good for teams familiar with Actix

##### Hyper (Low-Level Rust Server)

- **Performance**: Excellent, maximum control
- **Best for**: Custom protocols, embedded use cases, fine-grained HTTP
- **Ecosystem**: Minimal, low-level control
- **Recommendation**: Advanced use cases, custom requirements

---

#### **Python Servers (Compatibility)**

**Status**: Supported for compatibility, gradually phase out

**Support Level**: Full support in v2.0, maintenance mode v2.1+

##### FastAPI (Python Traditional Server)

- **Status**: Fully supported in v2.0, same as v1.8.x
- **Performance**: 100 req/sec per core (lower than Rust)
- **Best for**: Existing Python applications, team with Python expertise
- **Migration Path**: Can upgrade to Rust servers later
- **Recommendation**: Use if team requires Python, plan Rust migration

##### Starlette (Lightweight Python Server)

- **Status**: Fully supported in v2.0 (restored support)
- **Performance**: Similar to FastAPI, lightweight
- **Best for**: Minimal Python applications, custom ASGI needs
- **Migration Path**: Can upgrade to Rust servers later
- **Recommendation**: Use for lightweight Python deployments

---

### HTTP Middleware (Framework-Agnostic, Shared)

Both Rust and Python servers share the same modular middleware system:

```
Modular Middleware (Rust-backed, framework-agnostic)
├─ Authentication (Auth0, JWT, custom)
├─ Authorization (RBAC, field-level)
├─ Caching (result caching, APQ)
├─ Rate limiting
├─ CORS & CSRF
├─ Request logging
├─ Error handling
├─ Tracing & metrics
└─ Custom (user-defined)
```

**Key Point**: Middleware is the same regardless of framework choice.

---

### Migration and Compatibility

**For v1.8.x FastAPI users**:
1. **v1.8.x** → **v2.0 with FastAPI** (zero-change upgrade, same performance)
2. **v2.0 with FastAPI** → **v2.0 with Axum** (when ready for performance boost, step-by-step)
3. Alternative: **v2.0 with Axum** directly (for maximum performance immediately)

**For v1.8.x Starlette users**:
1. **v1.8.x** → **v2.0 with Starlette** (compatible upgrade)
2. **v2.0 with Starlette** → **v2.0 with Axum** (when ready for performance)

**For new v2.0 applications**:
- **Recommended**: Start with Axum (best performance, modern)
- **Alternative**: Actix-web (proven, good for migrations)
- **Python needed**: FastAPI or Starlette (compatible)

**Timeline**:
- **v2.0.0**: All servers (Axum, Actix, Hyper, FastAPI, Starlette) fully supported
- **v2.1.0**: All servers maintained, emphasis on Rust servers
- **v3.0.0+**: Python servers may be deprecated (depends on adoption)

---

## API Deprecations

### Pattern: Feature Deprecation

When deprecating an API or feature:

1. **Mark in code**:
   ```python
   import warnings

   @deprecated(version="1.8.0", removal_version="2.0.0")
   def old_function():
       """This function is deprecated.

       .. deprecated:: 1.8.0
           Use :func:`new_function` instead.
       """
       warnings.warn(
           "old_function is deprecated, use new_function instead",
           DeprecationWarning,
           stacklevel=2
       )
       return new_function()
   ```

2. **Document in code**:
   - Add to docstring with `.. deprecated::` directive
   - Link to migration guide
   - Show alternative

3. **Update documentation**:
   - Mark feature as deprecated in docs
   - Add migration guide
   - Update examples to use alternatives

4. **Log warnings**:
   - Runtime warnings when used
   - Include version information
   - Point to migration guide

## Removal Checklist

Before removing a deprecated feature:

- [ ] Feature has been deprecated for 2+ minor versions
- [ ] Migration guide exists and is linked from docs
- [ ] All internal code updated to use replacement
- [ ] Tests for deprecated feature removed or archived
- [ ] CHANGELOG entry created
- [ ] Release notes include removal notice
- [ ] Code moved to `.archive/deprecated/` with original commit reference

## Version Numbers

FraiseQL uses semantic versioning: `MAJOR.MINOR.PATCH`

- **Major**: Breaking changes (deprecations finalized)
- **Minor**: New features, deprecation announcements
- **Patch**: Bug fixes, no API changes

## FAQ

### Q: Will my code break when I upgrade?
**A**: No. Features are deprecated for at least 2 minor versions before removal. You'll receive warnings but code will continue to work.

### Q: How long do I have to migrate?
**A**: Minimum 6+ months. Major version releases (which include removals) happen roughly annually.

### Q: Where can I find deprecated features?
**A**: Check `.archive/deprecated/` for removed code and implementation examples.

### Q: What if I really need a deprecated feature?
**A**: Consider:
1. Using an older version of FraiseQL
2. Contributing a patch to keep the feature maintained
3. Implementing the feature in your application

---

## Communication

Deprecations are communicated through:

1. **Code warnings** - Runtime warnings when deprecated features are used
2. **Documentation** - Deprecation notices in docs
3. **CHANGELOG** - Documented in release notes
4. **Migration guides** - Step-by-step guides in `/docs/migration/`
5. **GitHub issues** - Announced in issues/discussions

---

**Last Updated**: January 8, 2026
**Policy Version**: 2.0
**Next Review**: v2.1.0 release
