# Extracted from: docs/advanced/llm-integration.md
# Block number: 10
async def execute_with_fallback(query_text: str, context: dict) -> dict:
    """Execute with fallback to simpler query on failure."""
    try:
        # Try full query
        result = await graphql(schema, query_text, context_value=context)
        if not result.errors:
            return result.data

        # Try with fewer fields
        simplified_query = simplify_query(query_text)
        result = await graphql(schema, simplified_query, context_value=context)
        if not result.errors:
            return {"data": result.data, "warning": "Used simplified query due to errors"}

    except Exception as e:
        # Fall back to error message
        return {"error": str(e), "suggestion": "Try a simpler query or rephrase your request"}


def simplify_query(query_text: str) -> str:
    """Remove nested fields to simplify query."""
    # Parse and remove fields beyond depth 2
    # This is a simplified implementation
    document = parse(query_text)
    # ... implementation to remove deep fields
    return print_ast(document)
