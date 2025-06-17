# Release Notes - FraiseQL 0.1.0a6

## Critical Regression Fix

This release fixes a critical regression introduced in 0.1.0a5 where simple function-based mutations were broken.

### Fixed

- **Simple mutations broken**: Fixed "Mutation create_branch must define 'success' type" error
  - The mutation decorator now properly supports both simple and class-based patterns
  - Simple mutations can return types directly without success/error wrappers
  - Added auto-registration of mutations to avoid manual registration

### Improvements

- **Better error handling**: Fixed PostgresDsn to string conversion for database URLs
- **Type system improvements**: 
  - Fixed DateTime scalar reuse to prevent duplicate type errors
  - Added caching for GraphQL input types to prevent duplicates
  - Improved type conflict resolution in tests

- **Test infrastructure**:
  - All tests now use Podman containers (no Docker/mock dependencies)
  - Improved test isolation to prevent type conflicts between test modules
  - Fixed DataLoader integration tests

### Technical Details

The main issue was that the mutation decorator was always expecting class-based mutations with success/error types, breaking the simpler function pattern that was documented in quickstart examples.

**Simple mutation pattern (now works correctly):**
```python
@fraiseql.mutation
async def create_user(info, input: CreateUserInput) -> User:
    """Create a new user."""
    return User(
        id=1,
        name=input.name,
        email=input.email,
        created_at=datetime.now()
    )
```

**Class-based mutation pattern (still supported):**
```python
@fraiseql.mutation
class CreateUser:
    input: CreateUserInput
    success: CreateUserSuccess
    error: CreateUserError
```

### Test Status

- ✅ All 650 tests passing
- ✅ All mutation patterns working correctly
- ✅ DataLoader integration tests fixed
- ✅ Using Podman containers for database tests

### Migration Guide

No migration needed - this release restores functionality that was broken in 0.1.0a5.