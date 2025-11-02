# Extracted from: docs/advanced/llm-integration.md
# Block number: 4
from typing import Any

from graphql import GraphQLError, parse, validate


class LLMQueryGenerator:
    """Generate and execute GraphQL queries from natural language."""

    def __init__(self, schema, llm_client, max_complexity: int = 50):
        self.schema = schema
        self.llm_client = llm_client
        self.max_complexity = max_complexity

    async def query_from_natural_language(self, user_request: str, context: dict) -> dict[str, Any]:
        """Convert natural language to GraphQL and execute."""
        # 1. Generate query
        query_text = await generate_query_with_llm(user_request, self.llm_client)

        # 2. Validate syntax
        try:
            document = parse(query_text)
        except GraphQLError as e:
            raise ValueError(f"Invalid GraphQL syntax: {e}")

        # 3. Validate against schema
        errors = validate(self.schema, document)
        if errors:
            raise ValueError(f"Schema validation failed: {errors}")

        # 4. Check complexity
        complexity = calculate_query_complexity(document, self.schema)
        if complexity > self.max_complexity:
            raise ValueError(f"Query too complex: {complexity} > {self.max_complexity}")

        # 5. Execute
        from graphql import graphql

        result = await graphql(self.schema, query_text, context_value=context)

        if result.errors:
            raise ValueError(f"Execution errors: {result.errors}")

        return result.data


def calculate_query_complexity(document, schema) -> int:
    """Calculate query complexity score."""
    # Simple implementation: count fields
    from graphql import visit

    complexity = 0

    def enter_field(node, key, parent, path, ancestors):
        nonlocal complexity
        complexity += 1

    visit(document, {"Field": {"enter": enter_field}})

    return complexity
