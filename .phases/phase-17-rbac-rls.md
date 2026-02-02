# Phase 17: Field-Level RBAC and Row-Level Security (RLS)

## Objective

Implement runtime authorization for GraphQL queries through:
1. **Field-Level RBAC**: Scope-based access control (already partially implemented via JWT filtering)
2. **Row-Level Security (RLS)**: Tenant isolation and owner-based record access (new in this phase)

Both features integrate with the compilation-first architecture, respecting the boundary where policies are defined at authoring time (fraiseql.toml → schema.compiled.json) and evaluated at runtime with SecurityContext.

## Current Status

### Completed ✅

#### Cycle 1: Security Foundation (DONE)
- **RED**: ✅ Created test files for SecurityContext and RLSPolicy
- **GREEN**: ✅ Implemented SecurityContext struct with:
  - User identity: `user_id`, `roles`, `tenant_id`
  - Permissions: `scopes`, `attributes`
  - Request info: `request_id`, `ip_address`, `issuer`, `audience`
  - Helper methods: `has_role()`, `has_scope()` (with wildcard support), `is_admin()`
  - Builder pattern for testing

- **GREEN**: ✅ Implemented RLSPolicy trait with:
  - `evaluate(&self, context: &SecurityContext, type_name: &str) -> Result<Option<WhereClause>>`
  - `cache_result()` for performance optimization
  - Three reference implementations:
    - **DefaultRLSPolicy**: Admin bypass + tenant isolation + owner-based access
    - **NoRLSPolicy**: No-op policy for open APIs
    - **CompiledRLSPolicy**: Rules from schema.compiled.json (skeleton)

- **REFACTOR**: ✅ Type-safe design using `Option<WhereClause>` for composition
- **CLEANUP**: ✅ All tests passing, code formatted, security module exports updated

**Commit**: `56ce5534` - SecurityContext and RLSPolicy foundation

### In Progress 🔄

#### Cycle 2: Executor Integration (RED PHASE)

**Objective**: Wire SecurityContext and RLSPolicy into the query executor so RLS filters are applied before SQL execution.

**Architecture**:

```
HTTP Request with Authorization header
     ↓
AuthMiddleware → AuthenticatedUser
     ↓
SecurityContext (created from AuthenticatedUser + request metadata)
     ↓
Executor::execute_regular_query()
     ├─ Match query → QueryDefinition
     ├─ Create execution plan → ExecutionPlan
     ├─ Apply RLS filter:
     │   ├─ Call RLSPolicy::evaluate(&context, &type_name)
     │   ├─ Compose with user WHERE clause: WhereClause::And(vec![user_where, rls_filter])
     │   └─ Inject into execution plan
     ├─ Generate SQL with composed WHERE clause
     └─ Execute against database
```

**Key Integration Point** (crates/fraiseql-core/src/runtime/executor.rs):

In `execute_regular_query()` (line 432):
```rust
// After: let plan = self.planner.plan(&query_match)?;
// Before: SQL execution

if let Some(ref rls_policy) = self.rls_policy {
    let rls_filter = rls_policy.evaluate(&context, &type_name)?;
    plan.where_clause = match plan.where_clause {
        None => rls_filter,
        Some(user_where) => Some(WhereClause::And(vec![user_where, rls_filter])),
    };
}
```

**Files to Modify**:
1. **Executor struct** (`executor.rs:96`): Add `rls_policy: Option<Arc<dyn RLSPolicy>>`
2. **Executor::new()** (`executor.rs:134`): Initialize RLS policy from config
3. **Executor::with_config()** (`executor.rs:146`): Pass RLS policy to constructor
4. **execute_regular_query()** (`executor.rs:432`): Apply RLS filter before SQL execution
5. **RuntimeConfig** (`runtime/mod.rs`): Add `rls_policy` field with default
6. **Tests**: Add integration test showing RLS filtering in action

**TDD Approach**:
- RED: Write test that verifies non-admin user can only see their own records
- GREEN: Implement minimal RLS integration in executor
- REFACTOR: Extract WHERE clause composition into helper method
- CLEANUP: Add documentation and handle edge cases

### Not Started ⏹️

#### Cycle 3: Server Handler Wiring
- Update HTTP handler to create SecurityContext from JWT + request headers
- Pass SecurityContext through to executor
- Handle RLS policy initialization from schema.compiled.json

#### Cycle 4: TOML Schema Enhancements
- Add RLS rule definitions to TOML schema
- Update compiler to extract rules from TOML and embed in schema.compiled.json
- Schema structure:
  ```toml
  [[security.rules]]
  name = "own_posts_only"
  rule = "user.id == object.author_id"
  cacheable = true
  cache_ttl_seconds = 300

  [[security.policies]]
  name = "read_own_posts"
  type = "rls"
  rules = ["own_posts_only"]
  description = "Users can only read their own posts"
  ```

