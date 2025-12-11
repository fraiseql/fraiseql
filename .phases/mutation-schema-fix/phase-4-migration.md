# Phase 4: Migration Guide & Backward Compatibility

## 🎯 Migration Goals

1. ✅ Existing code continues to work without changes
2. ✅ No breaking changes to public API
3. ✅ Clear upgrade path for users
4. ✅ Deprecation warnings where needed (if any)

---

## 🔄 Backward Compatibility Analysis

### ✅ No Breaking Changes Expected

The fix **adds fields to the schema** that were already being added to responses by Rust. This means:

1. **Existing queries continue to work** - They don't query the new fields, so no change
2. **Existing decorators unchanged** - `@success` and `@failure` API unchanged
3. **Existing responses unchanged** - Rust already adds these fields
4. **Only change**: Fields now **queryable** in GraphQL (previously caused errors)

### What Users Will Notice

**Before (v1.8.0 broken)**:
```graphql
mutation {
  createMachine(input: {...}) {
    ... on CreateMachineSuccess {
      status  # ❌ Error: "Cannot query field 'status'"
    }
  }
}
```

**After (v1.8.0 fixed)**:
```graphql
mutation {
  createMachine(input: {...}) {
    ... on CreateMachineSuccess {
      status  # ✅ Works! Returns "success"
    }
  }
}
```

---

## 📋 Migration Checklist for Users

### Users Adopting v1.8.0

**No action required!** The fix makes documented features work as expected.

#### Optional: Update Queries to Use New Fields

Users can now query auto-populated fields:

```graphql
mutation {
  createMachine(input: {...}) {
    ... on CreateMachineSuccess {
      # ✅ NEW: Can now query these fields
      status
      message
      errors { code message }
      id
      updatedFields

      # Existing field continues to work
      machine { id name }
    }
  }
}
```

### Users on v1.7.x or Earlier

No changes needed. The fix only affects v1.8.0's auto-populate feature.

---

## 🚨 Potential Issues & Solutions

### Issue 1: User Manually Defined Auto-Populated Fields

**Scenario**: User defined `status`, `message`, or `errors` manually before v1.8.0.

```python
@success
class CreateMachineSuccess:
    machine: Machine
    status: str = "success"  # User-defined
    message: str | None = None  # User-defined
```

**Impact**: None - decorator checks if field exists before adding.

**Code in decorator**:
```python
if "status" not in annotations:  # ✅ Checks first
    annotations["status"] = str
```

**Action**: No migration needed.

---

### Issue 2: Introspection Cache Issues

**Scenario**: GraphQL clients cache schema introspection results.

**Impact**: Clients may not see new fields until cache cleared.

**Solution**: Document cache clearing in release notes.

```bash
# Example: Clear Apollo Client cache
apollo client:codegen --target=typescript --localSchemaFile=schema.graphql

# Example: Refresh GraphQL Playground
Ctrl+R or Cmd+R in browser
```

**Action**: Add to release notes.

---

### Issue 3: Tests Expecting Specific Field Counts

**Scenario**: User tests check exact field count in schema.

```python
# User's test
def test_success_type_fields():
    assert len(CreateMachineSuccess.__gql_fields__) == 1  # ❌ Will fail
```

**Impact**: Test fails because now 6 fields instead of 1.

**Solution**: Update test to check for specific fields, not count.

```python
# Fixed test
def test_success_type_fields():
    assert "machine" in CreateMachineSuccess.__gql_fields__  # ✅ Correct
```

**Action**: Mention in release notes if we find such tests.

---

## 📝 Release Notes Template

### For CHANGELOG.md

```markdown
## [1.8.1] - 2024-XX-XX

### Fixed

- **CRITICAL**: Auto-populated mutation fields (`status`, `message`, `errors`, `id`, `updatedFields`) now properly appear in GraphQL schema and are queryable ([#XXX](link))
  - Previously these fields were added to responses at runtime but caused schema validation errors when queried
  - No breaking changes - existing queries continue to work
  - Users can now query these fields as documented in v1.8.0 CHANGELOG

### Migration Guide

**No action required for most users.** The fix makes v1.8.0's documented auto-populate feature work correctly.

#### Optional: Leverage New Queryable Fields

You can now query auto-populated fields in mutation responses:

```graphql
mutation {
  createMachine(input: {...}) {
    ... on CreateMachineSuccess {
      status        # ✅ Now works (previously errored)
      message       # ✅ Now works
      errors { code message }  # ✅ Now works
      machine { id }
    }
  }
}
```

#### Potential Issues

1. **GraphQL client caches**: Clear introspection cache to see new fields
2. **Tests checking field counts**: Update to check for specific fields instead

See [v1.8.0 CHANGELOG](link) for details on the auto-populate feature.
```

---

