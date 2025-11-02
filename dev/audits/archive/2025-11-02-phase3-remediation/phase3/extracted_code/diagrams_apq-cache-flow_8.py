# Extracted from: docs/diagrams/apq-cache-flow.md
# Block number: 8
async def warmup_apq_cache():
    """Pre-populate cache with common queries"""
    common_queries = [
        "query GetUser($id: ID!) { user(id: $id) { name email } }",
        "query GetPosts { posts { title author { name } } }",
        # ... more common queries
    ]

    for query in common_queries:
        query_hash = sha256(query)
        await apq_cache.set(query_hash, query)
