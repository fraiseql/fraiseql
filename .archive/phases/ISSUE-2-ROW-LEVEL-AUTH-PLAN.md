# Row-Level Authorization Filtering Middleware - Implementation Plan

**Status**: READY FOR IMPLEMENTATION
**Effort**: 6-8 hours
**Phases**: 5 (Core → Integration → Testing)
**Priority**: HIGH (Security-critical)

---

## EXECUTIVE SUMMARY

Implement automatic row-level authorization filtering middleware to prevent data exposure and improve security posture. Current system requires manual WHERE clause injection by developers, creating:
- Data exposure risk (unauthorized data fetched before filtering)
- Performance penalty (extra database load)
- Maintainability issues (developers must remember to add WHERE clauses)

The new middleware will automatically inject WHERE clauses based on user RBAC context, operating transparently to existing code.

---

## ARCHITECTURE

```
HTTP Request (FastAPI)
    ↓
[1] RbacContextMiddleware (Extract user/tenant/roles)
    ↓
[2] RowLevelAuthMiddleware (Resolve row filters from RBAC)
    ↓
GraphQL Parser
    ↓
[3] WHERE Clause Injection (Merge auth filters with explicit WHERE)
    ↓
Rust Pipeline (execute_via_rust_pipeline)
    ↓
PostgreSQL (Filtered query execution)
```

**Key Design Principles:**
- **Non-invasive**: No changes to existing WHERE clause API
- **Composable**: Stacks with explicit WHERE clauses without conflicts
- **Performant**: Filter resolution cached at request level (<1ms overhead cached)
- **Auditable**: All injected WHERE clauses logged for compliance
- **Testable**: Isolated middleware components with clear dependencies

---

## PHASE 1: Core Middleware & Filter Resolution (2-3 hours)

### Files to Create
1. `/home/lionel/code/fraiseql/src/fraiseql/enterprise/rbac/row_level_middleware.py` (400 LOC)
   - Main middleware orchestrating user context extraction, filter resolution, and WHERE injection
   - Methods: `resolve()`, `_get_row_filters()`, `_merge_where_clauses()`, `_build_ownership_filter()`, `_build_tenant_filter()`
   - Caches resolved filters per user+table combination

2. `/home/lionel/code/fraiseql/src/fraiseql/enterprise/rbac/row_filter_resolver.py` (350 LOC)
   - Database query resolver for RBAC constraints
   - Evaluates constraint expressions (e.g., `owner_id == :user_id`)
   - Supports: ownership, tenant, custom expressions, role-conditional filters
   - 2-layer cache: request-level (in-memory) + PostgreSQL

### Implementation Steps
- [ ] Create middleware base class with Strawberry integration
- [ ] Implement filter resolution from permissions table
- [ ] Add request-level caching for resolved filters
- [ ] Implement filter building for each type (ownership, tenant, expression, conditional)
- [ ] Add context storage (`info.context["__row_level_filters__"]`)
- [ ] Create basic unit tests

### Success Criteria
- ✅ Middleware successfully extracts user context
- ✅ Row filters resolved for each table/role combination
- ✅ Cache hit rate > 90% (request-level)
- ✅ Unit tests pass (filter resolution, caching)

---

## PHASE 2: WHERE Clause Integration (2 hours)

### Files to Create
1. `/home/lionel/code/fraiseql/src/fraiseql/enterprise/rbac/auth_where_builder.py` (300 LOC)
   - WHERE clause merging logic
   - Conflict detection (explicit WHERE vs auth filter)
   - Integration with existing `normalize_dict_where()`
   - Validates merged filters don't create unsatisfiable conditions

### Implementation Steps
- [ ] Create `AuthWhereClauseBuilder` class
- [ ] Implement filter merging (AND composition)
- [ ] Add conflict detection and resolution strategy
- [ ] Integrate with WHERE clause normalization pipeline
- [ ] Handle edge cases (null values, empty filters)
- [ ] Create integration tests with WHERE clauses

### Success Criteria
- ✅ Explicit WHERE clauses merged with auth filters correctly
- ✅ Conflicts detected and handled per configuration
- ✅ Merged WHERE clauses pass through Rust normalization
- ✅ Integration tests with real GraphQL queries pass

---

## PHASE 3: Configuration & Setup (1 hour)

