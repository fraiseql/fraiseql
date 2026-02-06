# Advanced Federation Patterns

Complex federation scenarios including circular references, 4+ subgraphs, field sharing, and conditional requirements.

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                    Apollo Router/Gateway                     │
│                      (Port 4000)                             │
└────┬────────────┬──────────────┬──────────────┬──────────────┘
     │            │              │              │
┌────▼──┐  ┌──────▼──┐  ┌───────▼───┐  ┌──────▼──┐
│Users  │  │Companies│  │Inventory  │  │Analytics│
│(4001) │  │(4002)   │  │(4003)     │  │(4004)   │
└────┬──┘  └──┬──────┘  └───┬───────┘  └──┬──────┘
     │       │             │             │
     └───────┼─────────────┼─────────────┘
             │             │
         (Circular:    (Shared:
          User ↔      Order @shareable
          Company)    Product @shareable)
```

## Key Features

- **Circular References**: User ↔ Company bidirectional federation
- **Shared Fields**: Multiple subgraphs provide same fields
- **Conditional Requirements**: @requires on complex conditions
- **4-Tier Hierarchy**: User → Company → Order → Product
- **Mixed Strategies**: Local DB + HTTP federation

## Complexity Patterns

### Pattern 1: Circular References (User ↔ Company)

**Problem**:

- Users belong to companies
- Companies have users
- Federation composability issue if not handled correctly

**Solution**:

Users Service:

```python
@type
@extends
@key(fields=["id"])
class Company:
    """Company extended from company-service"""
    id: str = external()
    users: list["User"] = requires(fields=["id"])

@type
@key(fields=["id"])
class User:
    """User entity owned by users-service"""
    id: str
    company_id: str
    name: str
```

Companies Service:

```python
@type
@extends
@key(fields=["id"])
class User:
    """User extended from users-service"""
    id: str = external()
    company_id: str = external()

@type
@key(fields=["id"])
class Company:
    """Company entity owned by companies-service"""
    id: str
    name: str
    users: list["User"] = requires(fields=["id"])
```

**Result**: Bidirectional traversal:

```graphql
query {
  company(id: "c1") {
    users {
      name
      company { name }  # Back reference works!
    }
  }
}
```

### Pattern 2: Shared Fields (@shareable)

**Problem**:
Multiple services need to provide the same field with different logic.

**Solution**:

Orders Service:

```python
@type
class Product:
    """Product with order-specific pricing"""
    id: str
    order_id: str
    name: str
    price: float  # @shareable: order price (may differ from catalog)
```

Inventory Service:

```python
@type
@shareable
class Product:
    """Product with inventory-specific data"""
    id: str
    name: str
    price: float  # @shareable: catalog price
    stock: int
```

**Result**: Both services can provide `price`, federation composition handles the merge.

### Pattern 3: Conditional Requirements (@requires)

**Problem**:
Resolving a field requires different input fields depending on context.

**Solution**:

Orders Service:

```python
@type
@extends
class Product:
    id: str = external()
    # Only need 'stock_level' field from inventory for 'can_order'
    can_order: bool = requires(fields=["stock_level"])
```

### Pattern 4: Multi-Tier Hierarchy

**Hierarchy**: User → Company → Order → Product

**Traversal Example**:

```graphql
query {
  users {
    id
    company {
      name
      orders {
        id
        products {
          name
        }
      }
    }
  }
}
```

This requires:

1. Users service (owns User)
2. Companies service (extends User, owns Company)
3. Orders service (extends Company, owns Order)
4. Inventory service (extends Order, owns Product)

## Setup

### Docker Compose

```bash
docker-compose up -d

# Wait for services
sleep 10
docker-compose ps
```

### Database Schemas

**Users Service**

```sql
CREATE TABLE users (
  id VARCHAR(50) PRIMARY KEY,
  company_id VARCHAR(50) NOT NULL,
  name VARCHAR(255) NOT NULL
);
```

**Companies Service**

```sql
CREATE TABLE companies (
  id VARCHAR(50) PRIMARY KEY,
  name VARCHAR(255) NOT NULL,
  parent_company_id VARCHAR(50)
);
```

**Orders Service**

```sql
CREATE TABLE orders (
  id VARCHAR(50) PRIMARY KEY,
  company_id VARCHAR(50) NOT NULL,
  status VARCHAR(50),
  amount DECIMAL(10, 2)
);
```

**Inventory Service**

```sql
CREATE TABLE products (
  id VARCHAR(50) PRIMARY KEY,
  name VARCHAR(255) NOT NULL,
  price DECIMAL(10, 2),
  stock INT
);

