# Extracted from: docs/core/concepts-glossary.md
# Block number: 14
config = FraiseQLConfig(
    apq_storage_backend="postgresql",
    apq_storage_schema="apq_cache",  # Schema for cache table
    apq_cache_ttl=3600,  # TTL in seconds (optional)
)

# Creates table:
# CREATE TABLE apq_cache.persisted_queries (
#     query_hash TEXT PRIMARY KEY,
#     query_text TEXT NOT NULL,
#     created_at TIMESTAMPTZ DEFAULT NOW(),
#     last_used TIMESTAMPTZ DEFAULT NOW()
# );
