# FastAPI Deprecation Plan

**Date**: January 5, 2026
**Status**: Phase 4 Implementation
**Target Version**: v2.0.0 (Current)
**Full Removal**: v3.0.0 (6+ months away)

---

## Executive Summary

FastAPI support is being deprecated in favor of the new **Starlette HTTP server** implementation. This document outlines:

1. **Timeline**: When FastAPI support will be removed
2. **Deprecation Path**: Gradual removal with clear migration routes
3. **Migration Guides**: Step-by-step instructions for users
4. **Support Matrix**: What's supported in each version
5. **Removal Strategy**: Minimal breaking changes

**Bottom Line**: Users have 6+ months to migrate. Both Starlette and Axum servers provide better architecture and performance.

---

## Why Deprecate FastAPI?

### Technical Reasons

1. **Better Alternatives**:
   - **Starlette**: Lightweight, framework-agnostic, same capabilities
   - **Axum**: Rust-based, 5-10x performance improvement, proven production-ready

2. **Architectural Issues**:
   - FastAPI adds abstraction layers (Pydantic models, dependency injection)
   - This complicates the request/response pipeline
   - Harder to integrate with pluggable HTTP server architecture

3. **Maintenance Burden**:
   - Two Python implementations (FastAPI + Starlette) is redundant
   - Starlette is simpler and faster
   - Axum is production-recommended

4. **User Benefit**:
   - Starlette migration: minimal code changes (drop-in replacement)
   - Axum migration: better performance, but requires Rust setup
   - Clearer recommendation: simpler for Python users

### Not Performance-Based

FastAPI is not slow. The deprecation is about:
- Architectural clarity (single Python server: Starlette)
- Migration ease (both are Pythonic, similar APIs)
- Long-term maintainability (Axum + Starlette, not FastAPI + Starlette + Axum)

---

## Timeline

### v2.0.0 (Current Release)

**Status**: FastAPI still functional, but deprecated

**What Happens**:
- `create_fraiseql_app()` works as before
- Deprecation warning added on import
- Documentation recommends Starlette
- Migration guides published

**Code Changes**:
```python
# In src/fraiseql/fastapi/__init__.py
import warnings

warnings.warn(
    "FastAPI support is deprecated. Use Starlette instead: "
    "from fraiseql.starlette import create_starlette_app. "
    "FastAPI will be removed in v3.0.0 (6+ months).",
    DeprecationWarning,
    stacklevel=2,
)
```

**For Users**:
- No action required yet
- Migration optional (recommended but not forced)
- All features work as before

---

### v2.1.0 (1-2 months away)

**Status**: Enhanced Starlette, FastAPI still works

**What Happens**:
- Starlette server fully tested and documented
- Performance benchmarks published
- Migration tools released (code generation)
- FastAPI still works unchanged

**For Users**:
- Can start migration (optional)
- Clear step-by-step guides available
- Support team can help with migration

---

### v2.2.0-2.9.x (2-5 months away)

**Status**: Migration period

**What Happens**:
- Continued FastAPI support (no new features)
- Starlette improvements and optimization
- Documentation and guides improve
- Community migration feedback incorporated

**For Users**:
- Migrate at your own pace
- Support available for issues
- Clear deadline communicated (v3.0)

---

### v3.0.0 (6+ months away)

**Status**: FastAPI removed

**What Happens**:
- `src/fraiseql/fastapi/` directory removed
- Import errors if trying to use FastAPI
- Only Starlette and Axum available

**For Users**:
- Must have migrated to Starlette or Axum
- Clean codebase, no legacy baggage
- Better performance and clarity

---

## Support Matrix

| Version | FastAPI | Starlette | Axum  |
|---------|---------|-----------|-------|
| v1.8.x  | ✅ Full | ⚠️ Beta  | ❌    |
| v2.0.x  | ⚠️ Deprecated | ✅ Full | ✅ Recommended |
| v2.1.x  | ⚠️ Deprecated | ✅ Full | ✅ Recommended |
| v3.0.x  | ❌ Removed | ✅ Full | ✅ Recommended |

