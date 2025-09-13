---
← [Filtering and Where Clauses](./filtering-and-where-clauses.md) | [Core Concepts Index](./index.md) | [Database Views →](./database-views.md)
---

# Ordering and Sorting

> **In this section:** Master GraphQL ordering with FraiseQL's JSONB-optimized sorting
> **Prerequisites:** Understanding of [Type System](./type-system.md) and [Filtering](./filtering-and-where-clauses.md)
> **Time to complete:** 15 minutes

FraiseQL provides powerful ordering capabilities that leverage PostgreSQL's native JSONB comparison for optimal performance and correct sorting behavior.

## Basic Ordering

FraiseQL automatically generates `OrderByInput` types for all your data types:

```python
@fraiseql.type
class Product:
    id: UUID
    name: str
    price: float
    quantity: int
    created_at: datetime

# Query with ordering
@fraiseql.query
async def products(
    info,
    order_by: Optional[List[ProductOrderBy]] = None
) -> List[Product]:
    repo = info.context["repo"]
    return await repo.find("v_product", order_by=order_by)
```

## GraphQL Ordering Syntax

### Single Field Ordering

```graphql
query {
  products(orderBy: [{field: "price", direction: ASC}]) {
    id
    name
    price
  }
}
```

### Multiple Field Ordering

```graphql
query {
  products(orderBy: [
    {field: "price", direction: DESC},
    {field: "name", direction: ASC}
  ]) {
    id
    name
    price
  }
}
```

### Nested Field Ordering

```graphql
query {
  users(orderBy: [{field: "profile.age", direction: DESC}]) {
    id
    name
    profile {
      age
    }
  }
}
```

## 🔧 **JSONB Numeric Ordering (v0.7.20+)**

!!! success "Critical Fix in v0.7.20"
    FraiseQL v0.7.20 fixed a critical bug where numeric fields were sorted lexicographically instead of numerically. This ensures proper data integrity for financial and numeric data.

### The Problem (Fixed in v0.7.20)

Before v0.7.20, FraiseQL used JSONB text extraction for ORDER BY clauses, causing incorrect sorting:

```sql
-- ❌ BEFORE v0.7.20: Text extraction (WRONG)
ORDER BY data ->> 'price' ASC
-- Result: ['1000.0', '1234.53', '125.0', '25.0'] (lexicographic - WRONG)
```

### The Solution (v0.7.20+)

FraiseQL now uses JSONB extraction that preserves data types:

```sql
-- ✅ AFTER v0.7.20: JSONB extraction (CORRECT)
ORDER BY data -> 'price' ASC
-- Result: [25.0, 125.0, 1000.0, 1234.53] (numeric - CORRECT)
```

### Impact on Your Application

| **Field Type** | **Before v0.7.20** | **v0.7.20+** | **Impact** |
|----------------|-------------------|---------------|------------|
| **Numeric** (`int`, `float`) | ❌ Lexicographic | ✅ Numeric | **Critical Fix** |
| **Financial** (`Decimal`, monetary) | ❌ String-based | ✅ Numeric | **Critical Fix** |
| **Dates** (`datetime`, `date`) | ✅ Correct | ✅ Correct | No change |
| **Strings** (`str`) | ✅ Correct | ✅ Correct | No change |

### Real-World Example

```python
# Financial data ordering - NOW WORKS CORRECTLY
@fraiseql.query
async def transactions_by_amount(info) -> List[Transaction]:
    return await repo.find(
        "v_transaction",
        order_by=[OrderBy(field="amount", direction="desc")]
    )
```

**Before v0.7.20:**
```json
[
  {"amount": "1000.0"},  // Wrong: String comparison
  {"amount": "1234.53"}, // "1" < "2" in strings
  {"amount": "125.0"},
  {"amount": "25.0"}
]
```

**v0.7.20+:**
```json
[
  {"amount": 1234.53},   // Correct: Numeric comparison
  {"amount": 1000.0},
  {"amount": 125.0},
  {"amount": 25.0}
]
```

## Performance Optimizations

### PostgreSQL Index Support

