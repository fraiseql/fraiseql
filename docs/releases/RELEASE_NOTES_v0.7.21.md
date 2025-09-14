# FraiseQL v0.7.21 Release Notes

**Release Date**: September 14, 2025
**Release Type**: Bug Fix
**Priority**: High

## 🐛 Bug Fix: Mutation Name Collision Resolution

### Problem Addressed
FraiseQL users experienced parameter validation errors when using mutations with similar names. For example, mutations like `CreateItem` and `CreateItemComponent` would interfere with each other, causing `createItemComponent` to incorrectly require the `item_serial_number` field from `CreateItemInput` instead of its own `CreateItemComponentInput` fields.

### Root Cause
The GraphQL resolver naming strategy used `to_snake_case(class_name)` which could create naming collisions when similar class names produced identical snake_case resolver names. This caused one mutation's metadata to overwrite another's in the GraphQL schema registry.

### Solution Implemented
- **Enhanced Resolver Naming**: Now uses PostgreSQL function names for resolver naming to ensure uniqueness (e.g., `create_item` vs `create_item_component`)
- **Memory Isolation**: Creates fresh annotation dictionaries for each resolver to prevent shared reference issues
- **Comprehensive Testing**: Added extensive test coverage to prevent regressions

### Technical Details

#### Files Modified
- `src/fraiseql/mutations/mutation_decorator.py` - Core resolver naming logic enhancement

#### New Test Coverage
- `tests/integration/graphql/mutations/test_similar_mutation_names_collision_fix.py` - 8 comprehensive test scenarios

#### Before/After Behavior
- **❌ Before**: Similar mutations could share validation logic causing incorrect parameter requirements
- **✅ After**: Each mutation validates independently with correct input type requirements

### Impact Assessment
- **Severity**: High - Blocks API functionality for projects with similar mutation names
- **Scope**: Affects GraphQL mutations with similar naming patterns
- **Backward Compatibility**: ✅ Fully maintained - no breaking changes
- **Performance**: No impact on performance

### Quality Assurance
- ✅ All 2,979+ existing tests continue to pass
- ✅ 8 new collision-prevention tests added
- ✅ Full CI/CD pipeline validation completed
- ✅ Code quality gates passed (lint, security, type checking)

### Upgrade Instructions

#### For Users Experiencing Version Display Issues
If `pip show fraiseql` shows an older version (like 0.7.10b1), clean install:

```bash
# Uninstall old version
pip uninstall fraiseql

# Install latest version
pip install fraiseql==0.7.21

# Verify installation
python -c "import fraiseql; print(f'Version: {fraiseql.__version__}')"
```

#### For Existing Projects
This is a transparent bug fix - no code changes required. Simply upgrade:

```bash
pip install --upgrade fraiseql
```

### Examples of Fixed Scenarios

#### Scenario 1: Item Management API
```python
@fraiseql.mutation(function="create_item")
class CreateItem:
    input: CreateItemInput  # Requires: item_serial_number
    success: CreateItemSuccess
    failure: CreateItemError

@fraiseql.mutation(function="create_item_component")
class CreateItemComponent:
    input: CreateItemComponentInput  # Requires: item_id, component_type
    success: CreateItemComponentSuccess
    failure: CreateItemComponentError
```

**Before v0.7.21**: `createItemComponent` would incorrectly require `item_serial_number`
**After v0.7.21**: Each mutation validates with its own correct parameters

#### Scenario 2: User Management API
```python
@fraiseql.mutation(function="create_user")
class CreateUser:
    input: CreateUserInput  # Requires: email, password

@fraiseql.mutation(function="create_user_profile")
class CreateUserProfile:
    input: CreateUserProfileInput  # Requires: user_id, bio
```

**Before v0.7.21**: Potential parameter validation confusion
**After v0.7.21**: Independent validation for each mutation

### Migration Notes
- **No action required** - This is a transparent bug fix
- **Existing GraphQL schemas** continue to work unchanged
- **PostgreSQL functions** remain unaffected
- **API contracts** are preserved

### Related Issues
- Fixes bug reported in user feedback regarding parameter validation confusion
- Resolves GraphQL mutation registry conflicts
- Improves developer experience for similar mutation names

### Next Steps
This release focuses solely on the mutation collision fix. Future releases will continue to enhance FraiseQL's GraphQL mutation system with additional improvements based on user feedback.

---

**Installation**: `pip install fraiseql==0.7.21`
**Documentation**: [FraiseQL Documentation](https://github.com/fraiseql/fraiseql)
**Issues**: [Report Issues](https://github.com/fraiseql/fraiseql/issues)
**Changelog**: [Full Changelog](https://github.com/fraiseql/fraiseql/blob/main/CHANGELOG.md)