## 🧪 Upgrade Testing Checklist

Before releasing fix:

- [ ] Run existing test suite (all tests pass)
- [ ] Run PrintOptim test suite (138 tests now pass)
- [ ] Test with cached GraphQL client (clear cache works)
- [ ] Test introspection queries (new fields visible)
- [ ] Test backward compatibility (old queries still work)
- [ ] Test user-defined field override (no conflicts)
- [ ] Document known issues (if any)

---

## 🔍 Validation Steps for Users

After upgrading to fixed v1.8.0/v1.8.1:

### Step 1: Verify Schema Includes Fields

Run introspection query:

```graphql
query {
  __type(name: "YourSuccessType") {
    fields {
      name
      type {
        name
      }
    }
  }
}
```

Expected output should include:
- `status`
- `message`
- `errors`
- `id` (if entity field present)
- `updatedFields`

### Step 2: Test Querying Fields

```graphql
mutation {
  yourMutation(input: {...}) {
    ... on YourSuccessType {
      status
      message
      errors { code message }
    }
  }
}
```

Should return response without errors.

### Step 3: Verify Existing Queries Still Work

Run existing queries that DON'T query auto-populated fields. They should continue to work unchanged.

---

## 🎓 Best Practices After Fix

### Recommended: Query `status` and `errors`

```graphql
mutation {
  createMachine(input: {...}) {
    ... on CreateMachineSuccess {
      status  # ✅ Always "success" for success types
      machine { id }
    }
    ... on CreateMachineError {
      status  # ✅ Error status code
      message  # ✅ Human-readable error
      errors { code message }  # ✅ Detailed error list
    }
  }
}
```

### Optional: Use `message` for User Feedback

```graphql
mutation {
  createMachine(input: {...}) {
    ... on CreateMachineSuccess {
      message  # ✅ e.g., "Machine created successfully"
      machine { id name }
    }
  }
}
```

Display `message` to users as confirmation feedback.

### Use `updatedFields` for Audit Logs

```graphql
mutation {
  updateMachine(input: {...}) {
    ... on UpdateMachineSuccess {
      updatedFields  # ✅ e.g., ["name", "type", "status"]
      machine { id name type status }
    }
  }
}
```

Track which fields changed for audit/history purposes.

---

## 📊 Monitoring Post-Release

### Metrics to Track

1. **Schema validation errors** - Should decrease to near zero
2. **Mutation success rate** - Should remain stable or improve
3. **Client introspection requests** - May spike briefly (cache refreshes)
4. **User-reported issues** - Monitor for unexpected behaviors

### Known Safe Patterns

✅ These patterns are safe after fix:
```python
# 1. User defines custom status
@success
class MySuccess:
    machine: Machine
    status: str = "custom"  # ✅ Overrides auto-inject

# 2. User defines custom message with field()
@success
class MySuccess:
    machine: Machine
    message: str = fraise_field(description="Custom")  # ✅ Overrides

# 3. No entity field
@success
class DeleteSuccess:
    pass  # ✅ No id field added

# 4. Multiple entity fields
@success
class MySuccess:
    machine: Machine
    user: User  # ✅ id still added correctly
```

---

## ✅ Sign-Off Checklist

Before marking fix complete:

- [ ] All phases reviewed by senior architect
- [ ] Implementation matches Phase 2 spec
- [ ] All Phase 3 tests pass
- [ ] Release notes written
- [ ] Migration guide documented
- [ ] Backward compatibility verified
- [ ] PrintOptim tests pass (external validation)
- [ ] No breaking changes introduced
- [ ] Documentation updated

---

## 🚀 Rollout Plan

### Phase 1: Internal Testing (Complete Before Merge)

- [ ] Run full FraiseQL test suite
- [ ] Run PrintOptim backend tests
- [ ] Manual GraphQL Playground testing
- [ ] Introspection validation

### Phase 2: PR Review

- [ ] Code review by senior architect
- [ ] Security review (if needed)
- [ ] Performance review (negligible impact expected)

### Phase 3: Merge & Release

- [ ] Merge to `dev` branch
- [ ] Tag release (v1.8.1 or v1.8.0-fixed)
- [ ] Update CHANGELOG
- [ ] Create GitHub release with migration notes

### Phase 4: Post-Release

- [ ] Monitor for issues
- [ ] Update documentation site
- [ ] Notify users via release announcement
- [ ] Close related issues

---

## 📞 Support Plan

If users encounter issues:

1. **Check introspection** - Verify fields in schema
2. **Clear cache** - GraphQL client caches may be stale
3. **Check override** - User-defined fields take precedence
4. **Report issue** - With schema output and query

**Expected Issues**: Very low (backward compatible fix)

---

**Implementation Ready**: All phases documented and ready for senior architect review and implementation approval.
