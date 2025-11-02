# Extracted from: docs/patterns/trinity_identifiers.md
# Block number: 2
from fraiseql import query


@query
def get_product_by_public_id(info: Info, public_id: str) -> Product | None:
    """Get product by public ID (SKU)."""
    return info.context.repo.find_one("products_view", public_id=public_id)


@query
def get_product_by_external_id(info: Info, external_id: str) -> Product | None:
    """Get product by external system ID."""
    return info.context.repo.find_one("products_view", external_id=external_id)
