# E-commerce GraphQL API Example

🟡 INTERMEDIATE | ⏱️ 30 min | 🎯 E-commerce | 🏷️ Business Logic

A complete production-ready e-commerce API built with FraiseQL, demonstrating best practices for building GraphQL APIs that are backed by PostgreSQL.

**What you'll learn:**
- Complex business logic with cross-entity validation
- Shopping cart and order management
- User authentication and profiles
- Product catalog with search and filtering
- Real-world application patterns

**Prerequisites:**
- `../blog_api/` - Basic CRUD and enterprise patterns
- Understanding of business domains

**Next steps:**
- `../enterprise_patterns/` - Add compliance and audit trails
- `../analytics_dashboard/` - Add business intelligence
- `../real_time_chat/` - Add real-time inventory updates

## Features

- **Complete E-commerce Domain Model**
  - Users, Products, Cart, Orders, Reviews, Addresses
  - Wishlist, Coupons, Order tracking
  - Inventory management

- **Production-Ready Architecture**
  - CQRS pattern with mutations as PostgreSQL functions
  - Type-safe GraphQL schema generation
  - Built-in authentication support
  - Monitoring with Prometheus metrics
  - Distributed tracing with OpenTelemetry
  - Docker and Kubernetes ready

- **Security & Performance**
  - SQL injection protection
  - Rate limiting ready
  - Optimized database queries
  - Connection pooling
  - Caching support

## Quick Start

### Prerequisites

- Python 3.13+
- PostgreSQL 14+
- Redis (optional, for caching)

### Setup

1. **Install dependencies**
```bash
pip install fraiseql psycopg2-binary python-dotenv
```

2. **Set up the database**
```bash
# Create database
createdb ecommerce

# Run schema
psql -d ecommerce -f schema.sql

# Run functions
psql -d ecommerce -f functions.sql

# (Optional) Seed sample data
psql -d ecommerce -f seed_data.sql
```

3. **Configure environment**
```bash
# Copy example env file
cp .env.example .env

# Edit .env with your settings
DATABASE_URL=postgresql://localhost/ecommerce
ENVIRONMENT=development
```

4. **Run the application**
```bash
python -m app
```

The GraphQL API will be available at:
- GraphQL endpoint: http://localhost:8000/graphql
- GraphQL Playground: http://localhost:8000/playground (development only)
- Health check: http://localhost:8000/health

## Project Structure

```
ecommerce/
├── models.py          # GraphQL type definitions
├── mutations.py       # GraphQL mutations
├── queries.py         # GraphQL queries
├── schema.sql         # PostgreSQL schema
├── functions.sql      # PostgreSQL mutation functions
├── app.py            # FastAPI application
├── test_ecommerce.py # Test suite
└── README.md         # This file
```

## GraphQL Schema

### Core Types

#### User
```graphql
type User {
  id: UUID!
  email: String!
  name: String!
  phone: String
  isActive: Boolean!
  isVerified: Boolean!
  createdAt: DateTime!
}
```

#### Product
```graphql
type Product {
  id: UUID!
  sku: String!
  name: String!
  description: String!
  category: ProductCategory!
  price: Decimal!
  compareAtPrice: Decimal
  inventoryCount: Int!
  images: [String!]!
  tags: [String!]!
}
```

#### Order
```graphql
type Order {
  id: UUID!
  orderNumber: String!
  status: OrderStatus!
  paymentStatus: PaymentStatus!
  total: Decimal!
  placedAt: DateTime!
}
```

### Example Queries

#### Search Products
```graphql
query SearchProducts {
  products(
    filters: {
      category: ELECTRONICS
      minPrice: "100"
      maxPrice: "1000"
      inStock: true
    }
    limit: 20
  ) {
    items {
      id
      name
      price
      inventoryCount
    }
    totalCount
    hasNextPage
  }
}
```