### Files to Create
1. `/home/lionel/code/fraiseql/src/fraiseql/enterprise/rbac/row_auth_config.py` (250 LOC)
   - Configuration schema for row-level auth policies
   - Support YAML/JSON/Python dict configurations
   - Per-table and per-role filter definitions
   - Environment-specific overrides

### Implementation Steps
- [ ] Define configuration schema (dataclass/Pydantic)
- [ ] Create YAML/JSON config loader
- [ ] Implement environment variable overrides
- [ ] Add configuration validation
- [ ] Create example config file

### Success Criteria
- ✅ Configuration loads from YAML/JSON/Python dict
- ✅ Invalid configs raise clear errors
- ✅ Environment overrides work correctly

---

## PHASE 4: Audit & Compliance (1-2 hours)

### Files to Create
1. `/home/lionel/code/fraiseql/src/fraiseql/enterprise/rbac/row_auth_auditor.py` (350 LOC)
   - Audit logging for all authorization decisions
   - Integration with FraiseQL audit module
   - Compliance reporting
   - Filter bypass detection

### Implementation Steps
- [ ] Create audit event schema (timestamp, user, table, filters, result)
- [ ] Implement logging to FraiseQL audit system
- [ ] Add bypass attempt detection
- [ ] Create compliance report generator
- [ ] Document audit event structure

### Success Criteria
- ✅ All authorization decisions logged
- ✅ Audit events contain full filter information
- ✅ Bypass attempts detected and logged
- ✅ Compliance reports can be generated

---

## PHASE 5: Testing & Documentation (2-3 hours)

### Files to Create
1. `/home/lionel/code/fraiseql/tests/integration/enterprise/rbac/test_row_level_auth_middleware.py` (500 LOC)
   - Unit tests for each middleware component

2. `/home/lionel/code/fraiseql/tests/integration/enterprise/rbac/test_row_level_security_integration.py` (600 LOC)
   - End-to-end integration tests with real GraphQL queries

3. `/home/lionel/code/fraiseql/tests/security/test_row_auth_bypass.py` (400 LOC)
   - Security-focused tests for bypass attempts

### Unit Tests to Implement
- [ ] Filter resolution for single/multiple roles
- [ ] Filter caching and cache invalidation
- [ ] Missing user context handling
- [ ] Missing table config handling
- [ ] WHERE clause merging logic
- [ ] Conflict detection
- [ ] Null safety

### Integration Tests to Implement
- [ ] GraphQL query with row filters applied
- [ ] Filters applied to nested queries
- [ ] Filters with mutations
- [ ] Performance benchmarks (<10ms overhead)

### Security Tests to Implement
- [ ] Explicit WHERE override prevention
- [ ] NULL owner bypass prevention
- [ ] Context tampering prevention
- [ ] Permission escalation prevention
- [ ] Unauthorized access denial

### Documentation
- [ ] Architecture diagram and description
- [ ] Configuration guide with examples
- [ ] Developer usage guide
- [ ] Security considerations document

---

## INTEGRATION POINTS

### Minimal changes to existing files:

1. **Modify**: `src/fraiseql/enterprise/rbac/__init__.py` (10 LOC)
   - Export new middleware classes

2. **Modify**: `src/fraiseql/core/graphql_type.py` (20 LOC)
   - Add row-level filter resolution in query resolvers
   - Merge filters using `AuthWhereClauseBuilder`

3. **Modify**: `src/fraiseql/fastapi/app.py` (10 LOC)
   - Register RowLevelAuthMiddleware in middleware stack
   - Load configuration on startup

4. **Modify**: `src/fraiseql/enterprise/rbac/middleware.py` (5 LOC)
   - Document middleware stacking order
   - Ensure RbacMiddleware is called first

---

## CONFIGURATION EXAMPLE

```python
ROW_LEVEL_AUTH_CONFIG = {
    "enabled": True,
    "default_strategy": "deny",  # deny-by-default for security

    "tables": {
        "documents": {
            "strategies": [
                {
                    "role": ["super_admin", "admin"],
                    "apply_filter": False,  # Admins see all
                },
                {
                    "role": ["manager"],
                    "filter": {
                        "AND": [
                            {"tenant_id": {"eq": "{user_tenant_id}"}},
                            {"status": {"nin": ["deleted", "archived"]}},
                        ]
                    }
                },
                {
                    "role": ["user"],
                    "filter": {
                        "AND": [
                            {"owner_id": {"eq": "{user_id}"}},
                            {"status": {"in": ["published", "shared"]}},
                        ]
                    }
                },
            ]
        }
    }
}
```

