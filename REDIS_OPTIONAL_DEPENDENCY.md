# Redis as Optional Dependency

**Date:** 2025-07-12
**Version:** 0.1.0b13+
**Feature:** Redis is now an optional dependency

## Overview

Redis is now an optional dependency in FraiseQL. You can use FraiseQL for GraphQL APIs without installing Redis, and only add Redis when you need specific features like WebSocket subscriptions, distributed caching, or distributed rate limiting.

## What Changed

### Before (v0.1.0b12 and earlier)
- Redis was a required dependency
- Importing FraiseQL would fail if Redis wasn't installed
- All users had to install Redis even if they didn't use Redis features

### After (v0.1.0b13+)
- Redis is optional - install only when needed
- Basic FraiseQL functionality works without Redis
- Clear error messages when Redis features are used without Redis installed

## Installation Options

### Basic Installation (No Redis)
```bash
# For GraphQL APIs without Redis features
pip install fraiseql
```

### With Redis Support
```bash
# For WebSocket subscriptions, distributed caching, rate limiting
pip install fraiseql[redis]

# Or install Redis separately
pip install fraiseql redis>=5.0.0
```

### Full Installation
```bash
# All optional dependencies (Redis, tracing, auth0)
pip install fraiseql[all]
```

## Features That Don't Require Redis

These work with basic installation:

- **GraphQL APIs**: Queries, mutations, schema generation
- **Database operations**: PostgreSQL with JSONB
- **Authentication**: JWT validation, Auth0 integration
- **In-memory caching**: For development and small deployments
- **In-memory rate limiting**: For single-server deployments
- **Token revocation**: In-memory store for development

## Features That Require Redis

Install `fraiseql[redis]` for these features:

- **WebSocket subscriptions**: Real-time GraphQL subscriptions
- **Distributed caching**: `RedisCache` for multi-server deployments
- **Distributed rate limiting**: `RedisRateLimiter` for load balancers
- **Distributed token revocation**: `RedisRevocationStore` for auth

## Migration Guide

### If You Don't Use Redis Features
```bash
# Old installation
pip install fraiseql redis

# New installation (smaller, faster)
pip install fraiseql
```

No code changes needed - everything works the same.

### If You Use Redis Features
```bash
# Recommended installation
pip install fraiseql[redis]

# Or explicit Redis version
pip install fraiseql redis>=5.0.0
```

Your code continues to work without changes.

## Error Messages

When you try to use Redis features without Redis installed:

```python
from fraiseql.caching import RedisCache

# This will raise a helpful error:
cache = RedisCache(redis_client)
# ImportError: Redis is required for RedisCache.
# Install it with: pip install fraiseql[redis]
```

## Configuration Examples

### Basic GraphQL API (No Redis)
```python
from fraiseql.fastapi import create_fraiseql_app

app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[User, Product],
    queries=[users, products],
)
```

### With Redis Features
```python
import redis.asyncio as redis
from fraiseql.fastapi import create_fraiseql_app
from fraiseql.caching import RedisCache

redis_client = redis.Redis.from_url("redis://localhost")

app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[User, Product],
    queries=[users, products],
    cache=RedisCache(redis_client),
    redis_client=redis_client,  # For subscriptions
)
```

## Development Recommendations

### Local Development
```python
# Use in-memory stores for development
from fraiseql.auth import InMemoryRevocationStore
from fraiseql.middleware import InMemoryRateLimiter

# No Redis needed for development
```

### Production Deployment
```bash
# Install with Redis for production features
pip install fraiseql[redis]
```

```python
# Use Redis stores for production
from fraiseql.auth import RedisRevocationStore
from fraiseql.caching import RedisCache
```

## Benefits

1. **Smaller installations**: No unnecessary dependencies
2. **Faster CI/CD**: Quicker builds when Redis isn't needed
3. **Simpler deployment**: Redis-free deployments for simple APIs
4. **Progressive enhancement**: Add Redis features when you need them
5. **Clear separation**: Obvious which features require Redis

## Backward Compatibility

- Existing code continues to work without changes
- All Redis-dependent classes are still available
- Error messages help users install Redis when needed
- No breaking changes to APIs

## Testing

Run tests to verify Redis is truly optional:

```bash
# Test without Redis installed
pip uninstall redis
python -c "import fraiseql; print('FraiseQL works without Redis!')"

# Test Redis features with Redis installed
pip install redis>=5.0.0
pytest tests/test_redis_features.py
```
