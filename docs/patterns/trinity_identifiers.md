# Trinity Identifiers Pattern

The Trinity Pattern for managing identifiers in FraiseQL applications.

## Overview

Trinity Identifiers provide a consistent way to handle entity identification across:
- Database (internal IDs)
- GraphQL API (public IDs)
- External Systems (external IDs)

## Pattern Structure

```python
from fraiseql import type, query, mutation, input, field
from uuid import UUID

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
```

## Benefits

### 1. Security
- Don't expose internal database IDs
- Use public IDs in URLs and APIs
- Prevent ID enumeration attacks

### 2. Flexibility
- Change internal IDs without affecting API
- Support multiple identifier schemes
- Integrate with external systems

### 3. Migration
- Maintain compatibility during migrations
- Support legacy identifiers
- Gradual identifier transitions

## Implementation

### Database Schema

```sql
CREATE TABLE products (
    -- Internal ID (UUID)
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Public ID (human-readable)
    public_id VARCHAR(255) UNIQUE NOT NULL,

    -- External ID (for integrations)
    external_id VARCHAR(255) UNIQUE,

    -- Other columns
    name VARCHAR(255) NOT NULL,
    price DECIMAL(10, 2) NOT NULL
);

-- Indexes
CREATE INDEX idx_products_public_id ON products(public_id);
CREATE INDEX idx_products_external_id ON products(external_id)
    WHERE external_id IS NOT NULL;
```

### GraphQL Queries

```python
from fraiseql import type, query, mutation, input, field

@query
def get_product_by_public_id(
    info: Info,
    public_id: str
) -> Product | None:
    """Get product by public ID (SKU)."""
    return info.context.repo.find_one(
        "products_view",
        public_id=public_id
    )

@query
def get_product_by_external_id(
    info: Info,
    external_id: str
) -> Product | None:
    """Get product by external system ID."""
    return info.context.repo.find_one(
        "products_view",
        external_id=external_id
    )
```

### Mutations

```python
from fraiseql import type, query, mutation, input, field

@mutation
async def create_product(
    info: Info,
    public_id: str,  # SKU or public identifier
    name: str,
    price: float,
    external_id: str | None = None
) -> Product:
    """Create product with Trinity identifiers."""
    product_data = {
        "public_id": public_id,
        "name": name,
        "price": price,
        "external_id": external_id
    }

    result = await info.context.repo.insert(
        "products",
        product_data
    )

    return info.context.repo.find_one(
        "products_view",
        id=result["id"]
    )
```

## Use Cases

### E-Commerce
- **Internal ID**: Database UUID
- **Public ID**: SKU (e.g., "WIDGET-001")
- **External ID**: Supplier product code

### User Management
- **Internal ID**: Database UUID
- **Public ID**: Username
- **External ID**: SSO provider ID

### Content Management
- **Internal ID**: Database UUID
- **Public ID**: Slug (URL-friendly)
- **External ID**: CMS import ID

## Best Practices

### 1. Always Use Public IDs in URLs

```
❌ Bad:  /products/550e8400-e29b-41d4-a716-446655440000
✅ Good: /products/WIDGET-001
```

### 2. Index All Identifier Types

```sql
CREATE INDEX idx_entity_public_id ON entity(public_id);
CREATE INDEX idx_entity_external_id ON entity(external_id)
    WHERE external_id IS NOT NULL;  -- Partial index
```

### 3. Validate Public ID Uniqueness

```python
from pydantic import BaseModel, validator

class ProductInput(BaseModel):
    public_id: str

    @validator('public_id')
    def validate_public_id(cls, v):
        # Ensure public ID format
        if not v.isalnum():
            raise ValueError("Public ID must be alphanumeric")
        return v.upper()
```

### 4. Handle ID Migrations

```python
from fraiseql import type, query, mutation, input, field

@query
def get_product(
    info: Info,
    id: str | None = None,
    public_id: str | None = None
) -> Product | None:
    """Support both ID types during migration."""
    if public_id:
        return info.context.repo.find_one(
            "products_view",
            public_id=public_id
        )
    elif id:
        # Legacy support
        return info.context.repo.find_one(
            "products_view",
            public_id=id  # Assume old ID was public_id
        )
    raise ValueError("Must provide either id or public_id")
```

## Related Patterns

- [CQRS](../../examples/enterprise_patterns/cqrs/)
- [Repository Pattern](../../examples/)
- [Hybrid Tables](../../examples/hybrid_tables/)

## Further Reading

- [Database Design](../architecture/)
- [Security Best Practices](../../SECURITY.md)
- [Blog Simple Example](../../examples/blog_simple/) - Complete trinity identifier implementation
- [Examples](../../examples/)
