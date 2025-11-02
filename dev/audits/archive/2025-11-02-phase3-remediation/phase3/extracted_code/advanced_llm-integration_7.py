# Extracted from: docs/advanced/llm-integration.md
# Block number: 7
def enforce_max_depth(document, max_depth: int = 3) -> None:
    """Enforce maximum query depth."""
    from graphql import visit

    current_depth = 0

    def enter_field(node, key, parent, path, ancestors):
        nonlocal current_depth
        current_depth = len([a for a in ancestors if a.get("kind") == "Field"])
        if current_depth > max_depth:
            raise ValueError(f"Query depth {current_depth} exceeds maximum {max_depth}")

    visit(document, {"Field": {"enter": enter_field}})
