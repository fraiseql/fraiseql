# Extracted from: docs/performance/apq-optimization-guide.md
# Block number: 9
from fraiseql.storage.apq_store import get_storage_stats

stats = get_storage_stats()
print(f"Stored queries: {stats['stored_queries']}")
print(f"Total size: {stats['total_size_bytes'] / 1024:.1f} KB")