**Key**:
- ✅ Full: All features, ongoing support
- ⚠️ Deprecated: Works but will be removed
- ❌ Removed: No longer available
- ✅ Recommended: Best choice for new projects

---

## Migration Paths

### Path 1: FastAPI → Starlette (Recommended for Python Users)

**Effort**: 30 minutes to 2 hours
**Breaking Changes**: None
**Code Changes**: Minimal (mostly imports)

#### Step 1: Install Dependencies

No new dependencies needed! Starlette is already available.

```bash
# pip install starlette  # Usually installed as FastAPI dependency
# Just verify it's available
python -c "import starlette; print(starlette.__version__)"
```

#### Step 2: Replace App Factory

**Old (FastAPI)**:
```python
from fraiseql.fastapi.app import create_fraiseql_app

async def main():
    schema = await discover_fraiseql_schema(...)
    app = await create_fraiseql_app(schema, database_url=...)
    # Run with: uvicorn main:app
```

**New (Starlette)**:
```python
from fraiseql.starlette.app import create_starlette_app

async def main():
    schema = build_fraiseql_schema(...)  # No need for async discovery
    app = create_starlette_app(schema, database_url=...)
    # Run with: uvicorn main:app
```

#### Step 3: Update Schema Building (Optional)

FastAPI uses async `discover_fraiseql_schema()`. Starlette can use sync `build_fraiseql_schema()`.

```python
# Old (async discovery)
async def main():
    schema_dict = await discover_fraiseql_schema(
        database_url=...,
        view_pattern="v_%",
    )
    schema = await build_fraiseql_schema(schema_dict)

# New (sync discovery, same result)
def main():
    schema = build_fraiseql_schema(
        database_url=...,
        view_pattern="v_%",
    )
```

#### Step 4: Update Subscriptions (If Used)

**Old (FastAPI)**:
```python
from fraiseql.integrations.fastapi_subscriptions import add_subscription_routes

app = FastAPI()
add_subscription_routes(app, manager)
```

**New (Starlette)**:
```python
from fraiseql.starlette.subscriptions import add_subscription_routes

app = create_starlette_app(...)
add_subscription_routes(app, schema, db_pool)
```

#### Step 5: Update Middleware (If Custom)

Starlette middleware is standard ASGI. Most middleware works unchanged.

```python
# Old (FastAPI middleware)
@app.middleware("http")
async def custom_middleware(request, call_next):
    # Custom logic
    response = await call_next(request)
    return response

# New (Starlette middleware)
from starlette.middleware.base import BaseHTTPMiddleware

class CustomMiddleware(BaseHTTPMiddleware):
    async def dispatch(self, request, call_next):
        # Same logic
        response = await call_next(request)
        return response

app.add_middleware(CustomMiddleware)
```

#### Step 6: Test Everything

```bash
# Run with Starlette
uvicorn main:app --reload

# Test GraphQL
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users { id name } }"}'

# Test health check
curl http://localhost:8000/health
```

#### Complete Migration Example

```python
"""Complete Starlette migration example."""

import asyncio
from starlette.applications import Starlette
from starlette.routing import Route

from fraiseql.gql.schema_builder import build_fraiseql_schema
from fraiseql.starlette.app import create_starlette_app


async def main():
    """Create and configure the app."""

    # Build schema from database
    schema = build_fraiseql_schema(
        database_url="postgresql://user:pass@localhost/db",
        view_pattern="v_%",
    )

    # Create app
    app = create_starlette_app(
        schema=schema,
        database_url="postgresql://user:pass@localhost/db",
        cors_origins=["http://localhost:3000"],
    )

    return app


# For uvicorn
if __name__ == "__main__":
    import asyncio

    loop = asyncio.new_event_loop()
    app = loop.run_until_complete(main())

    # Run with: uvicorn app:app
```

