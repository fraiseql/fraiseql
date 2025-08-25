# FraiseQL v0.5.0-beta1 Release Notes

**🎯 Beta Release for Clean Mutation Error Management System**

This beta release introduces the new **Clean Mutation Error Management System** that solves critical frontend compatibility issues with mutation error handling.

---

## 🆕 New Features

### 🧹 Clean Mutation Error Management System
A completely rebuilt error management system that provides **predictable, frontend-compatible error responses**.

#### Key Components Added:
- **`MutationResultProcessor`** - Immutable, predictable result processing
- **`clean_mutation` decorator** - Alternative to existing mutation decorators
- **`ErrorDetail` & `ProcessedResult`** - Immutable data structures for errors
- **`result_processor.py`** - Core processing logic
- **`clean_decorator.py`** - Clean mutation decorator implementation

#### Problems Solved:
- ❌ **Inconsistent errors arrays**: Sometimes `null`, sometimes empty, sometimes populated
- ❌ **Manual workarounds required**: No more `__post_init__()` hacks needed
- ❌ **Frontend compatibility issues**: Guaranteed structure for frontend consumption
- ❌ **Complex debugging**: Simple, predictable error flow

#### New Guarantees:
- ✅ **Always populated errors arrays**: Error types ALWAYS have populated errors array (never `null`)
- ✅ **Immutable processing**: No in-place object modifications during processing
- ✅ **Predictable structure**: Same input always produces same output
- ✅ **Frontend-first design**: Built specifically for frontend consumption
- ✅ **Status code mapping**: `noop:`/`blocked:` → 422, `failed:` → 500

---

## 🛠️ Technical Implementation

### New Modules:
```
src/fraiseql/mutations/
├── result_processor.py     # Core error processing logic
└── clean_decorator.py      # Clean mutation decorator
```

### Usage Example:
```python
from fraiseql.mutations.clean_decorator import clean_mutation

@clean_mutation(
    function="create_machine",
    context_params={"tenant_id": "input_pk_organization", "user": "input_created_by"}
)
class CreateMachine:
    class Input:
        name: str
        serial_number: str

    class Success:
        machine: Machine | None = None
        message: str = "Success"

    class Error:
        message: str = "Failed"
        error_code: str = "CREATE_FAILED"
        # NO manual errors field needed!
        # NO __post_init__ hack required!
```

### Error Response Structure:
```json
{
  "__typename": "CreateMachineError",
  "message": "Contract not found or access denied",
  "errorCode": "INVALID_CONTRACT_ID",
  "errors": [
    {
      "code": 422,
      "identifier": "invalid_contract_id",
      "message": "Contract not found or access denied",
      "details": {}
    }
  ]
}
```

---

## 🧪 Testing Coverage

### Comprehensive Test Suite Added:
- **29 comprehensive tests** covering all error management scenarios
- **TestErrorResultProcessor** - Core processing logic tests
- **TestGraphQLErrorIntegration** - GraphQL integration tests
- **TestCleanMutationDecorator** - Decorator functionality tests

### Test Locations:
```
tests/mutation_error_management/
├── test_error_result_processor.py     # 18 core tests
└── test_graphql_integration.py        # 11 integration tests
```

### Validation:
- ✅ All 29 new tests pass
- ✅ All existing FraiseQL tests still pass (no regressions)
- ✅ Backward compatibility maintained

---

## 🔄 Backward Compatibility

**100% Backward Compatible**: All existing mutations continue to work unchanged.

- ✅ Existing `@fraiseql.mutation` decorators work as before
- ✅ Existing `FraiseQLMutation` base classes work as before
- ✅ No breaking changes to existing APIs
- ✅ New system is **opt-in** - use when ready

---

## 🎯 Beta Testing Focus Areas

This beta is specifically designed for testing the new error management system:

