# APQ Tenant Context Support - COMPLEX

**Complexity**: Complex | **Phased TDD Approach**

## Executive Summary
Enable APQ backends to access request context for tenant-specific response caching while maintaining backward compatibility. This solves the critical multi-tenant caching issue where responses contain tenant-specific data.

## PHASES

### Phase 1: Backward-Compatible Context Interface
**Objective**: Add optional context parameter to APQ backend methods without breaking existing implementations

#### TDD Cycle:
1. **RED**: Write failing test for context-aware backend methods
   - Test file: `tests/storage/backends/test_context_aware_backend.py`
   - Expected failure: Methods should accept context parameter

2. **GREEN**: Implement minimal code to pass
   - Files to modify: `src/fraiseql/storage/backends/base.py`
   - Minimal implementation: Add context parameter with default None

3. **REFACTOR**: Clean up and optimize
   - Code improvements: Type hints, documentation
   - Pattern compliance: Follow existing backend patterns

4. **QA**: Verify phase completion
   - [ ] All existing backends still work
   - [ ] New context parameter is optional
   - [ ] Type hints are correct
   - [ ] Documentation updated

### Phase 2: Context Propagation from Router
**Objective**: Pass GraphQL context to APQ backend methods when available

#### TDD Cycle:
1. **RED**: Write failing test for context propagation
   - Test file: `tests/integration/test_apq_context_propagation.py`
   - Expected failure: Context not passed to backend methods

2. **GREEN**: Implement context passing in router
   - Files to modify: `src/fraiseql/fastapi/routers.py`
   - Minimal implementation: Pass context to backend methods

3. **REFACTOR**: Ensure clean integration
   - Code improvements: Extract context safely
   - Pattern compliance: Maintain router structure

4. **QA**: Verify phase completion
   - [ ] Context flows to backend methods
   - [ ] Backward compatibility maintained
   - [ ] No performance regression
   - [ ] Integration tests pass

### Phase 3: Tenant-Aware Response Caching
**Objective**: Implement tenant-specific response caching using context

#### TDD Cycle:
1. **RED**: Write failing test for tenant-specific caching
   - Test file: `tests/integration/test_tenant_specific_caching.py`
   - Expected failure: Responses not isolated by tenant

2. **GREEN**: Implement tenant-aware caching logic
   - Files to modify: `src/fraiseql/storage/backends/memory.py`, `postgresql.py`
   - Minimal implementation: Use tenant_id in cache key

3. **REFACTOR**: Optimize caching strategy
   - Code improvements: Efficient key generation
   - Pattern compliance: Cache invalidation patterns

4. **QA**: Verify phase completion
   - [ ] Tenant isolation works correctly
   - [ ] No cross-tenant data leakage
   - [ ] Cache invalidation per tenant
   - [ ] Performance benchmarks

### Phase 4: Documentation and Examples
**Objective**: Document the new context-aware APQ backend capabilities

#### TDD Cycle:
1. **RED**: Write failing documentation tests
   - Test file: `tests/docs/test_apq_context_examples.py`
   - Expected failure: Examples don't demonstrate context usage

2. **GREEN**: Create working examples
   - Files to create: `examples/apq_multi_tenant.py`
   - Minimal implementation: Basic multi-tenant example

3. **REFACTOR**: Polish documentation
   - Improvements: Clear explanations, best practices
   - Pattern compliance: Consistent with other docs

4. **QA**: Verify phase completion
   - [ ] Examples run successfully
   - [ ] Documentation is clear
   - [ ] Migration guide included
   - [ ] API reference updated

## Success Criteria
- [ ] All existing APQ backends continue to work (backward compatible)
- [ ] Context can be passed to and used by APQ backends
- [ ] Tenant-specific response caching is possible
- [ ] No performance degradation for existing use cases
- [ ] Comprehensive test coverage for new functionality
- [ ] Clear documentation and examples
