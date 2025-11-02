# Extracted from: docs/diagrams/apq-cache-flow.md
# Block number: 4
class DatabaseAPQCache:
    async def get(self, query_hash):
        async with db.connection() as conn:
            result = await conn.fetchrow(
                "SELECT query_text FROM apq_cache WHERE query_hash = $1", query_hash
            )
            if result:
                # Update usage statistics
                await conn.execute(
                    "UPDATE apq_cache SET last_used = now(), use_count = use_count + 1 WHERE query_hash = $1",
                    query_hash,
                )
            return result["query_text"] if result else None

    async def set(self, query_hash, query_text):
        async with db.connection() as conn:
            await conn.execute(
                "INSERT INTO apq_cache (query_hash, query_text) VALUES ($1, $2) ON CONFLICT (query_hash) DO NOTHING",
                query_hash,
                query_text,
            )
