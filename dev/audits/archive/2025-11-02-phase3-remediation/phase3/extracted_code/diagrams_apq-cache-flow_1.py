# Extracted from: docs/diagrams/apq-cache-flow.md
# Block number: 1
class APQMiddleware:
    def __init__(self, cache_store):
        self.cache = cache_store

    async def __call__(self, request, call_next):
        body = await request.json()

        # Check for APQ request
        extensions = body.get("extensions", {})
        persisted_query = extensions.get("persistedQuery")

        if persisted_query:
            query_hash = persisted_query["sha256Hash"]

            # Try to get cached query
            cached_query = await self.cache.get(f"apq:{query_hash}")

            if cached_query:
                # Use cached query
                body["query"] = cached_query
            else:
                # Query not cached, expect full query
                if "query" not in body:
                    return JSONResponse(
                        {
                            "errors": [
                                {
                                    "message": "PersistedQueryNotFound",
                                    "extensions": {"code": "PERSISTED_QUERY_NOT_FOUND"},
                                }
                            ]
                        },
                        status_code=200,
                    )

                # Cache the query for future use
                await self.cache.set(f"apq:{query_hash}", body["query"])

        return await call_next(request)
