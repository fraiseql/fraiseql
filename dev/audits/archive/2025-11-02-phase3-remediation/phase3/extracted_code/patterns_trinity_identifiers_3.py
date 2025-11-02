# Extracted from: docs/patterns/trinity_identifiers.md
# Block number: 3
from fraiseql import mutation


@mutation
async def create_product(
    info: Info,
    public_id: str,  # SKU or public identifier
    name: str,
    price: float,
    external_id: str | None = None,
) -> Product:
    """Create product with Trinity identifiers."""
    product_data = {
        "public_id": public_id,
        "name": name,
        "price": price,
        "external_id": external_id,
    }

    result = await info.context.repo.insert("products", product_data)

    return info.context.repo.find_one("products_view", id=result["id"])
