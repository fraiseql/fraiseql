# Extracted from: docs/performance/caching.md
# Block number: 21
# These produce the same key
key1 = key_builder.build_key("users", tenant_id="t1", filters={"status": "active", "role": "admin"})

key2 = key_builder.build_key(
    "users",
    tenant_id="t1",
    filters={"role": "admin", "status": "active"},  # Different order
)

assert key1 == key2  # True - filters sorted alphabetically
