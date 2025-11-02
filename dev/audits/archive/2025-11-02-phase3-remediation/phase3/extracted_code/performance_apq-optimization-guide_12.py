# Extracted from: docs/performance/apq-optimization-guide.md
# Block number: 12
from fastapi import Response


@app.post("/graphql")
async def graphql_endpoint(request: GraphQLRequest, response: Response):
    # Add cache headers for CDN
    if is_public_query(request):
        response.headers["Cache-Control"] = "public, max-age=300"

    # APQ handles query and response caching
    return await execute_graphql(request)
