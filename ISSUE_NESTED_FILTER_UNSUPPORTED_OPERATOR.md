# Bug Report: "Unsupported operator: id" in Nested Where Filters

## Summary

When using nested filtering with `where: { relatedEntity: { id: { eq: "uuid" } } }`, FraiseQL throws a warning `Unsupported operator: id` and fails to apply the filter correctly, resulting in no results being returned even when matching records exist.

## Environment

- **FraiseQL Version**: [Current version]
- **Python Version**: 3.11+
- **Database**: PostgreSQL

## Problem Description

The warning message appears:
```
WARNING  fraiseql.db:db.py:2580 Operator strategy failed for relatedEntity id {'eq': 'uuid-value'}: Unsupported operator: id
```

This causes the query to return empty results even when:
1. The record exists in the database
2. Direct queries (without nested filters) work correctly
3. The relationship is properly configured

## Reproduction Steps

### Minimal Example Schema

```python
import fraiseql
from uuid import UUID

@fraiseql.type
class Product:
    id: UUID
    name: str
    manufacturer_id: UUID
    manufacturer: "Manufacturer"

@fraiseql.type
class Manufacturer:
    id: UUID
    name: str
    products: list[Product]

@fraiseql.input
class ProductWhereInput:
    id: UUIDFilter | None = None
    name: StringFilter | None = None
    manufacturer: "ManufacturerWhereInput | None" = None

@fraiseql.input
class ManufacturerWhereInput:
    id: UUIDFilter | None = None
    name: StringFilter | None = None
```

### Query That Fails

```graphql
query GetProducts($manufacturerId: ID!) {
    products(where: { manufacturer: { id: { eq: $manufacturerId } } }) {
        id
        name
        manufacturer {
            id
            name
        }
    }
}
```

**Variables:**
```json
{
  "manufacturerId": "01234567-89ab-cdef-0123-456789abcdef"
}
```

### Expected Behavior

Should return all products where `manufacturer_id` matches the provided UUID.

### Actual Behavior

1. Warning logged: `Unsupported operator: id`
2. Query returns empty array `[]`
3. Same products ARE returned when queried directly without nested filter:
   ```graphql
   query GetProducts {
       products {
           id
           name
       }
   }
   ```

## Analysis

The error suggests FraiseQL is interpreting `id` as an operator name rather than recognizing it as a field name with an `eq` operator applied.

The filter structure is:
```python
{
    "manufacturer": {
        "id": {          # ← This is the FIELD name
            "eq": "uuid"  # ← This is the OPERATOR
        }
    }
}
```

But FraiseQL appears to be treating `"id"` itself as an operator, leading to the "Unsupported operator: id" error.

## Workaround

None found. Direct queries work but nested filtering is broken for this use case.

## Impact

- **Severity**: High - Nested filtering is a core FraiseQL feature
- **Affected Operations**: All queries using nested `where` filters on ID fields
- **User Impact**: Cannot filter by related entity IDs, forcing inefficient client-side filtering

## Additional Context

This appears to be a regression or edge case in the where filter operator resolution logic. The error originates from `fraiseql/db.py:2580` in the operator strategy handling.

## Suggested Fix Areas

1. Review operator resolution logic in nested where filters
2. Ensure field names are distinguished from operator names in nested contexts
3. Add test coverage for nested ID filtering scenarios

## Test Case for Regression Prevention

```python
async def test_nested_filter_by_id():
    """Test filtering by related entity ID in nested where clause."""
    # Setup
    manufacturer = await create_manufacturer(name="ACME Corp")
    product = await create_product(
        name="Widget",
        manufacturer_id=manufacturer.id
    )

    # Query with nested filter
    query = """
        query GetProducts($manufacturerId: ID!) {
            products(where: { manufacturer: { id: { eq: $manufacturerId } } }) {
                id
                name
            }
        }
    """

    result = await execute(query, {"manufacturerId": str(manufacturer.id)})

    # Should find the product
    assert len(result["data"]["products"]) == 1
    assert result["data"]["products"][0]["id"] == str(product.id)
```

---

**Priority**: High
**Component**: Query Filtering / Where Clause Processing
**Regression Risk**: High - Core filtering functionality
