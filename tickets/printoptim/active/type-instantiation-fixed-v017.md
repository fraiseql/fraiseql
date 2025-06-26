# Type Instantiation Fix in FraiseQL v0.1.0a17

## Issue Resolution

The type instantiation issue in v0.1.0a16 has been fixed. The problem was that the FraiseQL configuration wasn't being properly passed to the dependency injection system, causing the repository to default to production mode.

## Changes Made in v0.1.0a17

### 1. Configuration Storage
The FraiseQL config is now stored globally when the app is created:
```python
# In app.py
set_fraiseql_config(config)
```

### 2. Repository Mode Detection
The repository now correctly reads the mode from the config:
```python
# In dependencies.py
async def get_db() -> FraiseQLRepository:
    pool = get_db_pool()
    config = get_fraiseql_config()
    
    # Create repository with mode from config
    context = {}
    if config and hasattr(config, 'environment'):
        context["mode"] = "development" if config.environment == "development" else "production"
    
    return FraiseQLRepository(pool=pool, context=context)
```

### 3. Context Enhancement
The GraphQL context now includes the mode for debugging:
```python
# In build_graphql_context
config = get_fraiseql_config()
mode = "development" if config and config.environment == "development" else "production"

return {
    "db": db,
    "mode": mode,
    # ... other context fields
}
```

## Migration Guide

No changes are required to your code. Simply upgrade to v0.1.0a17:

```bash
pip install fraiseql==0.1.0a17
```

Your existing configuration will now work correctly:
```python
fraiseql_config = FraiseQLConfig(
    database_url=settings.database_url,
    environment="development",  # This will now be properly honored
    enable_introspection=True,
    enable_playground=True,
)
```

## Verification

To verify the fix is working, you can add this debug query:
```python
@fraiseql.query
async def debug_mode(info) -> dict[str, str]:
    """Check repository and context mode."""
    return {
        "repository_mode": info.context["db"].mode,
        "context_mode": info.context.get("mode", "not set"),
    }
```

Both should return "development" when configured for development mode.

## Summary

The issue was that the repository wasn't receiving the correct mode setting from the config, causing it to default to production mode. This has been fixed by properly passing the config through the dependency injection system.

With v0.1.0a17, type instantiation will work correctly in development mode as expected.