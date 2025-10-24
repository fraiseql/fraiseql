# Auto-Documentation Example

Production-ready example demonstrating FraiseQL's automatic GraphQL schema documentation generation from Python docstrings and type hints.

## What This Example Demonstrates

This example shows how FraiseQL **automatically generates comprehensive GraphQL documentation** without any extra configuration:
- ✅ Type-level documentation from class docstrings
- ✅ Field-level documentation from attribute docstrings
- ✅ Query/mutation documentation from function docstrings
- ✅ Argument documentation from parameter docstrings
- ✅ Enum value documentation
- ✅ Complex example queries embedded in docstrings
- ✅ Introspection-compatible documentation

**Key Benefit:** Write your Python documentation once, get GraphQL documentation automatically in all GraphQL clients (Playground, Apollo Studio, GraphiQL, Altair, etc.)

## The Documentation Problem

### Traditional GraphQL Approach

Most GraphQL frameworks require **duplicate documentation**:

```python
# Python code with docstrings
class Product:
    """A product in the catalog."""
    id: int
    """Product ID"""
    name: str
    """Product name"""

# PLUS separate GraphQL SDL documentation
"""
type Product {
  "A product in the catalog"
  id: Int!
  "Product ID"
  name: String!
  "Product name"
}
"""
```

**Problems:**
- 😫 Documentation written twice
- 🐛 Docs get out of sync
- ⏰ Wastes development time
- 📝 More to maintain

### FraiseQL Approach

Write documentation **once** in Python:

```python
@app.type
@dataclass
class Product:
    """A product in the catalog.

    Products support inventory tracking,
    multiple images, and customer reviews.
    """

    id: int
    """Unique product identifier (auto-generated)"""

    name: str
    """Product display name.

    Maximum 200 characters. Used in search
    results and product listings.
    """
```

**FraiseQL automatically generates:**
- ✅ GraphQL schema with full documentation
- ✅ Introspection responses
- ✅ GraphQL Playground documentation
- ✅ Apollo Studio documentation
- ✅ All client library documentation

**Result:** Single source of truth, always in sync.

## Features Demonstrated

### 1. Type Documentation

**Python:**
```python
@app.type
@dataclass
class Product:
    """A product in the e-commerce catalog.

    Products can be physical goods, digital downloads, or services.
    Each product has pricing, inventory tracking, and categorization.
    All products support multiple images and detailed specifications.
    """
    id: int
    name: str
    price: Decimal
```

**Generated GraphQL:**
```graphql
"""
A product in the e-commerce catalog.

Products can be physical goods, digital downloads, or services.
Each product has pricing, inventory tracking, and categorization.
All products support multiple images and detailed specifications.
"""
type Product {
  id: Int!
  name: String!
  price: Decimal!
}
```

### 2. Field Documentation

**Python:**
```python
@app.type
@dataclass
class Product:
    id: int
    """Unique product identifier (auto-generated)"""

    name: str
    """Product display name.

    Maximum 200 characters. Used in search results and product listings.
    Should be descriptive and include key features.
    """

    price: Decimal
    """Price in USD.

    Supports up to 2 decimal places (e.g., 19.99).
    Does not include taxes or shipping costs.
    """
```

**Generated GraphQL:**
```graphql
type Product {
  "Unique product identifier (auto-generated)"
  id: Int!

  """
  Product display name.

  Maximum 200 characters. Used in search results and product listings.
  Should be descriptive and include key features.
  """
  name: String!

  """
  Price in USD.

  Supports up to 2 decimal places (e.g., 19.99).
  Does not include taxes or shipping costs.
  """
  price: Decimal!
}
```

### 3. Query Documentation with Examples