CREATE TABLE order_items (
  order_id VARCHAR(50) NOT NULL,
  product_id VARCHAR(50) NOT NULL,
  PRIMARY KEY (order_id, product_id)
);
```

## Example Queries

### Circular Reference Query

```graphql
query {
  company(id: "c1") {
    name
    users {
      name
      company {
        name
        users { name }  # Deep circular traversal
      }
    }
  }
}
```

### Multi-Tier Hierarchy Query

```graphql
query {
  users {
    name
    company {
      name
      orders {
        status
        amount
        products {
          name
          price
          stock
        }
      }
    }
  }
}
```

### Shared Field Query

```graphql
query {
  products {
    id
    name
    price        # Could come from orders-service or inventory-service
    stock        # Only from inventory-service
    order { id } # Only if accessed via orders-service
  }
}
```

## Performance Characteristics

| Query | Latency | Explanation |
|-------|---------|-------------|
| Single user | <5ms | Direct local query |
| User + company | 10-20ms | 1 federation hop |
| User + company + orders | 20-40ms | 2 federation hops |
| Full 4-tier hierarchy | 40-100ms | 3 federation hops |
| Circular reference | 15-30ms | Cached after first resolution |

## Advanced Troubleshooting

### Circular Reference Infinite Loop

**Symptom**: Query hangs or returns with many repeated fields

**Solution**: Use field aliases to break cycles:

```graphql
query {
  company(id: "c1") {
    name
    users {
      name
      parentCompany: company {
        name
        # Stop here - don't query users again
      }
    }
  }
}
```

### Shared Field Conflicts

**Symptom**: Different services provide conflicting field definitions

**Solution**: Ensure `@shareable` fields have identical types:

```python
# ✅ Correct
class Product:
    price: float  # Same type in all services

# ❌ Wrong
# Users service: price: str
# Inventory service: price: float
```

### N+1 Query Problem in 4-Tier Hierarchy

**Symptom**: Performance degrades with nested queries

**Solution**: Use field selection and batching:

```graphql
query {
  users {
    id
    company {
      id
      orders {
        id
        products { id }  # Don't select all fields if not needed
      }
    }
  }
}
```

## Advanced Patterns in Production

### Pattern: Service Specialization

Different services can own different aspects of the same entity:

- **Users Service**: Owns user identity, credentials
- **Profile Service**: Owns biographical data, preferences
- **Billing Service**: Owns subscription, payment info

One query can pull from all three:

```graphql
query {
  user(id: "u1") {
    email
    profile { bio }
    billing { subscription }
  }
}
```

### Pattern: Gradual Migration

Migrate fields from one service to another:

1. **Week 1**: Orders service provides `Order.products`
2. **Week 2**: Inventory service also provides `Order.products` (@shareable)
3. **Week 3**: Switch routing from Orders → Inventory
4. **Week 4**: Remove from Orders service

### Pattern: Feature Flags via Federation

Different versions of same entity based on context:

```graphql
query {
  user(id: "u1", features: ["analytics"]) {
    id
    name
    analytics { views clicks }  # Only if features include "analytics"
  }
}
```

## Deployment Considerations

- **Circuit Breakers**: Fail gracefully if one subgraph is down
- **Caching**: Cache circular reference resolutions
- **Monitoring**: Track per-path latency (User→Company→Orders→Products)
- **Testing**: Test all circular paths before production
- **Documentation**: Document circular references clearly

## Next Steps

1. Start with Pattern 1 (circular references)
2. Add Pattern 2 (shared fields)
3. Introduce Pattern 3 (conditional requirements)
4. Extend to full 4-tier hierarchy
5. Monitor performance at each step