---

## USAGE EXAMPLE

```python
# Client code (NO CHANGES NEEDED - automatic!)
query = """
    query {
        documents(where: {status: {eq: "active"}}) {
            id
            name
            owner { id name }
        }
    }
"""

# What happens behind the scenes:
# 1. User context extracted: user_id = "550e8400...", tenant_id = "550e8401..."
# 2. Middleware resolves: owner_id = {user_id}
# 3. Merges with explicit where: status = "active"
# 4. Final SQL:
#    WHERE owner_id = $1 AND status = "active"
# 5. Returns only user's active documents
```

---

## PERFORMANCE TARGETS

| Metric | Target | Expected |
|--------|--------|----------|
| Filter resolution (cached) | <1ms | 0.5ms |
| Filter resolution (uncached) | <10ms | 5-8ms |
| WHERE merge overhead | <0.5ms | 0.2ms |
| Cache hit rate | >80% | 85-95% |
| Memory per filter | <500 bytes | 300 bytes |
| Total query latency overhead | <2ms | 1.5ms (cached) |

---

## TESTING CHECKLIST

### Unit Tests
- [ ] Filter resolution for different role types
- [ ] Caching behavior and invalidation
- [ ] WHERE clause merging with various scenarios
- [ ] Conflict detection and handling
- [ ] Null value handling
- [ ] Empty filter handling

### Integration Tests
- [ ] End-to-end GraphQL query execution with filters
- [ ] Nested query filtering
- [ ] Mutation filtering (UPDATE/DELETE)
- [ ] Performance benchmarks
- [ ] Real database interaction

### Security Tests
- [ ] Explicit WHERE override prevention
- [ ] NULL/empty filter bypass attempts
- [ ] Context tampering detection
- [ ] Permission escalation attempts
- [ ] Unauthorized access prevention

### Edge Cases
- [ ] User with multiple roles
- [ ] User with no roles
- [ ] Missing table configuration
- [ ] Missing user context
- [ ] Complex nested WHERE clauses
- [ ] Large result sets (performance)

---

## ROLLOUT STRATEGY

### Phase A: Backward Compatible (Zero Risk)
- Deploy with `enabled: false`
- No impact on existing queries
- No performance overhead
- Prepare production readiness

### Phase B: Gradual Rollout
- Enable for specific tables (non-critical first)
- Monitor audit logs
- Verify filter correctness
- Adjust thresholds if needed

### Phase C: Full Production
- Enable for all tables
- Monitor performance metrics
- Update documentation
- Retire manual WHERE clauses

---

## SUCCESS CRITERIA (Overall)

✅ All row-level auth filters automatically injected
✅ No changes needed to application code
✅ <2ms query latency overhead
✅ 100% audit coverage for all authorization decisions
✅ Zero data exposure incidents
✅ Full test coverage (unit + integration + security)
✅ Clear documentation for developers and operators

---

## TIMELINE

| Phase | Duration | Effort |
|-------|----------|--------|
| Phase 1: Core Middleware | 2-3 hrs | Highest complexity |
| Phase 2: WHERE Integration | 2 hrs | Medium complexity |
| Phase 3: Configuration | 1 hr | Low complexity |
| Phase 4: Audit | 1-2 hrs | Medium complexity |
| Phase 5: Testing & Docs | 2-3 hrs | High effort, medium complexity |
| **Total** | **6-8 hrs** | **Manageable** |

---

## CRITICAL SECURITY NOTES

1. **Deny-by-default**: Users get no access unless explicitly granted
2. **Auth always applies**: Explicit WHERE clauses cannot bypass row filters
3. **Immutable context**: User context extracted from JWT, read-only in middleware
4. **Full auditability**: Every authorization decision logged with full filter info
5. **No filter conflicts**: Invalid configurations caught at startup

---

## NEXT STEPS

1. Review and approve this implementation plan
2. Create feature branch: `feature/row-level-auth-middleware`
3. Begin Phase 1: Core Middleware & Filter Resolution
4. Follow with Phases 2-5 in sequence
5. Create PR when all phases complete
6. Code review focused on security
7. Deploy with gradual rollout strategy
