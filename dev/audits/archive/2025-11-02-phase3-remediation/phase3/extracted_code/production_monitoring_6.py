# Extracted from: docs/production/monitoring.md
# Block number: 6
# TTL-based expiration (automatic cleanup)
await cache.set("session:abc", session_data, ttl=900)  # 15 minutes

# Cleanup runs periodically (configurable)
# DELETE FROM cache_entries WHERE expires_at < NOW();
