# Extracted from: docs/advanced/bounded-contexts.md
# Block number: 7
# External system has different structure
@dataclass
class ExternalProduct:
    """External catalog system product."""

    sku: str
    title: str
    unitPrice: float
    stockLevel: int


# Your domain model
@dataclass
class Product:
    """Internal product model."""

    id: UUID
    name: str
    price: Money
    quantity_available: int


# Anti-Corruption Layer
class ProductACL:
    """Translates between external and internal product models."""

    @staticmethod
    def to_domain(external: ExternalProduct) -> Product:
        """Convert external product to domain product."""
        return Product(
            id=external.sku,
            name=external.title,
            price=Money(Decimal(str(external.unitPrice)), "USD"),
            quantity_available=external.stockLevel,
        )

    @staticmethod
    def to_external(product: Product) -> ExternalProduct:
        """Convert domain product to external format."""
        return ExternalProduct(
            sku=product.id,
            title=product.name,
            unitPrice=float(product.price.amount),
            stockLevel=product.quantity_available,
        )


# Usage
from fraiseql import query


@query
async def get_product_from_external(info, sku: str) -> Product:
    """Fetch product from external system via ACL."""
    external_product = await fetch_from_external_catalog(sku)
    return ProductACL.to_domain(external_product)
