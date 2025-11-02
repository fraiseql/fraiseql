# Extracted from: docs/advanced/llm-integration.md
# Block number: 12
from decimal import Decimal

from fraiseql import type


# ✅ Good: Write docstrings once with Fields section
@type(sql_source="v_product")
class Product:
    """Product available for purchase.

    Fields:
        sku: Stock keeping unit (format: ABC-12345)
        name: Product name
        price: Price in USD cents (e.g., 2999 = $29.99)
        in_stock: Whether product is currently available
    """

    sku: str
    name: str
    price: Decimal
    in_stock: bool


# ❌ Bad: Don't manually maintain separate schema docs
# LLMs automatically read descriptions from introspection
