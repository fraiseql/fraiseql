# Extracted from: docs/production/security.md
# Block number: 3
from graphql import parse, validate


def sanitize_graphql_query(query: str) -> str:
    """Validate GraphQL query syntax."""
    try:
        # Parse to AST (validates syntax)
        document = parse(query)

        # Validate against schema
        errors = validate(schema, document)
        if errors:
            raise ValueError(f"Invalid query: {errors}")

        return query

    except Exception as e:
        raise ValueError(f"Query validation failed: {e}")
