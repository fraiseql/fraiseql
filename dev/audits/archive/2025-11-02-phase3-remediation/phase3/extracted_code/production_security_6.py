# Extracted from: docs/production/security.md
# Block number: 6
from fraiseql.analysis.complexity import calculate_query_cost


@app.middleware("http")
async def query_cost_middleware(request: Request, call_next):
    if request.url.path != "/graphql":
        return await call_next(request)

    body = await request.json()
    query = body.get("query", "")

    # Calculate cost
    cost = calculate_query_cost(query, schema)

    # Reject expensive queries
    if cost > 1000:
        return Response(
            content=json.dumps(
                {
                    "errors": [
                        {
                            "message": f"Query cost {cost} exceeds limit 1000",
                            "extensions": {"code": "QUERY_TOO_EXPENSIVE"},
                        }
                    ]
                }
            ),
            status_code=400,
            media_type="application/json",
        )

    return await call_next(request)
