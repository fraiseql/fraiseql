# Extracted from: docs/patterns/trinity_identifiers.md
# Block number: 1
from uuid import UUID

from fraiseql import type


@type
class Product:
    """Product with Trinity identifiers."""

    # Internal database ID
    id: UUID

    # Public-facing ID (e.g., SKU)
    public_id: str

    # External system ID (optional)
    external_id: str | None = None

    # Other fields
    name: str
    price: float
