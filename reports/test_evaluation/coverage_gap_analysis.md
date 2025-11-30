# Coverage Gap Analysis

## Summary
- Modules with <50% coverage: 45+
- Critical gaps (core modules): 8
- Non-critical gaps (optional features): 37+
- Zero coverage modules: 30+

## Critical Coverage Gaps (P0-P1)

| Module | Coverage | Risk | Recommendation |
|--------|----------|------|----------------|
| `core/nested_field_resolver.py` | 11% | **High** | Core resolver logic needs tests |
| `cqrs/pagination.py` | 13% | **High** | Pagination affects all list queries |
| `cqrs/repository.py` | 15% | **High** | Repository pattern is foundational |
| `auth/auth0.py` | 20% | **High** | Auth flows are security-critical |
| `auth/token_revocation.py` | 28% | **High** | Token security is critical |
| `analysis/query_analyzer.py` | 32% | **Medium** | Query analysis affects performance |
| `cache/view_metadata.py` | 33% | **Medium** | Caching bugs cause stale data |
| `execution/mode_selector.py` | 39% | **Medium** | Mode selection affects query execution |

## Non-Critical Gaps (P2-P3)

### P2 - Medium Priority (Optional Features)
| Module | Coverage | Notes |
|--------|----------|-------|
| `decorators.py` | 42% | Many decorator variants untested |
| `errors/user_friendly.py` | 30% | Error formatting for users |
| `fastapi/routers.py` | 43% | Router edge cases |
| `db.py` | 48% | Large module, partial coverage |

### P3 - Low Priority (Enterprise/Optional)
- `enterprise/rbac/*` - All 0% (enterprise feature)
- `enterprise/audit/*` - All 0% (enterprise feature)
- `enterprise/crypto/*` - All 0% (enterprise feature)

## Zero Coverage Modules

### Intentionally Untested (CLI/DevTools)
| Module | Reason |
|--------|--------|
| `cli/main.py` | CLI entry point |
| `cli/commands/*` | All CLI commands (check, dev, doctor, generate, init, migrate, sbom, sql, turbo) |
| `cli/sql_helper.py` | SQL CLI helper |
| `debug/debug.py` | Debug utilities |

### Enterprise Features (Untested - 0%)
| Module | Reason |
|--------|--------|
| `enterprise/rbac/mutations.py` | RBAC mutations |
| `enterprise/rbac/directives.py` | GraphQL directives |
| `enterprise/rbac/middleware.py` | RBAC middleware |
| `enterprise/audit/event_logger.py` | Audit logging |

### Native Auth (Untested - 0%)
| Module | Reason |
|--------|--------|
| `auth/native/factory.py` | Native auth factory |
| `auth/native/middleware.py` | Auth middleware |
| `auth/native/provider.py` | Auth provider |
| `auth/native/router.py` | Auth router |
| `auth/native/tokens.py` | Token management |

### Caching Module (Untested - 0%)
| Module | Reason |
|--------|--------|
| `caching/cache_key.py` | Cache key generation |
| `caching/postgres_cache.py` | PostgreSQL cache |
| `caching/result_cache.py` | Result caching |
| `caching/schema_analyzer.py` | Schema analysis for caching |

## Action Items

### P0 - Critical (Immediate)
1. **Add tests for `core/nested_field_resolver.py`** - 11% coverage on core functionality
   - Test nested object resolution
   - Test array field handling
   - Test error cases

2. **Add tests for `cqrs/repository.py`** - 15% coverage on data access layer
   - Test CRUD operations
   - Test query building
   - Test transaction handling

### P1 - High (This Sprint)
3. **Add tests for `cqrs/pagination.py`** - 13% coverage
   - Test cursor-based pagination
   - Test offset pagination
   - Test edge cases (empty results, single page)

4. **Add auth tests** - 20-28% coverage
   - `auth/auth0.py` - Test token validation
   - `auth/token_revocation.py` - Test revocation flows

### P2 - Medium (Next Sprint)
5. **Add caching tests** - 0% coverage for entire module
   - Start with `caching/result_cache.py`
   - Add `caching/cache_key.py` tests

6. **Add FastAPI router tests** - 43% coverage
   - Test route handlers
   - Test error responses

### P3 - Low (Backlog)
7. **CLI tests** - Consider e2e tests for critical CLI commands
   - `init` command
   - `migrate` command

8. **Enterprise features** - Add tests when features are actively used
   - RBAC tests
   - Audit logging tests

## Coverage Improvement Targets

| Timeframe | Target | Focus Areas |
|-----------|--------|-------------|
| Week 1 | 55% overall | Core resolvers, CQRS |
| Week 2 | 60% overall | Auth, Caching |
| Month 1 | 70% overall | FastAPI, Decorators |
| Month 2 | 80% overall | Enterprise, CLI |