#### Cycle 5: Field-Level RBAC Completion
- **Current State**: Scope-based JWT filtering exists but decorator syntax is missing
- **Gap**: No `@scope` decorator in Python/TypeScript SDKs
- **Implementation**:
  - Add `@scope("read:User.email")` decorator syntax
  - Generate scope requirements in schema.json
  - Enforce in executor before field projection
  - Integration with FieldFilter (already in security module)

#### Cycle 6: Integration Tests
- Multi-tenant RLS test suite
- Owner-based access control tests
- Admin bypass verification tests
- Combined RBAC + RLS tests (scope + row-level)
- Performance tests for WHERE clause composition

## Architecture Compliance

✅ **Compilation-First Boundary Respected**:
- Policies defined in fraiseql.toml → embedded in schema.compiled.json
- Runtime evaluation with actual SecurityContext (user, tenant, attributes)
- No dynamic policy changes (immutable compiled schema)

✅ **Type Safety**:
- SecurityContext carries all user info
- RLSPolicy trait for pluggable policies
- WHERE clause composition via `WhereClause::And()` (type-safe)

✅ **No Breaking Changes**:
- RLS policy is optional (config defaults to None)
- Existing queries work without RLS
- Backward compatible with current field filtering

✅ **Performance**:
- WHERE clause composition happens once per query
- Optional caching via `RLSPolicy::cache_result()`
- Indexes on tenant_id, author_id support RLS queries

## Dependencies

- **Requires**: Phase 7+ (security infrastructure already in place)
- **Blocks**: Phase 19+ (GraphQL subscriptions with RLS)
- **Affected**: All runtime query execution paths

## TDD Cycles

### RED Phase (Current)
Test: `test_executor_applies_rls_filter_for_non_admin()`
```rust
// Non-admin user should only see their own posts
let policy = DefaultRLSPolicy::new();
let context = SecurityContext { user_id: "user1", roles: vec!["user"] };
let results = executor.execute_with_context(query, Some(variables), &context).await?;
// Should only contain posts with author_id == "user1"
```

### GREEN Phase
Implement minimal RLS in executor:
```rust
// In execute_regular_query()
if let Some(ref rls) = self.rls_policy {
    let filter = rls.evaluate(&context, &type_name)?;
    // Compose with user WHERE clause
}
```

### REFACTOR Phase
Extract WHERE clause composition:
```rust
fn compose_where_clauses(user_where: Option<WhereClause>, rls_filter: Option<WhereClause>) -> Option<WhereClause> {
    match (user_where, rls_filter) {
        (None, rls) => rls,
        (user, None) => user,
        (Some(u), Some(r)) => Some(WhereClause::And(vec![u, r])),
    }
}
```

### CLEANUP Phase
- Remove debug logging
- Add edge case handling (multiple policies, complex filters)
- Update module documentation
- Verify all tests pass

## Success Criteria

- [ ] SecurityContext flows through request lifecycle
- [ ] RLSPolicy evaluates correctly with different user types
- [ ] Admin users bypass RLS filters
- [ ] Non-admin users see only filtered rows
- [ ] WHERE clause composition respects both user and RLS filters
- [ ] Tenant isolation enforced for multi-tenant queries
- [ ] All integration tests pass
- [ ] No performance regression (WHERE composition is O(1) per query)
- [ ] Code formatted and lints clean

## Files Modified / Created

### Created ✅
- `crates/fraiseql-core/src/security/security_context.rs` (245 lines)
- `crates/fraiseql-core/src/security/rls_policy.rs` (504 lines)

### Will Modify
- `crates/fraiseql-core/src/runtime/executor.rs` (add RLS policy field + integration)
- `crates/fraiseql-core/src/runtime/mod.rs` (RuntimeConfig + RLS policy)
- `crates/fraiseql-core/src/security/mod.rs` (re-exports)

### Will Create
- `crates/fraiseql-core/tests/integration_rls.rs` (integration tests)

## Estimated Effort

- **Cycle 2** (Executor): 4-5 hours
  - 2h: Code modification + testing
  - 2h: Integration tests
  - 1h: Documentation

- **Cycle 3** (Server): 3-4 hours
  - 2h: HTTP handler updates
  - 1h: SecurityContext creation
  - 1h: Tests

- **Cycle 4** (TOML): 3-4 hours
  - 2h: Schema updates
  - 1h: Compiler changes
  - 1h: Tests

- **Cycle 5** (RBAC): 5-7 hours
  - 3h: Decorator implementation
  - 2h: Executor integration
  - 2h: Tests

- **Cycle 6** (Integration): 4-6 hours

**Total**: ~19-26 hours

## Next Step

Start Cycle 2 (Executor Integration):
1. Add `rls_policy` field to Executor struct
2. Modify `execute_regular_query()` to apply RLS filter
3. Write integration test verifying RLS behavior
4. Run full test suite to ensure no regressions

## Notes

- RLSPolicy is async-ready (can be made async later if needed for external policy services)
- DEFAULT: No RLS policy (opt-in feature)
- CACHING: Optional via `cache_result()` method
- MULTITENANCY: Automatic tenant_id isolation when tenant_id is present in context
- COMPOSITION: Safe to compose multiple WHERE clauses via `WhereClause::And()`