FraiseQL's JSONB ordering works optimally with PostgreSQL JSONB indexes:

```sql
-- Create JSONB indexes for frequently ordered fields
CREATE INDEX idx_product_price ON tb_product USING gin ((data -> 'price'));
CREATE INDEX idx_product_created_at ON tb_product USING gin ((data -> 'created_at'));

-- For numeric fields, consider expression indexes
CREATE INDEX idx_product_price_numeric ON tb_product ((data -> 'price')::numeric);
```

### Order By Best Practices

1. **Use Specific Field Types**: Define precise types for better ordering
   ```python
   # ✅ Good: Specific numeric type
   price: float

   # ❌ Avoid: Generic types that might be ambiguous
   price: Any
   ```

2. **Leverage Database Views**: Pre-sort in views when possible
   ```sql
   CREATE VIEW v_product_by_price AS
   SELECT data FROM tb_product
   ORDER BY data -> 'price' DESC;
   ```

3. **Combine with Pagination**: Always use ordering with pagination
   ```graphql
   query {
     products(
       orderBy: [{field: "price", direction: DESC}],
       first: 20,
       after: "cursor123"
     ) {
       edges { node { id name price } }
       pageInfo { hasNextPage endCursor }
     }
   }
   ```

## Architecture Insights

### Why the Fix Works

FraiseQL's ordering architecture uses different strategies for different operations:

| **Operation** | **Extraction Method** | **Purpose** |
|---------------|----------------------|-------------|
| **ORDER BY** | `data -> 'field'` | Preserve types for sorting |
| **WHERE clauses** | `(data ->> 'field')::type` | Cast for comparisons |
| **SELECT** | Both as needed | Context-dependent |

### JSONB vs Text Extraction

```sql
-- JSONB extraction (data -> 'field'): Preserves original data type
SELECT data -> 'price' FROM products;  -- Returns JSONB number

-- Text extraction (data ->> 'field'): Converts to text
SELECT data ->> 'price' FROM products; -- Returns text string
```

This architectural distinction ensures:
- ✅ **ORDER BY**: Uses type-preserving JSONB extraction
- ✅ **WHERE clauses**: Use text extraction with proper casting
- ✅ **Performance**: Optimal for each use case

## Common Patterns

### Multi-Level Sorting

```python
# Sort by price descending, then by name ascending
order_by = [
    OrderBy(field="price", direction="desc"),
    OrderBy(field="name", direction="asc")
]
```

### Dynamic Ordering

```python
@fraiseql.query
async def products_sorted(
    info,
    sort_field: str = "created_at",
    sort_direction: str = "desc"
) -> List[Product]:
    order_by = [OrderBy(field=sort_field, direction=sort_direction)]
    return await repo.find("v_product", order_by=order_by)
```

### Null Handling

PostgreSQL JSONB ordering naturally handles null values:

```sql
-- Nulls appear last in ascending order
ORDER BY data -> 'optional_field' ASC NULLS LAST

-- Nulls appear first in descending order
ORDER BY data -> 'optional_field' DESC NULLS FIRST
```

## Migration from Pre-v0.7.20

If you're upgrading from before v0.7.20:

### ✅ **No Action Required**

The fix is **fully backward compatible**:
- ✅ All existing GraphQL queries continue to work
- ✅ No breaking changes to your application code
- ✅ Ordering behavior automatically improves

### 🔍 **Verify Your Data**

After upgrading, verify that numeric ordering now works correctly:

```python
# Test numeric ordering
products = await repo.find(
    "v_product",
    order_by=[OrderBy(field="price", direction="asc")]
)

# Verify ascending numeric order
prices = [p.price for p in products]
assert prices == sorted(prices)  # Should now pass!
```

---

**Key Takeaways:**
- ✅ FraiseQL v0.7.20+ provides correct numeric ordering
- ✅ JSONB extraction preserves data types for optimal sorting
- ✅ No migration needed - improvement is automatic
- ✅ Better performance with native PostgreSQL comparison
- ✅ Critical for financial and e-commerce applications
