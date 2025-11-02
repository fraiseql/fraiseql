# Extracted from: docs/core/concepts-glossary.md
# Block number: 13
from fraiseql import FraiseQLConfig

config = FraiseQLConfig(
    apq_storage_backend="memory",  # Default - LRU cache
    apq_cache_size=1000,  # Max cached queries
)
