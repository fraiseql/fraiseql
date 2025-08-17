# Release Preparation for FraiseQL v0.3.4

## Current Status
- **Version**: 0.3.3 (to be bumped to 0.3.4)
- **Tests**: 2498 passing, 6 failing (needs fixing before release)
- **Uncommitted Changes**:
  - Bug fix in `src/fraiseql/core/json_passthrough.py`
  - Test improvements in `tests/security/test_json_passthrough_config_fix.py`
  - New edge case tests in `tests/core/json/test_json_passthrough_edge_cases.py`

## Changes Made in This Session
1. **Fixed JSONPassthrough Bug**:
   - Issue: `dict[str, Any]` fields were incorrectly wrapped in JSONPassthrough
   - Solution: Added type checking to return plain dicts when appropriate
   - Impact: Proper handling of metadata and dictionary-typed fields

2. **Improved Test Coverage**:
   - Added 14 comprehensive edge case tests
   - Fixed test configuration issues
   - Improved test naming to avoid pytest conflicts

## Pre-Release Checklist
- [ ] Fix 6 failing tests (see prompts 01-03)
- [ ] Update CHANGELOG.md with v0.3.4 entry
- [ ] Bump version in pyproject.toml to 0.3.4
- [ ] Commit all changes with clear message
- [ ] Run full test suite to confirm all tests pass
- [ ] Run linting and type checking
- [ ] Create git tag for v0.3.4
- [ ] Build and test the package locally

## CHANGELOG Entry for v0.3.4
```markdown
## [0.3.4] - 2025-01-17

### Fixed
- **JSONPassthrough**: Fixed incorrect wrapping of `dict[str, Any]` typed fields
  - Dictionary fields with generic typing are now correctly returned as plain dicts
  - Only typed objects (custom classes) are wrapped in JSONPassthrough
  - This fixes issues with metadata and other dictionary fields in GraphQL responses

### Added
- Comprehensive edge case test suite for JSON passthrough functionality
- Tests cover: Unicode handling, special characters, deep nesting, mixed types, and more

### Improved
- Test isolation and configuration in security tests
- Better error messages in JSONPassthrough for missing fields
```

## Commit Message
```
fix: JSONPassthrough correctly handles dict[str, Any] fields

- Fixed bug where dictionary fields with generic typing were incorrectly wrapped
- Added comprehensive edge case tests for JSON passthrough
- Improved test configuration to avoid pytest collection conflicts

This ensures proper handling of metadata and other dictionary-typed fields
in GraphQL responses, maintaining expected behavior for dict types while
still providing passthrough optimization for custom object types.
```

## Post-Release Steps
1. Push to GitHub
2. Create GitHub release with changelog
3. Publish to PyPI
4. Update documentation if needed
5. Notify users of the bug fix