**Python:**
```python
@app.query
async def products(
    info,
    category: ProductCategory | None = None,
    in_stock_only: bool = False,
    min_price: Decimal | None = None,
    max_price: Decimal | None = None,
    limit: int = 20
) -> list[Product]:
    """Query products with flexible filtering.

    Supports filtering by category, availability, price range, and ratings.
    Results are paginated and sorted by relevance.

    Args:
        category: Filter by product category (optional)
        in_stock_only: If True, only return available products
        min_price: Minimum price filter (inclusive)
        max_price: Maximum price filter (inclusive)
        limit: Maximum number of results (default: 20, max: 100)

    Returns:
        List of products matching the filters

    Example:
        ```graphql
        {
          products(
            category: ELECTRONICS,
            in_stock_only: true,
            min_price: 10.00,
            max_price: 100.00,
            limit: 10
          ) {
            id
            name
            price
          }
        }
        ```
    """
    # Implementation...
```

**Generated GraphQL:**
```graphql
"""
Query products with flexible filtering.

Supports filtering by category, availability, price range, and ratings.
Results are paginated and sorted by relevance.

Args:
    category: Filter by product category (optional)
    in_stock_only: If True, only return available products
    min_price: Minimum price filter (inclusive)
    max_price: Maximum price filter (inclusive)
    limit: Maximum number of results (default: 20, max: 100)

Returns:
    List of products matching the filters

Example:
    {
      products(
        category: ELECTRONICS,
        in_stock_only: true,
        min_price: 10.00,
        max_price: 100.00,
        limit: 10
      ) {
        id
        name
        price
      }
    }
"""
products(
  category: ProductCategory
  inStockOnly: Boolean = false
  minPrice: Decimal
  maxPrice: Decimal
  limit: Int = 20
): [Product!]!
```

### 4. Enum Documentation

**Python:**
```python
class ProductCategory(str, Enum):
    """Product category classification.

    Categories help organize products for browsing and filtering.
    Each product must belong to exactly one category.
    """

    ELECTRONICS = "electronics"
    """Electronic devices and accessories"""

    CLOTHING = "clothing"
    """Apparel and fashion items"""

    BOOKS = "books"
    """Physical and digital books"""
```

**Generated GraphQL:**
```graphql
"""
Product category classification.

Categories help organize products for browsing and filtering.
Each product must belong to exactly one category.
"""
enum ProductCategory {
  "Electronic devices and accessories"
  ELECTRONICS

  "Apparel and fashion items"
  CLOTHING

  "Physical and digital books"
  BOOKS
}
```

## Setup

### 1. Install Dependencies

```bash
cd examples/documented_api
pip install -r requirements.txt
```

### 2. Setup Database

```bash
# Create database
createdb ecommerce_docs

# Run schema
psql ecommerce_docs < schema.sql
```

### 3. Run the Example

```bash
python main.py
```

The API will be available at:
- **GraphQL API:** http://localhost:8000/graphql
- **GraphQL Playground:** http://localhost:8000/graphql (with full documentation)

## Using the Documentation

### In GraphQL Playground

1. Open http://localhost:8000/graphql
2. Click **"Docs"** tab on the right side
3. Browse the schema documentation
4. Click any type/field to see full documentation

**Features:**
- Full type descriptions
- Field descriptions with formatting
- Argument documentation
- Enum value documentation
- Example queries from docstrings

### In Apollo Studio

1. Configure Apollo Studio with your endpoint
2. Use Schema > Reference tab
3. Full documentation automatically available

### Programmatic Introspection

```graphql
query GetTypeDocumentation {
  __type(name: "Product") {
    name
    description
    fields {
      name
      description
      type {
        name
        description
      }
    }
  }
}
```

Response includes all docstring content:
```json
{
  "__type": {
    "name": "Product",
    "description": "A product in the e-commerce catalog.\n\nProducts can be physical goods, digital downloads, or services...",
    "fields": [
      {
        "name": "id",
        "description": "Unique product identifier (auto-generated)",
        "type": {
          "name": "Int",
          "description": null
        }
      }
    ]
  }
}
```

## Documentation Best Practices

### 1. Be Descriptive But Concise

**❌ Too Short:**
```python
price: Decimal
"""Product price"""
```

