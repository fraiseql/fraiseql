# Extracted from: docs/performance/apq-optimization-guide.md
# Block number: 5
from fraiseql.storage.apq_store import compute_query_hash, store_persisted_query

# Get top queries from analytics
top_queries = [
    "query GetUsers { users { id name email } }",
    "query GetPosts { posts { id title content } }",
    # ... more queries
]

# Pre-warm the cache
for query in top_queries:
    hash_value = compute_query_hash(query)
    store_persisted_query(hash_value, query)