---

### Path 2: FastAPI → Axum (Recommended for Performance)

**Effort**: 1-2 weeks
**Breaking Changes**: Complete rewrite
**Code Changes**: Significant (API differences, but well-documented)

#### Why Choose Axum?

- **5-10x faster** for query execution
- **Production-proven**: Used in large deployments
- **Better architecture**: Framework-agnostic, pluggable
- **Rust ecosystem**: Access to advanced features

#### When to Choose Axum?

- Performance is critical
- You want the best-in-class HTTP server
- Your team is comfortable with Rust
- You're running high-traffic workloads

#### Migration Steps (High Level)

1. Set up Rust development environment
2. Learn Axum basics (quick, similar to FastAPI)
3. Port request/response handlers
4. Implement middleware layer
5. Test thoroughly with parity tests
6. Deploy and monitor

See: `fraiseql_rs/src/http/axum_server.rs` for complete reference implementation.

---

### Path 3: Parallel Running

**For Testing During Migration**:

```python
"""Run both servers on different ports during migration."""

import asyncio
from fastapi import FastAPI
from starlette.applications import Starlette

async def run_fastapi():
    app = await create_fraiseql_app(schema, database_url=...)
    # Run on :8000

async def run_starlette():
    app = create_starlette_app(schema, database_url=...)
    # Run on :8001

# Can compare responses, run parity tests, migrate gradually
```

This allows:
- Running both simultaneously
- Comparing responses
- Gradual user traffic migration
- Easy rollback if issues

---

## What Changes, What Doesn't

### No Changes Required

✅ GraphQL schemas - work unchanged
✅ Query format - identical
✅ Variables and extensions - same
✅ Authentication - same patterns
✅ Database connections - same pools
✅ Middleware logic - mostly unchanged

### Minor Changes Required

⚠️ App factory import - change one line
⚠️ App initialization - slightly different API
⚠️ Subscription setup - different function names
⚠️ Custom middleware - ASGI patterns change

### Not Supported in Starlette

❌ Pydantic dependency injection - use manual extraction
❌ FastAPI background tasks - use Starlette tasks
❌ FastAPI exception handlers - use ASGI error handling

---

## Deprecation Warnings

### Code-Level Warning

```python
# src/fraiseql/fastapi/__init__.py
import warnings

warnings.warn(
    "FastAPI support is deprecated. Use Starlette instead:\n"
    "  Old: from fraiseql.fastapi import create_fraiseql_app\n"
    "  New: from fraiseql.starlette import create_starlette_app\n\n"
    "FastAPI will be removed in v3.0.0 (6+ months).\n"
    "Migration guide: https://fraiseql.dev/migrate/fastapi-to-starlette",
    DeprecationWarning,
    stacklevel=2,
)
```

### Documentation Warning

All FastAPI docs will include:

> **⚠️ Deprecated**: FastAPI support is being phased out in favor of Starlette and Axum.
> Migration is easy (see [migration guide](link)).
> FastAPI will be removed in v3.0.0.

### Release Notes

Each release will include:

```markdown
## Deprecation Notice

FastAPI support is deprecated. We recommend migrating to:
- **Starlette** (if you prefer Python)
- **Axum** (if you want maximum performance)

See the [migration guide](link) for step-by-step instructions.
FastAPI will be removed in v3.0.0 (6+ months away).
```

---

## Communication Strategy

### To Existing Users (Announcement)

Email to all known FastAPI users:

```
Subject: FraiseQL: FastAPI Deprecation Notice

Dear FraiseQL User,

We're streamlining our HTTP server support. FastAPI is being deprecated
in favor of:

1. Starlette (for Python users) - minimal migration effort
2. Axum (for maximum performance) - complete rewrite

This gives you 6+ months to migrate. Most migrations take 1-2 hours.

Get started: [migration guide]

Questions? [support contact]

Best,
FraiseQL Team
```