**❌ Too Long:**
```python
price: Decimal
"""The price of the product which is stored as a decimal value in the database
and represents the amount that the customer will need to pay when they purchase
this product from our e-commerce platform, not including any taxes, shipping
costs, or other fees that may be added at checkout..."""
```

**✅ Just Right:**
```python
price: Decimal
"""Price in USD.

Supports up to 2 decimal places (e.g., 19.99).
Does not include taxes or shipping costs.
"""
```

### 2. Include Usage Examples

For complex queries, include GraphQL examples:

```python
@app.query
async def search_products(info, query: str, filters: SearchFilters) -> list[Product]:
    """Full-text product search with filters.

    Example:
        ```graphql
        query {
          searchProducts(
            query: "wireless headphones",
            filters: {
              category: ELECTRONICS,
              priceRange: { min: 20, max: 200 }
            }
          ) {
            id
            name
            price
          }
        }
        ```
    """
```

### 3. Document Constraints and Validation

```python
rating: int
"""Star rating (1-5).

1 = Very Poor
2 = Poor
3 = Average
4 = Good
5 = Excellent

Must be between 1 and 5 inclusive.
"""
```

### 4. Explain Null Behavior

```python
average_rating: float | None
"""Average customer rating (1.0 to 5.0 stars).

Calculated from all customer reviews.
Null if no reviews exist yet.
"""
```

### 5. Use Markdown Formatting

FraiseQL preserves markdown in docstrings:

```python
description: str
"""Product description in **markdown** format.

Supports:
- **Bold** and *italic* text
- Lists and `code blocks`
- [Links](https://example.com)

Rendered in product detail pages.
"""
```

## Documentation Styles

### Style 1: Inline Field Docs

```python
@app.type
@dataclass
class User:
    id: int
    """Unique user identifier"""

    email: str
    """User email address (used for login)"""

    name: str
    """User's full name"""
```

**Best for:** Simple, single-line field descriptions

### Style 2: Multi-line Field Docs

```python
@app.type
@dataclass
class Product:
    name: str
    """Product display name.

    Maximum 200 characters. Used in search results
    and product listings. Should be descriptive
    and include key features.
    """
```

**Best for:** Fields needing detailed explanation

### Style 3: Args Documentation

```python
@app.query
async def products(
    info,
    category: ProductCategory | None = None,
    in_stock_only: bool = False,
    limit: int = 20
) -> list[Product]:
    """Query products with filtering.

    Args:
        category: Filter by product category (optional)
        in_stock_only: If True, only return available products
        limit: Maximum number of results (default: 20, max: 100)

    Returns:
        List of products matching the filters
    """
```

**Best for:** Complex queries with multiple parameters

## Example Queries

### Browse Products

```graphql
query BrowseProducts {
  products(limit: 20) {
    id
    name
    description
    price
    category
    inStock
    averageRating
    reviewCount
  }
}
```

### Filter Products

```graphql
query FilteredProducts {
  products(
    category: ELECTRONICS
    inStockOnly: true
    minPrice: 10.00
    maxPrice: 500.00
    minRating: 4.0
    limit: 10
  ) {
    id
    name
    price
    averageRating
  }
}
```

### Get Product with Reviews

```graphql
query ProductDetails {
  product(id: 1) {
    id
    name
    description
    price
    stockQuantity
    averageRating
  }

  reviews(productId: 1, verifiedOnly: true, limit: 5) {
    id
    customerName
    rating
    title
    content
    verifiedPurchase
    helpfulCount
    createdAt
  }
}
```

## Advanced Documentation Features

### Deprecated Fields

```python
@app.type
@dataclass
class Product:
    old_price: Decimal | None
    """[DEPRECATED] Use 'price' field instead.

    This field will be removed in v2.0.
    """
```

### Field Relationships

```python
@app.type
@dataclass
class Order:
    customer_id: int
    """Foreign key to Customer.id.

    Use the 'customer' resolver to fetch
    full customer details.
    """
```

### Performance Notes

