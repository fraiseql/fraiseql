# Extracted from: docs/advanced/llm-integration.md
# Block number: 8
class SafeLLMExecutor:
    """Execute only safe, read-only queries from LLM."""

    ALLOWED_ROOT_FIELDS = ["users", "user", "orders", "order", "products", "product"]

    @classmethod
    def validate_safe_query(cls, document) -> None:
        """Ensure query only uses allowed fields."""
        from graphql import visit

        def enter_field(node, key, parent, path, ancestors):
            # Check root fields
            if len(ancestors) == 3:  # Root query field
                if node.name.value not in cls.ALLOWED_ROOT_FIELDS:
                    raise ValueError(f"Field '{node.name.value}' not allowed for LLM queries")

        visit(document, {"Field": {"enter": enter_field}})

    async def execute_llm_query(self, query_text: str, context: dict) -> dict:
        """Execute LLM-generated query with safety checks."""
        document = parse(query_text)

        # Check for mutations
        has_mutation = any(
            op.operation == "mutation" for op in document.definitions if hasattr(op, "operation")
        )
        if has_mutation:
            raise ValueError("Mutations not allowed for LLM queries")

        # Validate safe operations
        self.validate_safe_query(document)

        # Check depth
        enforce_max_depth(document, max_depth=3)

        # Execute
        from graphql import graphql

        result = await graphql(self.schema, query_text, context_value=context)

        return result.data