### 1. **Error Array Population**
Test that error responses **always** have populated `errors` arrays:
```python
# Should NEVER be null
assert response["errors"] is not None
assert isinstance(response["errors"], list)
assert len(response["errors"]) > 0  # For error responses
```

### 2. **Frontend Compatibility**
Verify error structure matches frontend expectations:
- `errors[0].code` - HTTP-style error code (422, 500, etc.)
- `errors[0].identifier` - Machine-readable identifier
- `errors[0].message` - Human-readable message
- `errors[0].details` - Additional error context

### 3. **Consistency Testing**
Same error conditions should produce identical error structures across multiple test runs.

### 4. **Migration Path**
Test migrating existing mutations to use the clean system without breaking functionality.

---

## 📦 Installation

### For Testing in Other Repositories:

```bash
# Install beta version
pip install fraiseql==0.5.0b1

# Or with uv
uv add fraiseql==0.5.0b1
```

### For Development/Local Testing:

```bash
# Install from local source (most up-to-date)
cd /path/to/fraiseql
pip install -e .

# Or
uv add --editable /path/to/fraiseql
```

---

## 🚀 Migration Guide (For Beta Testers)

### Step 1: Install Beta Version
```bash
uv add fraiseql==0.5.0b1
```

### Step 2: Create Test Mutation (Side-by-Side)
```python
# Keep existing mutation working
from your_app.base_mutation import YourBaseMutation

class CreateItem(YourBaseMutation, function="create_item"):
    input: CreateItemInput
    success: CreateItemSuccess
    failure: CreateItemError  # May have manual __post_init__ hacks

# Add new clean version for testing
from fraiseql.mutations.clean_decorator import clean_mutation

@clean_mutation(function="create_item")
class CreateItemClean:
    class Input:
        name: str
        # Same fields as CreateItemInput

    class Success:
        item: Item | None = None
        message: str = "Success"

    class Error:
        message: str = "Failed"
        error_code: str = "CREATE_FAILED"
        # NO manual errors field!
        # NO __post_init__ hack!
```

### Step 3: Register Both Mutations
```python
# In your GraphQL schema registration:
MUTATIONS = [
    CreateItem,       # Existing (for comparison)
    CreateItemClean,  # New clean version (for testing)
]
```

### Step 4: Test Both Versions
```python
# Test old vs new side-by-side
old_result = await client.execute("mutation { createItem(...) }")
new_result = await client.execute("mutation { createItemClean(...) }")

# Compare error structure consistency
assert new_result["errors"] is not None  # Never null!
assert len(new_result["errors"]) > 0     # Always populated for errors!
```

---

## ⚠️ Beta Limitations

1. **New `clean_mutation` decorator**: May need additional GraphQL integration work
2. **Limited real-world testing**: Needs validation in production-like environments
3. **Documentation**: Complete docs will come with stable release
4. **Migration tooling**: Automated migration tools not yet available

---

## 🐛 Feedback & Bug Reports

Please report any issues or feedback:

1. **GitHub Issues**: https://github.com/fraiseql/fraiseql/issues
2. **Focus Areas**:
   - Error array population consistency
   - Frontend integration compatibility
   - Performance impact
   - Migration experience
   - Unexpected behavior vs existing system

---

## 🎯 Next Steps

After beta testing feedback:

1. **v0.5.0 Stable Release**: Address any beta feedback
2. **Migration Tooling**: Automated tools for converting existing mutations
3. **Complete Documentation**: Full docs for the clean error management system
4. **Integration Examples**: More real-world usage examples

---

**This beta release enables testing the clean error management system in real applications while maintaining full backward compatibility.**

---

## 📊 Test Results Summary

- ✅ **29/29** new error management tests pass
- ✅ **18/18** existing parser tests pass (no regressions)
- ✅ **6/6** existing decorator tests pass (backward compatibility)
- ✅ **All core FraiseQL functionality** remains unchanged

**Ready for beta testing in dependent projects!** 🚀
