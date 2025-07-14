# Make Redis an Optional Dependency

## Summary
Redis should be an optional dependency in FraiseQL, not a required one. Currently, importing FraiseQL fails if Redis is not installed, even when Redis features (WebSocket subscriptions) are not being used.

## Current Behavior
When importing FraiseQL without Redis installed:
```python
from fraiseql.fastapi import create_fraiseql_app, FraiseQLConfig
```

Results in:
```
ModuleNotFoundError: No module named 'redis'
```

The error trace shows the import chain:
- `fraiseql/__init__.py` imports from `.subscriptions`
- `fraiseql/subscriptions/__init__.py` imports from `.websocket`
- WebSocket module requires Redis

## Expected Behavior
FraiseQL should work without Redis installed when:
- Not using WebSocket subscriptions
- Using `redis_client=None` in configuration
- Only using query/mutation operations

## Proposed Solution

### Option 1: Lazy Import
Make Redis imports lazy, only importing when WebSocket features are actually used:

```python
# In fraiseql/subscriptions/websocket.py
def get_redis_client():
    try:
        import redis
        return redis
    except ImportError:
        raise ImportError(
            "Redis is required for WebSocket subscriptions. "
            "Install it with: pip install fraiseql[redis]"
        )
```

### Option 2: Optional Import with Feature Detection
```python
# In fraiseql/__init__.py
try:
    from .subscriptions import subscription
    HAS_SUBSCRIPTIONS = True
except ImportError:
    HAS_SUBSCRIPTIONS = False
    subscription = None
```

### Option 3: Extras Dependency
Define Redis as an optional extra in `pyproject.toml`:

```toml
[project.optional-dependencies]
subscriptions = ["redis>=5.0.0"]
all = ["redis>=5.0.0"]
```

Then users can install with:
```bash
# Basic installation (no Redis)
pip install fraiseql

# With subscriptions support
pip install fraiseql[subscriptions]
```

## Impact
- **Breaking Change**: No, this would be backwards compatible
- **Benefits**:
  - Smaller dependency footprint for users not needing subscriptions
  - Faster installation
  - Easier to use in environments where Redis is not available
  - More flexible deployment options

## Use Case
In the PrintOptim backend project, we use FraiseQL for GraphQL but don't currently need WebSocket subscriptions. We had to add Redis as a dependency just to make imports work, even though we set `redis_client=None` in our configuration.

## Testing
- Ensure FraiseQL works without Redis for basic operations
- Ensure clear error messages when trying to use subscriptions without Redis
- Verify existing Redis-based features continue to work when Redis is installed