#### Get Product with Reviews
```graphql
query GetProduct($id: UUID!) {
  productWithReviews(id: $id) {
    product {
      id
      name
      description
      price
      images
    }
    averageRating
    reviewCount
    reviews(limit: 5) {
      items {
        rating
        title
        comment
        user {
          name
        }
      }
    }
  }
}
```

### Example Mutations

#### User Registration
```graphql
mutation Register {
  register(input: {
    email: "user@example.com"
    password: "SecurePass123!"
    name: "John Doe"
  }) {
    ... on AuthSuccess {
      user {
        id
        email
      }
      token
    }
    ... on AuthError {
      message
      code
    }
  }
}
```

#### Add to Cart
```graphql
mutation AddToCart {
  addToCart(input: {
    productId: "123e4567-e89b-12d3-a456-426614174000"
    quantity: 2
  }) {
    ... on CartSuccess {
      cart {
        itemsCount
        subtotal
      }
    }
    ... on CartError {
      message
      code
    }
  }
}
```

#### Checkout
```graphql
mutation Checkout {
  checkout(input: {
    shippingAddressId: "address-uuid"
    billingAddressId: "address-uuid"
    couponCode: "SAVE10"
  }) {
    ... on OrderSuccess {
      order {
        orderNumber
        total
        status
      }
    }
    ... on OrderError {
      message
      code
    }
  }
}
```

## Testing

Run the test suite:
```bash
pytest test_ecommerce.py -v
```

The test suite covers:
- User registration and authentication
- Product search and filtering
- Cart operations
- Order placement
- Address management
- Review creation

## Production Deployment

### Docker

Build and run with Docker:
```bash
# Build image
docker build -t ecommerce-api .

# Run container
docker run -p 8000:8000 \
  -e DATABASE_URL=postgresql://user:pass@host/db \
  -e ENVIRONMENT=production \
  ecommerce-api
```

### Kubernetes

Deploy to Kubernetes:
```bash
kubectl apply -f k8s/
```

See the [deployment guide](../../docs/deployment/) for detailed instructions.

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `DATABASE_URL` | PostgreSQL connection string | `postgresql://localhost/ecommerce` |
| `ENVIRONMENT` | Environment (development/production) | `development` |
| `AUTH0_DOMAIN` | Auth0 domain for authentication | - |
| `AUTH0_API_IDENTIFIER` | Auth0 API identifier | - |
| `SESSION_SECRET` | Secret for session middleware | `dev-secret` |
| `CORS_ORIGINS` | Allowed CORS origins | `*` |
| `TRACING_ENDPOINT` | OpenTelemetry endpoint | - |
| `TRACING_FORMAT` | Tracing format (otlp/jaeger/zipkin) | `otlp` |

## Monitoring

The application includes:

- **Prometheus metrics** at `/metrics`
  - Query count and duration
  - Database connection pool stats
  - Cache hit rates
  - Error rates by type

- **Health checks**
  - `/health` - Basic health check
  - `/ready` - Readiness probe with database check

- **Distributed tracing**
  - OpenTelemetry integration
  - Automatic context propagation
  - Database query tracing

## Security Considerations

1. **Authentication**: Uses JWT tokens with Auth0 integration
2. **SQL Injection**: Protected by parameterized queries
3. **Rate Limiting**: Ready for rate limiting middleware
4. **CORS**: Configurable CORS origins
5. **Secrets**: Never committed to repository
6. **HTTPS**: Use TLS in production

## Performance Optimization

1. **Database**
   - Indexes on frequently queried fields
   - Materialized views for complex aggregations
   - Connection pooling
   - Query optimization

2. **Caching**
   - Redis integration ready
   - Query result caching
   - Session caching

3. **Monitoring**
   - Performance metrics
   - Slow query logging
   - Error tracking

## Contributing

See the main [FraiseQL contributing guide](../../CONTRIBUTING.md).

## License

This example is part of the FraiseQL project. See the [LICENSE](../../LICENSE) file for details.
