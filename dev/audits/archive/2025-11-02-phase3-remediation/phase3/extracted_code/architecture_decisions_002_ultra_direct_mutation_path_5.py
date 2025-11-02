# Extracted from: docs/architecture/decisions/002_ultra_direct_mutation_path.md
# Block number: 5
# FastAPI (EXISTING CODE)

# In your GraphQL endpoint
@app.post("/graphql")
async def graphql_endpoint(request: Request):
    result = await execute_graphql(schema, query, variables, context)

    # If result is RawJSONResult, return directly
    if isinstance(result, RawJSONResult):
        return Response(content=result.json_string, media_type="application/json")

    # Otherwise, serialize normally
    return result
