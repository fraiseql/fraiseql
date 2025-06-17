# Release Notes - FraiseQL 0.1.0a7

## New Features

This release adds significant improvements to developer experience and type system flexibility.

### N+1 Query Detection

Automatic detection of N+1 query patterns in development mode helps catch performance issues early:

- Configurable thresholds and time windows
- Warning mode (default) or strict mode (raises exceptions)
- Integrated into development router with minimal overhead
- Detailed suggestions for using DataLoaders
- Can be disabled for specific fields with `@fraiseql.field(track_n1=False)`

### Comprehensive Strawberry Migration Support

Complete migration toolkit for projects moving from Strawberry GraphQL:

- **Migration Guide**: Expanded documentation covering all aspects
- **Compatibility Layer**: Drop-in replacements for Strawberry decorators
- **Migration Checker**: Automated tool to analyze codebases for Strawberry patterns
- **Feature Parity**: Support for enums, mutations, DataLoaders, and more

### JSON/dict Type Support

Full support for dynamic JSON data in GraphQL schemas:

- `fraiseql.JSON` scalar type for flexible data structures
- Enhanced JSON literal parsing supporting GraphQL object notation
- Works in both output types and input types
- Backward compatible with string-based JSON

## Improvements

- Better error messages for type validation
- Enhanced field resolver tracking for performance monitoring
- Improved test isolation to prevent type conflicts

## Technical Details

### N+1 Detection Example:
```python
# Automatically warns about this pattern:
@fraiseql.type
class Post:
    @fraiseql.field
    async def author(self, info) -> User:
        # This triggers a separate query for each post!
        return await db.get_user(self.author_id)
```

### JSON Type Example:
```python
@fraiseql.type
class Config:
    settings: dict[str, Any]  # Works as JSON
    metadata: fraiseql.JSON   # Explicit JSON scalar
```

### Migration from Strawberry:
```python
# Use compatibility layer for easier migration
from fraiseql.strawberry_compat import strawberry

@strawberry.type  # Works with FraiseQL!
class User:
    name: str
```

## Migration Guide

No breaking changes. New features are opt-in:

- N+1 detection is automatic in development mode
- JSON support uses existing dict[str, Any] syntax
- Strawberry compatibility layer is optional

## Test Status

- ✅ All 650+ tests passing
- ✅ N+1 detection working correctly
- ✅ JSON type support fully tested
- ✅ Strawberry migration patterns validated