```python
@app.query
async def expensive_report(info) -> Report:
    """Generate analytics report.

    **Warning:** This query performs heavy aggregations
    and may take 10-30 seconds to complete.
    Consider using the async export API for large datasets.
    """
```

## Troubleshooting

### Documentation Not Appearing

**Problem:** Docstrings not showing in GraphQL Playground

**Solutions:**
1. Ensure docstrings use `"""` (triple quotes)
2. Check docstring is immediately after field/function
3. Verify FraiseQL version >= 0.10.0
4. Clear browser cache and reload Playground

### Markdown Not Rendering

**Problem:** Markdown shows as plain text

**Cause:** Most GraphQL clients show documentation as plain text

**Solution:** This is expected - markdown is preserved but not rendered in introspection. Frontend apps can render the markdown when displaying documentation.

### Missing Argument Docs

**Problem:** Query arguments don't show documentation

**Solution:** Use Args-style docstrings:

```python
@app.query
async def search(info, query: str, limit: int) -> list[Result]:
    """Search resources.

    Args:
        query: Search query string
        limit: Maximum results
    """
```

## Comparison: FraiseQL vs Other Frameworks

| Feature | FraiseQL | GraphQL-Python | Strawberry | Ariadne |
|---------|----------|----------------|------------|---------|
| **Auto-docs from docstrings** | ✅ Automatic | ❌ Manual | ⚠️ Partial | ❌ Manual |
| **Field descriptions** | ✅ From docstrings | ❌ Separate decorators | ⚠️ From docstrings | ❌ SDL only |
| **Enum value docs** | ✅ Automatic | ❌ Manual | ⚠️ Partial | ❌ SDL only |
| **Example queries** | ✅ In docstrings | ❌ Not supported | ❌ Not supported | ❌ Not supported |
| **Markdown support** | ✅ Preserved | ⚠️ Varies | ⚠️ Varies | ❌ Plain text |
| **Single source of truth** | ✅ Yes | ❌ No | ⚠️ Partial | ❌ No |

## Production Tips

### 1. Document Public APIs Thoroughly

For public GraphQL APIs, comprehensive documentation is critical:

```python
@app.query
async def public_data(info, filters: PublicFilters) -> list[Data]:
    """Access public dataset.

    **Rate Limit:** 100 requests per hour
    **Authentication:** API key required
    **Data Freshness:** Updated every 15 minutes

    Args:
        filters: Filter criteria (see PublicFilters documentation)

    Returns:
        Array of matching records (max 1000 per request)

    Example:
        ```graphql
        query {
          publicData(filters: { category: "health" }) {
            id
            title
            value
          }
        }
        ```
    """
```

### 2. Use Consistent Terminology

Maintain a terminology guide:

- "User" vs "Customer" vs "Account"
- "Product" vs "Item" vs "SKU"
- "Order" vs "Purchase" vs "Transaction"

### 3. Link Related Types

```python
order_id: int
"""Parent order ID.

See Order type for full order details.
Use orderById(id: order_id) to fetch.
"""
```

### 4. Document Error Behavior

```python
@app.mutation
async def create_order(info, input: OrderInput) -> Order:
    """Create a new order.

    **Errors:**
    - INSUFFICIENT_STOCK: Product out of stock
    - INVALID_PAYMENT: Payment method declined
    - INVALID_ADDRESS: Shipping address incomplete

    Returns:
        Created order on success
    """
```

## Related Examples

- [`../fastapi/`](../fastapi/) - FastAPI integration with auto-docs
- [`../filtering/`](../filtering/) - Filter operators documentation
- [`../specialized_types/`](../specialized_types/) - Custom scalar docs

## References

- [GraphQL Specification - Descriptions](https://spec.graphql.org/June2018/#sec-Descriptions)
- [Best Practices - Documentation](https://graphql.org/learn/best-practices/#documentation)

---

**This example demonstrates FraiseQL's zero-configuration documentation generation. Write Python docstrings once, get comprehensive GraphQL documentation everywhere!** ✨
