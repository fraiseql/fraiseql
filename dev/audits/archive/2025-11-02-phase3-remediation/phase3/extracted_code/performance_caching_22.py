# Extracted from: docs/performance/caching.md
# Block number: 22
# UUID
filters = {"user_id": UUID("...")}
# → user_id:00000000-0000-0000-0000-000000000000

# Date/DateTime
filters = {"created_after": datetime(2025, 1, 1)}
# → created_after:2025-01-01T00:00:00

# List (sorted)
filters = {"status__in": ["active", "pending"]}
# → status__in:active,pending

# Complex list (hashed for brevity)
filters = {"ids": [UUID(...), UUID(...)]}
# → ids:a1b2c3d4  (MD5 hash prefix)

# Boolean
filters = {"is_active": True}
# → is_active:true

# None
filters = {"deleted_at": None}
# → deleted_at:null
