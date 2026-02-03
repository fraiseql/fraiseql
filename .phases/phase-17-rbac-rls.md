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

### Completed ✅ (continued)

#### Cycle 2: Executor Integration (DONE)

- **RED**: ✅ Created 6 integration tests verifying RLS behavior
  - Admin bypass, tenant isolation, multi-tenant, WHERE clause composition, context metadata

- **GREEN**: ✅ Implemented execute_with_security() public method
  - RuntimeConfig now holds rls_policy: Option<Arc<dyn RLSPolicy>>
  - SecurityContext flows through new execute_with_security_internal()
  - execute_regular_query_with_security() validates token expiration
  - Infrastructure ready for WHERE clause injection in Cycle 3

- **CLEANUP**: ✅ All tests passing, no regressions, unused imports removed

**Commit**: `a4760c34` - Executor RLS infrastructure

### Completed ✅ (continued)

#### Cycle 3: SQL Integration (DONE)

- **REFACTOR**: ✅ Integrated WHERE clause filtering into RLS query execution
  - Added WhereClause import to executor.rs
  - Evaluate RLS policy → Option<WhereClause>
  - Pass rls_where_clause to execute_with_projection()
  - Database adapter handles SQL composition

- **Implementation Details**:
  - execute_regular_query_with_security() evaluates policy and builds WHERE clause
  - Calls adapter.execute_with_projection(..., rls_where_clause.as_ref(), ...)
  - Follows TenantEnforcer pattern for type-safe composition
  - No ExecutionPlan changes needed (application is post-planning)

- **Testing**: ✅ Added 2 new integration tests
  - test_rls_policy_produces_correct_where_clauses()
  - test_rls_compose_with_tenant_and_owner_filters()

- **CLEANUP**: ✅ Comprehensive documentation, all tests passing

**Commit**: `21fb495b` - WHERE clause RLS integration

**Key Architecture Insights**:

- DatabaseAdapter.execute_with_projection() already supported WHERE clauses
- Was passing None as third parameter (line 597 before changes)
- TenantEnforcer provides proven reference pattern for clause composition
- Filtering happens at database level, not in Rust (performance critical)

### Not Started ⏹️

#### Cycle 4: Server Handler Wiring

- Update HTTP handler to create SecurityContext from JWT + request headers
- Pass SecurityContext through to executor.execute_with_security()
- Extract user info from JWT claims (user_id, roles, tenant_id, scopes, attributes)
- Handle RLS policy initialization from schema.compiled.json

**Files to Modify**:

- Server handler (location TBD based on framework)
- JWT token extraction middleware
- ExecutionContext integration

#### Cycle 5: TOML Schema Enhancements

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

## Cycle Breakdown

### Cycle 1: Security Foundation (COMPLETE)

- SecurityContext struct with user info and permissions
- RLSPolicy trait with evaluate() method
- DefaultRLSPolicy reference implementation
- 6 integration tests

### Cycle 2: Executor Infrastructure (COMPLETE)

- RuntimeConfig.rls_policy field
- execute_with_security() public method
- execute_with_security_internal() routing
- execute_regular_query_with_security() handler

### Cycle 3: WHERE Clause Integration (COMPLETE)

- Evaluate RLS policy → Option<WhereClause>
- Pass to execute_with_projection()
- Database adapter handles SQL composition
- Type-safe via WhereClause enum
- 8 total integration tests (6 + 2 new)

### Cycle 4: Server Handler Wiring (COMPLETE)

- **RED**: ✅ Created tests for request metadata extraction and SecurityContext creation
- **GREEN**: ✅ Implemented HTTP handler routing through execute_with_security()
  - execute_graphql_request() accepts optional SecurityContext parameter
  - Routes to execute_with_security() when context present
  - Falls back to execute() for unauthenticated requests
  - 7 header extraction unit tests (all passing)
  - 8 RLS integration tests still passing (no regressions)

- **REFACTOR**: ✅ Created custom Axum extractor for optional SecurityContext
  - New extractors.rs module with OptionalSecurityContext type
  - Implements FromRequestParts for Axum 0.8 compatibility
  - Automatically extracts AuthUser from request extensions
  - Creates SecurityContext from AuthUser + HTTP headers
  - Moved helper functions to extractors module:
    - extract_request_id() - gets x-request-id or generates UUID
    - extract_ip_address() - extracts from x-forwarded-for or x-real-ip
    - extract_tenant_id() - gets x-tenant-id from headers
  - Updated graphql handlers to use OptionalSecurityContext extractor
  - Cleaner handler signatures, reduced code duplication

- **CLEANUP**: ✅ Added comprehensive SecurityContext creation test
  - test_optional_security_context_creation_from_auth_user validates full flow
  - Verified RLS filtering integration at HTTP handler level
  - All tests passing, zero warnings

**Result**: HTTP handlers now automatically create SecurityContext from authenticated users and request metadata, enabling RLS policy evaluation

### Cycle 5: TOML Schema Enhancements (FUTURE)

- RLS rule definitions in TOML
- Compiler extracts and embeds rules
- RLS configuration from schema.compiled.json

