# E-commerce API Example - Complex Validation Patterns

A comprehensive e-commerce API demonstrating advanced validation patterns and cross-entity business rules with FraiseQL.

## Patterns Demonstrated

### ✅ Cross-Entity Validation
- Inventory availability checking across product variants
- Customer credit limit validation during checkout
- Price consistency validation across order items
- See: Enterprise mutation classes with complex validation logic

### ✅ Multi-Layer Validation
- GraphQL: Type validation, required fields
- App layer: Basic bounds checking, format validation
- Core layer: Business rules, cross-entity consistency
- Database: Constraint validation, transaction integrity
- See: `ProcessOrderEnterprise` and validation examples

### ✅ Enterprise Error Handling
- Structured error responses with field-level context
- Business rule violation details
- System constraint information
- Actionable suggestions for resolution
- See: Enhanced error types with detailed context

### ✅ NOOP Handling for Business Rules
- Inventory shortfall scenarios
- Credit limit exceeded cases
- Pricing discrepancy detection
- Concurrent modification conflicts
- See: NOOP result types with business context

### ❌ Complete Audit Trails
For full audit trail implementation, see `../enterprise_patterns/`

## Features

- **Product Catalog**: Categories, products with variants, validation patterns
- **Shopping Cart**: Multi-layer validation with inventory checking
- **Order Management**: Cross-entity validation and business rule enforcement
- **Customer Accounts**: Comprehensive validation and eligibility checking
- **Reviews & Ratings**: Verified purchase validation
- **Wishlist**: User eligibility and product availability validation
- **Inventory Management**: Real-time validation with business rule enforcement
- **Search & Filtering**: Validated filtering with business logic
- **Coupons & Discounts**: Complex eligibility validation patterns

## Architecture

This example demonstrates FraiseQL's CQRS architecture:

- **Views** for queries: Optimized PostgreSQL views for read operations
- **Functions** for mutations: Business logic encapsulated in PostgreSQL functions
- **Type Safety**: Pydantic models ensure type safety throughout
- **Performance**: Materialized views and indexes for optimal performance

## Setup

### 1. Database Setup

```bash
# Create database
createdb ecommerce

# Run migrations
psql -d ecommerce -f db/migrations/001_initial_schema.sql

# Create views
psql -d ecommerce -f db/views/product_views.sql
psql -d ecommerce -f db/views/customer_order_views.sql

# Create mutation functions
psql -d ecommerce -f db/functions/cart_functions.sql
psql -d ecommerce -f db/functions/order_functions.sql
psql -d ecommerce -f db/functions/customer_functions.sql

# Load sample data (optional)
psql -d ecommerce -f db/seeds/sample_data.sql
```

### 2. Environment Setup

```bash
# Create virtual environment
python -m venv venv
source venv/bin/activate  # On Windows: venv\Scripts\activate

# Install dependencies
pip install fraiseql fastapi uvicorn asyncpg

# Set environment variables
export DATABASE_URL="postgresql://user:password@localhost:5432/ecommerce"
```

### 3. Run the Application

```bash
# Run the server
uvicorn app:app --reload

# Access the API
# GraphQL Playground: http://localhost:8000/graphql
# REST API Docs: http://localhost:8000/docs
```

## GraphQL Examples

### Product Search

```graphql
query SearchProducts {
  productSearch(
    where: {
      name: { _ilike: "%laptop%" }
      inStock: { _eq: true }
      minPrice: { _gte: 500 }
      maxPrice: { _lte: 2000 }
    }
    orderBy: { averageRating: DESC }
    limit: 10
  ) {
    id
    name
    slug
    minPrice
    maxPrice
    primaryImageUrl
    categoryName
    reviewCount
    averageRating
  }
}
```

### Get Product Details

```graphql
query GetProduct($productId: UUID!) {
  productDetail(where: { id: { _eq: $productId } }) {
    id
    name
    description
    category
    images
    variants
    reviewSummary
  }
}
```

### Add to Cart

```graphql
mutation AddToCart($variantId: UUID!, $quantity: Int!) {
  addToCart(variantId: $variantId, quantity: $quantity) {
    success
    message
    cartId
    cart {
      id
      items
      subtotal
      itemCount
    }
  }
}
```

### Create Order

```graphql
mutation CreateOrder($cartId: UUID!, $customerId: UUID!, $addressId: UUID!) {
  createOrder(
    cartId: $cartId
    customerId: $customerId
    shippingAddressId: $addressId
  ) {
    success
    orderId
    orderNumber
    totalAmount
    order {
      id
      status
      items
      shippingAddress
    }
  }
}
```

### Submit Review

```graphql
mutation SubmitReview($productId: UUID!, $rating: Int!, $comment: String) {
  submitReview(
    customerId: "YOUR_CUSTOMER_ID"
    productId: $productId
    rating: $rating
    comment: $comment
  ) {
    success
    reviewId
    isVerifiedPurchase
  }
}
```

## Performance Features

### 1. Optimized Views

- `product_search`: Full-text search with aggregated data
- `category_tree`: Recursive CTE for hierarchical categories
- `shopping_cart`: Denormalized cart data for single query retrieval

### 2. Smart Indexes

- Trigram indexes for fuzzy search
- Partial indexes for active records
- Composite indexes for common query patterns

### 3. Materialized Views (Optional)

For high-traffic scenarios, convert views to materialized:

```sql
-- Convert to materialized view
CREATE MATERIALIZED VIEW product_search_mat AS
SELECT * FROM product_search;

-- Create indexes
CREATE INDEX idx_product_search_mat_name ON product_search_mat USING gin(name gin_trgm_ops);
CREATE INDEX idx_product_search_mat_category ON product_search_mat(category_id);

-- Refresh periodically
REFRESH MATERIALIZED VIEW CONCURRENTLY product_search_mat;
```

## Testing

### Run Tests

```bash
# Unit tests
pytest tests/

# Load tests
locust -f tests/load_test.py --host=http://localhost:8000

# GraphQL query tests
pytest tests/test_graphql_queries.py
```

### Test Coverage Areas

- Cart operations with inventory
- Order creation and fulfillment
- Concurrent cart updates
- Review submission and moderation
- Search performance

## Production Considerations

### 1. Security

- Implement proper authentication (JWT, OAuth)
- Add rate limiting for mutations
- Validate all inputs
- Use prepared statements (handled by FraiseQL)

### 2. Caching

- Redis for session management
- Query result caching
- Static asset CDN

### 3. Monitoring

- Query performance tracking
- Inventory level alerts
- Order processing metrics
- Error tracking with Sentry

### 4. Scaling

- Read replicas for queries
- Connection pooling
- Horizontal scaling with load balancer
- Background job processing for orders

## Comparison with Other Solutions

### vs Hasura

- **FraiseQL**: Business logic in PostgreSQL, better performance
- **Hasura**: More features out-of-box, but less flexible

### vs PostGraphile

- **FraiseQL**: Explicit schema definition, better TypeScript support
- **PostGraphile**: Auto-generated from database, more magic

### vs Custom GraphQL

- **FraiseQL**: 10x faster development, consistent patterns
- **Custom**: More control, but more code to maintain

## Next Steps

1. Add authentication and authorization
2. Implement GraphQL subscriptions for real-time updates
3. Add payment gateway integration
4. Implement recommendation engine
5. Add multi-language support
6. Create admin dashboard

## Resources

- [FraiseQL Documentation](https://fraiseql.dev)
- [PostgreSQL Best Practices](https://wiki.postgresql.org/wiki/Main_Page)
- [GraphQL Best Practices](https://graphql.org/learn/best-practices/)
