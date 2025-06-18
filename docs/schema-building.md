# Schema Building

FraiseQL automatically builds GraphQL schemas from your Python types and decorators. This document explains how the schema building process works and how to customize it.

## Automatic Schema Generation

FraiseQL scans your types and generates a complete GraphQL schema:

```python
import fraiseql

@fraiseql.type
class User:
    id: int
    name: str
    email: str

@fraiseql.query
async def users(info) -> list[User]:
    return await get_all_users()

# Schema is automatically built from your types
app = fraiseql.create_fraiseql_app(
    types=[User],
    queries=[users]
)
```

## Schema Components

### Types

FraiseQL converts Python classes to GraphQL types:

```python
@fraiseql.type
class Product:
    id: int
    name: str
    price: float
    in_stock: bool
```

Generates:

```graphql
type Product {
  id: Int!
  name: String!
  price: Float!
  inStock: Boolean!
}
```

### Queries

Query functions become GraphQL query fields:

```python
@fraiseql.query
async def product(info, id: int) -> Product | None:
    return await Product.get_by_id(id)

@fraiseql.query
async def products(info, limit: int = 10) -> list[Product]:
    return await Product.get_all(limit=limit)
```

Generates:

```graphql
type Query {
  product(id: Int!): Product
  products(limit: Int = 10): [Product!]!
}
```

### Mutations

Mutation functions are added to the Mutation type:

```python
@fraiseql.mutation
async def create_product(info, name: str, price: float) -> Product:
    return await Product.create(name=name, price=price)
```

Generates:

```graphql
type Mutation {
  createProduct(name: String!, price: Float!): Product!
}
```

### Subscriptions

Subscription functions create real-time GraphQL subscriptions:

```python
@fraiseql.subscription
async def product_updates(info) -> AsyncGenerator[Product, None]:
    async for product in product_stream():
        yield product
```

Generates:

```graphql
type Subscription {
  productUpdates: Product!
}
```

## Input Types

FraiseQL automatically generates input types:

```python
@fraiseql.input
class CreateProductInput:
    name: str
    price: float
    category_id: int

@fraiseql.mutation
async def create_product(info, input: CreateProductInput) -> Product:
    return await Product.create(**input.__dict__)
```

Generates:

```graphql
input CreateProductInput {
  name: String!
  price: Float!
  categoryId: Int!
}

type Mutation {
  createProduct(input: CreateProductInput!): Product!
}
```

## Enums

Python enums become GraphQL enums:

```python
from enum import Enum

@fraiseql.enum
class ProductStatus(Enum):
    DRAFT = "draft"
    PUBLISHED = "published"
    ARCHIVED = "archived"

@fraiseql.type
class Product:
    id: int
    name: str
    status: ProductStatus
```

Generates:

```graphql
enum ProductStatus {
  DRAFT
  PUBLISHED
  ARCHIVED
}

type Product {
  id: Int!
  name: String!
  status: ProductStatus!
}
```

## Interfaces

Define shared fields across types:

```python
@fraiseql.interface
class Node:
    id: int

@fraiseql.type
class User(Node):
    name: str
    email: str

@fraiseql.type
class Product(Node):
    name: str
    price: float
```

Generates:

```graphql
interface Node {
  id: Int!
}

type User implements Node {
  id: Int!
  name: String!
  email: String!
}

type Product implements Node {
  id: Int!
  name: String!
  price: Float!
}
```

## Custom Scalars

Define custom scalar types:

```python
from datetime import datetime
import fraiseql

# Custom scalar for datetime
DateTimeScalar = fraiseql.scalar(
    datetime,
    name="DateTime",
    description="ISO 8601 datetime string"
)

@fraiseql.type
class Event:
    id: int
    name: str
    start_time: datetime  # Uses DateTimeScalar
```

## Schema Validation

FraiseQL validates your schema during build:

```python
# This will raise an error if the schema is invalid
schema = fraiseql.build_fraiseql_schema(
    types=[User, Product],
    queries=[users, products],
    mutations=[create_user, create_product]
)
```

Common validation errors:

- Missing return type annotations
- Circular type references
- Invalid field types
- Duplicate type names

## Schema Introspection

Enable introspection for development:

```python
app = fraiseql.create_fraiseql_app(
    types=[User],
    enable_introspection=True  # Default: True in development
)
```

## Schema Documentation

Add descriptions to your types and fields:

```python
@fraiseql.type(description="A user of the application")
class User:
    id: int = fraiseql.field(description="Unique user identifier")
    name: str = fraiseql.field(description="User's display name")
    
    @fraiseql.field(description="User's email address")
    def email(self, info) -> str:
        return self.email_address
```

## Advanced Schema Building

### Custom Type Resolution

Override default type resolution:

```python
def custom_type_resolver(obj, info, type_):
    if isinstance(obj, dict):
        return type_.name
    return type(obj).__name__

schema = fraiseql.build_fraiseql_schema(
    types=[User],
    type_resolver=custom_type_resolver
)
```

### Schema Directives

Add custom directives:

```python
from graphql import GraphQLDirective, DirectiveLocation

# Define custom directive
cache_directive = GraphQLDirective(
    name="cache",
    locations=[DirectiveLocation.FIELD_DEFINITION],
    args={"maxAge": GraphQLArgument(GraphQLInt)}
)

schema = fraiseql.build_fraiseql_schema(
    types=[User],
    directives=[cache_directive]
)
```

### Schema Extensions

Extend existing types:

```python
@fraiseql.extend_type("User")
class UserExtensions:
    @fraiseql.field
    def computed_field(self, info) -> str:
        return "computed value"
```

## Performance Considerations

### Lazy Loading

Types are loaded lazily to improve startup time:

```python
# Types are only processed when schema is built
@fraiseql.type
class HeavyType:
    # Complex type definition
    pass
```

### Schema Caching

Cache built schemas in production:

```python
import fraiseql

# Build once, reuse across requests
SCHEMA = fraiseql.build_fraiseql_schema(
    types=[User, Product],
    queries=[users, products]
)

app = fraiseql.create_fraiseql_app(schema=SCHEMA)
```

## Troubleshooting

### Common Issues

1. **Import errors**: Ensure all types are imported before schema building
2. **Circular imports**: Use forward references with quotes: `'User'`
3. **Missing annotations**: All fields need type annotations
4. **Invalid types**: Ensure all referenced types are FraiseQL types

### Debug Mode

Enable debug logging:

```python
import logging
logging.getLogger('fraiseql.schema').setLevel(logging.DEBUG)

schema = fraiseql.build_fraiseql_schema(types=[User])
```

### Schema Validation

Validate your schema programmatically:

```python
from graphql import validate_schema

schema = fraiseql.build_fraiseql_schema(types=[User])
errors = validate_schema(schema)

if errors:
    for error in errors:
        print(f"Schema error: {error}")
```

## Best Practices

1. **Organize types in modules**: Keep related types together
2. **Use meaningful names**: Type and field names should be descriptive
3. **Add documentation**: Use descriptions for better developer experience
4. **Validate early**: Build and validate schemas during development
5. **Cache in production**: Pre-build schemas for better performance
6. **Handle errors gracefully**: Provide meaningful error messages
7. **Keep types focused**: Each type should have a single responsibility