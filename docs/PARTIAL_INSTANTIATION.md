# Partial Object Instantiation in FraiseQL

As of v0.1.0a18, FraiseQL supports partial object instantiation, allowing GraphQL queries to request only the fields they need from nested objects without errors.

## The Problem It Solves

Before v0.1.0a18, queries like this would fail:

```graphql
query GetUsers {
  users {
    id
    name
    profile {
      id
      avatar  # Only want 2 fields from profile
    }
  }
}
```

Error: `missing required argument: 'email'` (if Profile type has required email field)

## How It Works

In development mode, FraiseQL now creates partial instances of objects with only the requested fields:

### 1. Basic Partial Query

```python
@fraise_type
class Profile:
    id: UUID
    avatar: str
    email: str  # Required field
    bio: str    # Required field
    website: str | None = None

@fraise_type  
class User:
    id: UUID
    name: str
    profile: Profile

# This query now works!
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("user_view")
```

GraphQL query requesting partial fields:
```graphql
{
  users {
    id
    name
    profile {
      id
      avatar  # Only these 2 fields, not email or bio
    }
  }
}
```

### 2. Deeply Nested Partials

```python
@fraise_type
class Company:
    id: UUID
    name: str
    address: str
    employees: int

@fraise_type
class Department:
    id: UUID
    name: str
    company: Company
    budget: Decimal

@fraise_type
class Employee:
    id: UUID
    name: str
    department: Department
```

Complex nested query:
```graphql
{
  employees {
    name
    department {
      name
      company {
        name  # 3 levels deep, only name field
      }
    }
  }
}
```

## Technical Details

### How to Check if an Object is Partial

```python
from fraiseql.partial_instantiation import is_partial_instance, get_available_fields

# In a resolver or post-processing
users = await db.find("user_view")
for user in users:
    if is_partial_instance(user.profile):
        print(f"Profile is partial with fields: {get_available_fields(user.profile)}")
        # Output: Profile is partial with fields: {'id', 'avatar'}
```

### Partial Instance Attributes

Partial instances have special attributes:
- `__fraiseql_partial__`: Boolean indicating partial instantiation
- `__fraiseql_fields__`: Set of field names that were actually provided

### What Happens to Missing Fields?

Missing required fields are set to `None`:

```python
# If only id and avatar were requested:
profile.id       # ✓ Has value
profile.avatar   # ✓ Has value  
profile.email    # None (even though it's required)
profile.bio      # None (even though it's required)
profile.website  # None (was already optional)
```

## Important Notes

### 1. Development Mode Only

Partial instantiation only works in development mode:

```python
# Enable development mode
app = create_fraiseql_app(
    database_url="...",
    production=False  # or environment="development"
)
```

In production mode, raw dictionaries are returned instead of instantiated objects.

### 2. Database Views Must Have Complete Data

Your JSONB `data` column must contain all fields:

```sql
CREATE VIEW user_view AS
SELECT 
    id,
    jsonb_build_object(
        'id', id,
        'name', name,
        'profile', jsonb_build_object(
            'id', p.id,
            'avatar', p.avatar,
            'email', p.email,     -- Include all fields
            'bio', p.bio,         -- Even if not always requested
            'website', p.website
        )
    ) as data
FROM users u
JOIN profiles p ON p.user_id = u.id;
```

### 3. Type Validation

While partial instantiation bypasses constructor validation, type hints are still used:

```python
@fraise_type
class Product:
    id: UUID
    price: Decimal  # Will be converted from string/int
    created_at: datetime  # Will be parsed from ISO string
```

## Use Cases

### 1. List Views

Show minimal data in lists:

```graphql
query ProductList {
  products {
    id
    name
    price
    # Don't need description, specs, etc.
  }
}
```

### 2. Mobile APIs

Reduce bandwidth by requesting only essential fields:

```graphql
query MobileUserProfile {
  user(id: $id) {
    name
    profile {
      avatar
      # Skip bio, website, social links
    }
  }
}
```

### 3. Performance Optimization

Request only fields needed for specific views:

```graphql
query OrderSummary {
  orders {
    id
    total
    customer {
      name  # Just the name, not full customer data
    }
  }
}
```

## Best Practices

### 1. Design Types for Partial Use

Consider which fields are commonly requested together:

```python
@fraise_type
class Article:
    # Always requested together
    id: UUID
    title: str
    slug: str
    
    # Often skipped in lists
    content: str
    html_content: str
    
    # Metadata - sometimes needed
    created_at: datetime
    updated_at: datetime
    view_count: int
```

### 2. Use Field Resolvers for Expensive Fields

For computed or expensive fields, consider using field resolvers:

```python
@fraise_type
class User:
    id: UUID
    name: str
    
    @fraise_field
    async def statistics(self, info) -> UserStats:
        # Only computed when requested
        return await calculate_user_stats(self.id)
```

### 3. Document Required Fields

Make it clear which fields are always available vs. potentially None:

```python
@fraise_type
class Product:
    """Product type.
    
    Fields always available: id, name, sku
    Fields that may be None in partial queries: description, specs, images
    """
    id: UUID
    name: str
    sku: str
    description: str  # May be None in partial instances
    specs: dict[str, str]  # May be None in partial instances
```

## Troubleshooting

### Error: "Expected value of type X but got: {...}"

This means FraiseQL is returning raw dictionaries instead of instantiated types. Ensure:
1. You're in development mode
2. The view is registered with `register_type_for_view()`
3. The config is properly set

### Partial Instance Has Wrong Fields

Check that your GraphQL query selection matches what you're accessing:

```python
# If query only requested id and name
user.email  # Will be None, not an error

# To check what was requested:
if hasattr(user, '__fraiseql_fields__'):
    print(f"Available fields: {user.__fraiseql_fields__}")
```

### Performance Considerations

Partial instantiation has minimal overhead, but:
- Still fetches complete data from database
- Only saves on Python object creation
- For true performance gains, create specialized views

## Migration from Older Versions

If upgrading from <v0.1.0a18:

1. **Remove Workarounds**: Delete any code making fields optional for GraphQL
2. **Update Queries**: Nested queries that were failing will now work
3. **Test Thoroughly**: Ensure partial instances behave correctly in your app

## Summary

Partial instantiation brings FraiseQL closer to GraphQL's promise of "ask for what you need, get exactly that" while maintaining Python's type safety where possible. It's automatic, requires no code changes, and just works in development mode.