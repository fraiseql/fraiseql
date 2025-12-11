# Mutation Schema Fix - Auto-Populate Fields Missing from GraphQL Schema

## 🚨 Issue Summary

**Severity**: HIGH - Blocks v1.8.0 production adoption
**Affected**: Mutation success/failure types with auto-populated fields
**Impact**: 138+ tests failing in PrintOptim backend

### The Problem

FraiseQL v1.8.0's auto-populate feature (commit f64db8ac) adds mutation response fields (`status`, `message`, `errors`) at runtime, but these fields are **NOT included in the GraphQL schema**. This causes:

1. **Schema validation errors** when trying to query auto-populated fields
2. **GraphQL spec violations** - fields appear in responses without being requested
3. **Cannot use in production** - queries fail with "Cannot query field 'X' on type 'Y'"

### Evidence

```python
# What developers expect (per CHANGELOG):
mutation {
  createMachine(input: {...}) {
    ... on CreateMachineSuccess {
      status      # ✅ Should work
      message     # ✅ Should work
      errors { code message }  # ✅ Should work
      machine { id }
    }
  }
}
```

**Actual behavior**:
- Querying `status`, `message`, `errors` → GraphQL error: "Cannot query field 'status' on type 'CreateMachineSuccess'"
- NOT querying them → They appear in response anyway (GraphQL spec violation)

### Test Results from PrintOptim

```
Test 1: Minimal query (only entity)
✅ Returns: ['__typename', 'id', 'machine', 'message', 'updatedFields']
   Problem: Fields not requested but returned anyway

Test 2: Query with 'id' field explicitly
❌ Error: "Cannot query field 'id' on type 'CreateMachineSuccess'"
   Problem: Field in response but not in schema

Test 3-5: Query status/message/errors
✅ No error, but fields silently ignored in schema validation
   Problem: Fields not actually queryable, just lucky they don't error
```

## 📁 Phase Documentation

This fix is broken into phases for safe implementation:

1. **[Phase 1: Root Cause Analysis](./phase-1-root-cause.md)** - Deep dive into architecture
2. **[Phase 2: Fix Implementation](./phase-2-fix-implementation.md)** - Code changes with examples
3. **[Phase 3: Testing Strategy](./phase-3-testing.md)** - Comprehensive test coverage
4. **[Phase 4: Migration Guide](./phase-4-migration.md)** - Backward compatibility

## 🎯 Goals

1. ✅ Auto-populated fields appear in GraphQL schema
2. ✅ Fields are queryable via GraphQL
3. ✅ Backward compatible with existing decorators
4. ✅ No breaking changes to public API
5. ✅ Comprehensive test coverage

## 📊 Success Criteria

- [ ] All auto-populated fields queryable in GraphQL introspection
- [ ] PrintOptim 138 tests pass
- [ ] Existing FraiseQL test suite passes
- [ ] No GraphQL spec violations
- [ ] Documentation updated

## 🔗 Related Files

**Key Files to Modify**:
- `src/fraiseql/mutations/decorators.py` - Where fields are auto-injected
- `src/fraiseql/types/constructor.py` - Where `__gql_fields__` is populated
- `src/fraiseql/core/graphql_type.py` - Where GraphQL schema is generated

**Test Files**:
- `tests/unit/mutations/test_auto_populate_fields.py` - Unit tests for decorator
- `tests/integration/test_mutation_schema_generation.py` - Integration tests

## 📝 Review Checklist

Before implementing:
- [ ] Senior architect reviewed root cause analysis
- [ ] Proposed fix approach approved
- [ ] Testing strategy validated
- [ ] Migration path confirmed
- [ ] No alternative simpler solution exists

## 🚀 Implementation Order

1. Review Phase 1 (root cause) - **START HERE**
2. Review Phase 2 (fix) - Approve approach
3. Review Phase 3 (testing) - Validate coverage
4. Review Phase 4 (migration) - Confirm compatibility
5. Implement fix with TDD
6. Run full test suite
7. Manual verification with PrintOptim
8. Commit and document

---

**Next**: Start with [Phase 1: Root Cause Analysis](./phase-1-root-cause.md)