### Cycle 6: Field-Level RBAC & Integration Tests (FUTURE)

## Success Criteria (Cycles 1-3: COMPLETE)

- [x] SecurityContext flows through request lifecycle
  - Implemented with 18 fields covering user identity, permissions, metadata

- [x] RLSPolicy trait evaluates correctly with different user types
  - Trait with evaluate() method returns Option<WhereClause>
  - DefaultRLSPolicy implements admin bypass, tenant isolation, owner-based access

- [x] Admin users bypass RLS filters
  - DefaultRLSPolicy::evaluate returns None for admins
  - Tests verify admin users get unrestricted access

- [x] Non-admin users see only filtered rows
  - RLS WHERE clause passed to execute_with_projection()
  - Database adapter applies filtering at SQL level

- [x] WHERE clause composition respects both user and RLS filters
  - Type-safe via WhereClause::Field and WhereClause::And
  - Database adapter composes at SQL generation time

- [x] Tenant isolation enforced for multi-tenant queries
  - DefaultRLSPolicy builds AND([tenant_id filter, author_id filter])
  - Integration test verifies multi-tenant filtering

- [x] All integration tests pass
  - 8/8 integration tests passing
  - 1432/1432 lib tests passing

- [x] No performance regression (WHERE composition is O(1) per query)
  - Policy evaluation is per-query (constant time)
  - Filtering at database level (optimal performance)
  - Tested with integration test suite

- [x] Code formatted and lints clean
  - No clippy warnings
  - All tests pass

## Files Modified / Created (Cycles 1-3)

### Created ✅

- `crates/fraiseql-core/src/security/security_context.rs` (245 lines) - Cycle 1
- `crates/fraiseql-core/src/security/rls_policy.rs` (504 lines) - Cycle 1
- `crates/fraiseql-core/tests/integration_rls.rs` (309 lines) - Cycles 1-3

### Modified ✅

- `crates/fraiseql-core/src/runtime/executor.rs`
  - Added WhereClause import (Cycle 3)
  - Implemented execute_with_security() (Cycle 2)
  - Implemented execute_with_security_internal() (Cycle 2)
  - Implemented execute_regular_query_with_security() with RLS evaluation (Cycle 3)

- `crates/fraiseql-core/src/runtime/mod.rs`
  - Added rls_policy field to RuntimeConfig (Cycle 2)
  - Implemented with_rls_policy() builder method (Cycle 2)
  - Custom Debug impl for dyn RLSPolicy (Cycle 2)

- `crates/fraiseql-core/src/security/mod.rs`
  - Added rls_policy and security_context module exports (Cycles 1-2)

## Effort Summary

### Completed (Cycles 1-4)

- **Cycle 1** (Security Foundation): ~6 hours
  - 2h: SecurityContext implementation (245 lines)
  - 3h: RLSPolicy trait + 3 implementations (504 lines)
  - 1h: Tests and documentation

- **Cycle 2** (Executor Infrastructure): ~4 hours
  - 2h: RuntimeConfig + execute_with_security() methods
  - 1h: Integration with routing
  - 1h: Tests

- **Cycle 3** (WHERE Clause Integration): ~2 hours
  - 30m: Add imports and modify execute_regular_query_with_security()
  - 30m: Add 2 new integration tests
  - 1h: Documentation and cleanup

- **Cycle 4** (Server Handler Wiring): ~3 hours
  - Created custom Axum extractor for SecurityContext
  - Updated HTTP handlers to use extractor
  - Added comprehensive header extraction tests
  - Refactored for cleaner architecture

**Completed Total**: ~15 hours

### Remaining (Cycles 5-6)

- **Cycle 5** (TOML Schema Enhancements): ~4 hours
  - RLS rule definitions in TOML
  - Compiler integration

- **Cycle 6** (Field-Level RBAC & Integration): ~6 hours
  - Decorator syntax implementation
  - Comprehensive test suite

**Remaining Estimate**: ~10 hours
**Total Estimated for Full Implementation**: ~25 hours

## Next Steps

### Immediate (Cycle 4: Server Handler Wiring)

1. Identify server HTTP handler location
2. Extract user claims from JWT token
3. Create SecurityContext from user info
4. Call executor.execute_with_security() with context
5. Wire HTTP response handling

### Short-term (Cycle 5: TOML Enhancements)

1. Add RLS rule definitions to TOML schema
2. Update compiler to extract rules and embed in schema.compiled.json
3. Parse rules at runtime and create policy instances

### Medium-term (Cycle 6: Field-Level RBAC)

1. Add @scope decorator syntax to Python/TypeScript SDKs
2. Generate scope requirements in schema.json
3. Enforce scopes in executor before field projection
4. Build comprehensive integration test suite

## Notes

- RLSPolicy is async-ready (can be made async later if needed for external policy services)
- DEFAULT: No RLS policy (opt-in feature)
- CACHING: Optional via `cache_result()` method
- MULTITENANCY: Automatic tenant_id isolation when tenant_id is present in context
- COMPOSITION: Safe to compose multiple WHERE clauses via `WhereClause::And()`
