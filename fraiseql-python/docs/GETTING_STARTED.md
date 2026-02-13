# Getting Started with FraiseQL

## What is FraiseQL?

FraiseQL is a **compiled GraphQL execution engine** for databases. You define your GraphQL schema using Python decorators, and FraiseQL compiles it to optimized SQL at build time.

**Key idea**: No runtime GraphQL parsing, no N+1 queries, no magic. Just fast, predictable SQL execution.

## Architecture

```
┌──────────────────┐
│  Python Code     │
│  @fraiseql.type  │
└────────┬─────────┘
         │
         ↓ (generates)
┌──────────────────┐
│  schema.json     │
└────────┬─────────┘
         │
         ↓ (fraiseql-cli compile)
┌──────────────────────┐
│  schema.compiled.json│
│  Optimized SQL      │
└────────┬─────────────┘
         │
         ↓ (loaded by)
┌──────────────────┐
│ fraiseql-server  │
│ Execute queries  │
└──────────────────┘
```

## Your First Schema

Create a file `schema.py`:

```python
import fraiseql

# Step 1: Define types
@fraiseql.type
class User:
    """A user in the system."""
    id: int
    name: str
    email: str
    created_at: str

# Step 2: Define queries
@fraiseql.query(sql_source="v_user")
def users(limit: int = 10, offset: int = 0) -> list[User]:
    """Get all users with pagination."""
    pass

@fraiseql.query(sql_source="v_user")
def user(id: int) -> User | None:
    """Get a single user by ID."""
    pass

# Step 3: Export schema
if __name__ == "__main__":
    fraiseql.export_schema("schema.json")
```

## Running Your Schema

```bash
# Generate schema.json
python schema.py

# This outputs:
# ✅ Schema exported to schema.json
#    Types: 1
#    Queries: 2
#    Mutations: 0
```

## What Gets Generated?

Open `schema.json`:

```json
{
  "types": [
    {
      "name": "User",
      "description": "A user in the system.",
      "fields": [
        {
          "name": "id",
          "type": "Int",
          "nullable": false
        },
        {
          "name": "name",
          "type": "String",
          "nullable": false
        },
        {
          "name": "email",
          "type": "String",
          "nullable": false
        },
        {
          "name": "created_at",
          "type": "String",
          "nullable": false
        }
      ]
    }
  ],
  "queries": [
    {
      "name": "users",
      "return_type": "User",
      "returns_list": true,
      "nullable": false,
      "description": "Get all users with pagination.",
      "arguments": [
        {
          "name": "limit",
          "type": "Int",
          "nullable": false,
          "default": 10
        },
        {
          "name": "offset",
          "type": "Int",
          "nullable": false,
          "default": 0
        }
      ],
      "sql_source": "v_user"
    },
    {
      "name": "user",
      "return_type": "User",
      "returns_list": false,
      "nullable": true,
      "description": "Get a single user by ID.",
      "arguments": [
        {
          "name": "id",
          "type": "Int",
          "nullable": false
        }
      ],
      "sql_source": "v_user"
    }
  ],
  "mutations": []
}
```

## Next Steps

1. **Add Mutations**: Modify data with create/update/delete operations
2. **Map SQL Sources**: Define `v_user` view in your database
3. **Compile Schema**: Use `fraiseql-cli compile schema.json`
4. **Deploy Server**: Run `fraiseql-server --schema schema.compiled.json`

## Database Setup

You'll need to create a SQL view or function for each data source:

```sql
-- Create view for queries
CREATE VIEW v_user AS
SELECT id, name, email, created_at
FROM users
WHERE deleted_at IS NULL;

-- Create function for mutations
CREATE OR REPLACE FUNCTION fn_create_user(
    p_name text,
    p_email text
) RETURNS SETOF users AS $$
INSERT INTO users (name, email, created_at) VALUES ($1, $2, NOW())
RETURNING id, name, email, created_at;
$$ LANGUAGE SQL;
```

## Type Mapping

| Python Type | GraphQL Type | Example |
|-------------|--------------|---------|
| `int` | `Int` | `age: 25` |
| `float` | `Float` | `price: 19.99` |
| `str` | `String` | `name: "John"` |
| `bool` | `Boolean` | `active: true` |
| `list[T]` | `[T!]` | `tags: ["admin", "user"]` |
| `T \| None` | `T` (nullable) | `bio: null` |
| Custom class | Object | `user: { id: 1, name: "John" }` |

## Common Patterns

### Single Type with Multiple Queries

```python
@fraiseql.type
class Product:
    id: int
    name: str
    price: float

@fraiseql.query(sql_source="v_product")
def products() -> list[Product]:
    pass

@fraiseql.query(sql_source="v_product")
def product(id: int) -> Product | None:
    pass

@fraiseql.query(sql_source="fn_search_products")
def search_products(q: str) -> list[Product]:
    pass
```

### Mutations

```python
@fraiseql.mutation(sql_source="fn_create_product", operation="CREATE")
def create_product(name: str, price: float) -> Product:
    """Create a new product."""
    pass

@fraiseql.mutation(sql_source="fn_update_product", operation="UPDATE")
def update_product(id: int, name: str | None = None, price: float | None = None) -> Product:
    """Update an existing product."""
    pass

@fraiseql.mutation(sql_source="fn_delete_product", operation="DELETE")
def delete_product(id: int) -> bool:
    """Delete a product."""
    pass
```

## Learn More

- [Decorators Reference](DECORATORS_REFERENCE.md) - Complete API reference
- [Example Schemas](EXAMPLES.md) - Real-world examples
- [Analytics Guide](ANALYTICS_GUIDE.md) - Fact tables and aggregate queries
- [Troubleshooting](TROUBLESHOOTING.md) - Common issues and solutions

## What's NOT Included

❌ Runtime GraphQL execution (use the Rust server for that)
❌ Database drivers (provide your own views/functions)
❌ Schema validation (happens at compile time)
❌ Custom resolvers (schemas are data-driven)

## Philosophy

FraiseQL is **not** a GraphQL framework. It's a **schema compiler**:

1. **You define**: Types and queries
2. **You provide**: SQL views and functions
3. **FraiseQL optimizes**: Builds fast, deterministic queries
4. **Rust server executes**: With zero runtime overhead

This simplicity is intentional. FraiseQL scales because it has no magic.