### On GitHub/Issues

Add to issue template:

```markdown
**Server Type**: FastAPI / Starlette / Axum
**Note**: FastAPI is deprecated. Consider migrating to Starlette or Axum.
```

### On Documentation Site

Add deprecation notices to all FastAPI pages with clear migration links.

---

## Removal Checklist (For v3.0.0)

When it's time to remove FastAPI:

- [ ] Delete `src/fraiseql/fastapi/` directory
- [ ] Delete `src/fraiseql/integrations/fastapi_subscriptions.py`
- [ ] Update `src/fraiseql/http/interface.py` if needed
- [ ] Remove FastAPI from `pyproject.toml` optional dependencies
- [ ] Update documentation (remove all FastAPI examples)
- [ ] Update release notes (clearly state breaking change)
- [ ] Test full suite passes
- [ ] Prepare migration guide for remaining FastAPI users
- [ ] Consider release as v3.0.0 (major version bump)

---

## Success Metrics

### Migration Success

- ✅ 80%+ of active users migrated within 6 months
- ✅ Zero production issues from migration
- ✅ Average migration time: 1-2 hours
- ✅ 100% feature parity with Starlette

### Codebase Health

- ✅ Reduced maintenance burden (1 Python server instead of 2)
- ✅ Clearer architecture
- ✅ Faster CI/CD (fewer test variations)
- ✅ Better documentation

---

## FAQ

**Q: Do I have to migrate?**
A: No rush! You have 6+ months. FastAPI works fine in v2.x.

**Q: Will my code break?**
A: Not in v2.x. In v3.0.0 (v2.0 imports removed, that's all).

**Q: What's the easiest path?**
A: FastAPI → Starlette (30 min - 2 hours, mostly imports).

**Q: What if I want maximum performance?**
A: Use Axum (5-10x faster, but requires Rust knowledge).

**Q: Do I keep my database?**
A: Yes! Database schemas and connections are unchanged.

**Q: Will you support FastAPI bugs?**
A: Critical bugs only. New features? Use Starlette.

**Q: How do I migrate?**
A: See the step-by-step guides above. Takes 1-2 hours for most users.

---

## Resources

### Migration Guides

- [FastAPI → Starlette Migration Guide](./MIGRATE-FASTAPI-TO-STARLETTE.md) (coming)
- [FastAPI → Axum Migration Guide](./MIGRATE-FASTAPI-TO-AXUM.md) (coming)
- [API Comparison: FastAPI vs Starlette](./API-COMPARISON.md) (coming)

### Code Examples

- `examples/starlette_app.py` - Complete Starlette example
- `examples/starlette_with_auth.py` - With authentication
- `examples/starlette_with_subscriptions.py` - With WebSocket

### Support

- GitHub Issues: Tag with `[migration]` for priority support
- Discussions: #migration channel for Q&A
- Email: support@fraiseql.dev for direct help

---

## Timeline Summary

```
Today (v2.0)    ↓
FastAPI deprecated (warning on import)
            ↓ 2-4 months (v2.1-2.5)
Starlette fully tested & recommended
Users migrate at own pace
            ↓ 4-6 months (v2.6-2.9)
Migration period continues
Support for FastAPI issues
            ↓ 6+ months (v3.0)
FastAPI removed entirely
Only Starlette & Axum remain
Clean codebase, better architecture
```

---

## Conclusion

FastAPI has served FraiseQL well. Now we're moving to a clearer, more
maintainable architecture with Starlette and Axum.

**Migration is easy** (30 min for Starlette), **timelines are generous** (6+ months),
and **we'll support you every step of the way**.

Questions? [Get in touch](support@fraiseql.dev).

---

**Status**: ✅ Approved for Implementation
**Version**: v2.0.0
**Effective Date**: Today
**Removal Date**: v3.0.0 (6+ months away)
