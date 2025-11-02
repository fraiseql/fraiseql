# Extracted from: docs/advanced/authentication.md
# Block number: 18
import redis.asyncio as redis

from fraiseql.auth import RedisRevocationStore, TokenRevocationService

# Create Redis client
redis_client = redis.from_url("redis://localhost:6379/0")

# Create revocation store
revocation_store = RedisRevocationStore(
    redis_client=redis_client,
    ttl=86400,  # 24 hours
)

# Create revocation service
revocation_service = TokenRevocationService(
    store=revocation_store, config=RevocationConfig(enabled=True, check_revocation=True, ttl=86400)
)
