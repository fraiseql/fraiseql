# Extracted from: docs/patterns/trinity_identifiers.md
# Block number: 5
from fraiseql import query


@query
def get_product(info: Info, id: str | None = None, public_id: str | None = None) -> Product | None:
    """Support both ID types during migration."""
    if public_id:
        return info.context.repo.find_one("products_view", public_id=public_id)
    if id:
        # Legacy support
        return info.context.repo.find_one(
            "products_view",
            public_id=id,  # Assume old ID was public_id
        )
    raise ValueError("Must provide either id or public_id")
