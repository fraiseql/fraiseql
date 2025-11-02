# Extracted from: docs/production/security.md
# Block number: 5
from graphql import GraphQLError


def enforce_max_depth(document, max_depth: int = 10):
    """Prevent excessively nested queries."""
    from graphql import visit

    current_depth = 0

    def enter_field(node, key, parent, path, ancestors):
        nonlocal current_depth
        depth = len([a for a in ancestors if hasattr(a, "kind") and a.kind == "field"])

        if depth > max_depth:
            raise GraphQLError(
                f"Query depth {depth} exceeds maximum {max_depth}",
                extensions={"code": "MAX_DEPTH_EXCEEDED"},
            )

    visit(document, {"Field": {"enter": enter_field}})
