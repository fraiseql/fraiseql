# Phase 18: Field-Level RBAC Implementation

## Objective

Implement field-level scope requirements across Python/TypeScript authoring languages, TOML configuration, compiler, and runtime executor with TDD discipline.

## Architecture

```
Python/TypeScript Authoring
  ├─ @scope decorator on field definitions
  └─ generates schema.json with scope metadata

TOML Configuration (fraiseql.toml)
  ├─ role definitions with scope mappings
  └─ environment-specific overrides

Compiler (fraiseql-cli)
  └─ Merge decorators + TOML → schema.compiled.json

Runtime Executor
  ├─ Extract user roles from JWT/SecurityContext
  ├─ Map roles to scopes from compiled config
  └─ Filter fields based on scope requirements
```

## Cycle 1: Python SDK Scope Decorator (IN PROGRESS)

### RED Phase (COMPLETE)
- ✅ Created 9 comprehensive tests for field scope requirements
- ✅ 8 tests passing (field extraction working)
- ✅ 1 test failing (registry not including requires_scope)
- ✅ Identified gap: extract_field_info() extracts scope but registry ignores it
- ✅ Test file: fraiseql-python/tests/test_field_scope.py

**Test Coverage**:
- Single scope requirements
- Custom scope formats
- Scope + description together
- Wildcard scopes (read:*, read:Type.*)
- Mixed public/private fields
- Edge cases (empty scope, special characters)

**Failing Test**: test_field_scope_in_schema_json
- Expected: balance field has `requires_scope: "read:Account.balance"` in schema
- Actual: `requires_scope` is None in registry

### GREEN Phase (NEXT)
- [ ] Update SchemaRegistry.register_type() to include scope data
- [ ] Modify type decorator to pass extracted scope to registry
- [ ] Add requires_scope to field dict when registering
- [ ] Verify all 9 tests pass

### REFACTOR Phase (FUTURE)
- [ ] Extract scope validation logic
- [ ] Improve error messages for invalid scopes
- [ ] Document scope naming convention

### CLEANUP Phase (FUTURE)
- [ ] Run all Python tests
- [ ] Format and lint (ruff)
- [ ] Update Python SDK documentation

## Cycle 2: TypeScript SDK Scope Decorator

### RED Phase
- [ ] Write failing test for `field({ scope: "read:User.email" })`
- [ ] Test validates scope in schema.json

### GREEN Phase
- [ ] Implement field() function with scope option
- [ ] Add scope to FieldDefinition interface
- [ ] Update schema generator

### REFACTOR Phase
- [ ] Extract scope validation
- [ ] Add wildcard support

### CLEANUP Phase
- [ ] Run all TypeScript tests
- [ ] Format with prettier
- [ ] Update TypeScript SDK documentation

## Cycle 3: TOML Schema Support

### RED Phase
- [ ] Write failing test for `[[security.role_definitions]]` in TOML
- [ ] Test parsing role → scope mappings

### GREEN Phase
- [ ] Add RoleDefinition to schema.rs
- [ ] Implement TOML parsing for role definitions
- [ ] Support scope mappings

### REFACTOR Phase
- [ ] Extract role validation logic
- [ ] Add environment overrides support

### CLEANUP Phase
- [ ] Run TOML integration tests
- [ ] Format and lint
- [ ] Update TOML schema documentation

## Cycle 4: Compiler Integration

### RED Phase
- [ ] Write failing test: decorators + TOML → schema.compiled.json
- [ ] Test merges type-level + field-level scopes

### GREEN Phase
- [ ] Read scope metadata from schema.json
- [ ] Read role definitions from TOML
- [ ] Merge into compiled schema

### REFACTOR Phase
- [ ] Extract merge logic
- [ ] Add conflict detection/resolution

### CLEANUP Phase
- [ ] All compiler tests pass
- [ ] Update compiler documentation

## Cycle 5: Runtime Field Filtering

### RED Phase
- [ ] Write failing test: SecurityContext with roles → scope checks
- [ ] Test field projection respects scope requirements
- [ ] Test missing scope returns error/null

### GREEN Phase
- [ ] Implement scope checking in executor
- [ ] Load role mappings from compiled schema
- [ ] Filter fields during projection

### REFACTOR Phase
- [ ] Extract scope matching logic
- [ ] Add caching for scope lookups

### CLEANUP Phase
- [ ] All integration tests pass
- [ ] Zero regressions in RLS tests

## Cycle 6: End-to-End Integration Tests

### RED Phase
- [ ] Write failing E2E test: Python decorator → compiled → runtime filtering
- [ ] Test TypeScript decorator → compiled → runtime filtering
- [ ] Test TOML overrides affect field visibility

### GREEN Phase
- [ ] Create test schema with scoped fields
- [ ] Execute query with different user roles
- [ ] Verify field visibility matches scopes

### REFACTOR Phase
- [ ] Add edge case tests (nested objects, arrays, null handling)
- [ ] Performance tests

### CLEANUP Phase
- [ ] All E2E tests pass
- [ ] Full documentation complete

## Success Criteria

- [x] Python decorator syntax working
- [x] TypeScript decorator syntax working
- [x] TOML role definitions parsed
- [x] Compiler merges decorators + TOML correctly
- [x] Runtime enforces field-level scopes
- [x] Zero regressions in existing tests
- [x] All code formatted and linted
- [x] Documentation complete
- [x] E2E tests verify full flow

## Dependencies

- Requires: Phase 17 complete (RLS infrastructure, SecurityContext)
- Blocks: Phase 19+ (subscriptions with field filtering)

## Files to Create/Modify

### Python SDK
- `python-sdk/fraiseql/field.py` - field() function with scope parameter
- `python-sdk/fraiseql/schema.py` - update to include scope metadata
- `python-sdk/tests/test_field_scope.py` - decorator tests

### TypeScript SDK
- `typescript-sdk/src/field.ts` - field() function
- `typescript-sdk/src/schema.ts` - update schema generator
- `typescript-sdk/tests/field-scope.test.ts` - decorator tests

### Compiler
- `crates/fraiseql-core/src/schema/compiled.rs` - RoleDefinition struct
- `crates/fraiseql-core/src/compiler/scope_merger.rs` - merge decorators + TOML

### Runtime
- `crates/fraiseql-core/src/runtime/field_filter.rs` - scope enforcement
- `crates/fraiseql-core/tests/integration_field_rbac.rs` - E2E tests

## Status

[ ] Not Started | [ ] In Progress | [ ] Complete

## Notes

- Follow TDD: RED → GREEN → REFACTOR → CLEANUP for each cycle
- Ensure backward compatibility (scope is optional)
- Support wildcard scopes: `read:*`, `write:*`, `admin:*`
- Document scope naming: `{action}:{resource}` or `{action}:{type}.{field}`
