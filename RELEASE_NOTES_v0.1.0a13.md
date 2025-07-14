# FraiseQL v0.1.0a13 Release Notes

## Overview

This release introduces a powerful dual-mode repository instantiation feature that allows FraiseQL to adapt its behavior based on the environment, providing the best of both worlds: excellent developer experience in development and maximum performance in production.

## Key Features

### Dual-Mode Repository Instantiation

The `FraiseQLRepository` now supports two operational modes:

1. **Development Mode**: Full recursive instantiation of typed objects
   - Better developer experience with type safety
   - Automatic nested object instantiation
   - IDE autocomplete support
   - Easier debugging

2. **Production Mode**: Raw dictionary data (default)
   - Zero overhead - data flows directly from database to client
   - Maximum performance
   - Minimal memory usage

### How It Works

```python
# Development mode - returns typed objects
repo = FraiseQLRepository(pool, {"mode": "development"})
product = await repo.find_one("products", id=product_id)
print(product.name)  # IDE autocomplete works!
print(isinstance(product, Product))  # True

# Production mode - returns raw dicts
repo = FraiseQLRepository(pool, {"mode": "production"})
product = await repo.find_one("products", id=product_id)
print(product["name"])  # Dictionary access
print(isinstance(product, dict))  # True
```

### Mode Configuration

Mode detection follows this priority:
1. Context override (per-request)
2. Environment variable `FRAISEQL_ENV`
3. Default to production

```bash
# Enable development mode globally
export FRAISEQL_ENV=development

# Or override per request
context = {"mode": "development"}
repo = FraiseQLRepository(pool, context)
```

### New Repository Methods

- `find(view_name: str, **kwargs)` - Find multiple records
- `find_one(view_name: str, **kwargs)` - Find single record

Both methods return mode-appropriate data types.

### Additional Features

- Automatic UUID and datetime type conversion
- CamelCase to snake_case field name conversion
- Circular reference detection and caching
- Maximum recursion depth protection (10 levels)
- Zero breaking changes - existing code continues to work

## Installation

```bash
pip install fraiseql==0.1.0a13
```

## Testing

This release includes comprehensive test coverage with 11 unit tests covering all aspects of the dual-mode functionality.

## Migration

No migration needed - this is a backward-compatible enhancement. Existing code will continue to work in production mode by default.

## Future Enhancements

- Partial instantiation: Configure which types to instantiate
- Lazy instantiation: Instantiate objects on first access
- Performance monitoring: Track instantiation overhead
- Caching: Reuse instantiated objects across requests

## Credits

This feature was developed using Test-Driven Development (TDD) with comprehensive test coverage from the start.
