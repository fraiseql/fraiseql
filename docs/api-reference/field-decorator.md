# Field Decorator API Reference

The `@field` decorator is used to define GraphQL fields on your types with enhanced functionality and validation.

## Basic Usage

```python
import fraiseql

@fraiseql.type
class User:
    id: int

    @fraiseql.field
    def full_name(self, info) -> str:
        """Get the user's full name."""
        return f"{self.first_name} {self.last_name}"
```

## Parameters

### `description`

Provide a description for the GraphQL field:

```python
@fraiseql.field(description="The user's full display name")
def full_name(self, info) -> str:
    return f"{self.first_name} {self.last_name}"
```

### `resolver`

Specify a custom resolver function:

```python
def get_post_count(user, info):
    # Custom logic here
    return len(user.posts)

@fraiseql.type
class User:
    id: int

    @fraiseql.field(resolver=get_post_count)
    def post_count(self) -> int:
        pass  # Implementation provided by resolver
```

### `deprecation_reason`

Mark a field as deprecated:

```python
@fraiseql.field(deprecation_reason="Use full_name instead")
def name(self, info) -> str:
    return self.full_name
```

## Field Types

### Scalar Fields

```python
@fraiseql.type
class Product:
    @fraiseql.field
    def price(self, info) -> float:
        return self.base_price * self.tax_rate
```

### Object Fields

```python
@fraiseql.type
class User:
    @fraiseql.field
    def profile(self, info) -> 'UserProfile':
        return UserProfile(user_id=self.id)
```

### List Fields

```python
@fraiseql.type
class User:
    @fraiseql.field
    def posts(self, info) -> list['Post']:
        return Post.get_by_user_id(self.id)
```

### Optional Fields

```python
@fraiseql.type
class User:
    @fraiseql.field
    def avatar_url(self, info) -> str | None:
        return self.avatar.url if self.avatar else None
```

## Async Fields

Field resolvers can be async:

```python
@fraiseql.type
class User:
    @fraiseql.field
    async def posts(self, info) -> list['Post']:
        db = info.context["db"]
        return await db.fetch_all(
            "SELECT * FROM posts WHERE user_id = %s",
            (self.id,)
        )
```

## Context Access

Access request context in field resolvers:

```python
@fraiseql.type
class User:
    @fraiseql.field
    def can_edit(self, info) -> bool:
        current_user = info.context.get("user")
        return current_user and current_user.id == self.id
```

## Error Handling

Handle errors gracefully in field resolvers:

```python
@fraiseql.type
class User:
    @fraiseql.field
    def profile_image(self, info) -> str | None:
        try:
            return self.get_profile_image_url()
        except ImageNotFoundError:
            return None
```

## Field Arguments

Define arguments for your fields:

```python
@fraiseql.type
class User:
    @fraiseql.field
    def posts(self, info, limit: int = 10, offset: int = 0) -> list['Post']:
        return Post.get_by_user_id(self.id, limit=limit, offset=offset)
```

## Validation

Add validation to field arguments:

```python
@fraiseql.type
class User:
    @fraiseql.field
    def posts(self, info, limit: int = 10) -> list['Post']:
        if limit > 100:
            raise ValueError("Limit cannot exceed 100")
        return Post.get_by_user_id(self.id, limit=limit)
```

## Performance Considerations

### Database N+1 Prevention

Use the `@dataloader_field` decorator for fields that might cause N+1 queries:

```python
from fraiseql import dataloader_field

@fraiseql.type
class User:
    @dataloader_field
    def posts(self, info) -> list['Post']:
        return Post.get_by_user_id(self.id)
```

### Caching

Implement caching for expensive operations:

```python
from functools import lru_cache

@fraiseql.type
class User:
    @fraiseql.field
    @lru_cache(maxsize=128)
    def expensive_calculation(self, info) -> float:
        # Expensive computation here
        return complex_calculation(self.data)
```

## Best Practices

1. **Keep resolvers simple**: Field resolvers should be focused and do one thing well
2. **Use appropriate return types**: Be explicit about nullable vs non-nullable fields
3. **Handle errors gracefully**: Don't let field errors crash the entire query
4. **Consider performance**: Use dataloaders for fields that fetch related data
5. **Document your fields**: Always provide meaningful descriptions

## Common Patterns

### Computed Fields

```python
@fraiseql.type
class Order:
    @fraiseql.field
    def total(self, info) -> float:
        return sum(item.price * item.quantity for item in self.items)
```

### Formatted Fields

```python
@fraiseql.type
class User:
    @fraiseql.field
    def created_at_formatted(self, info) -> str:
        return self.created_at.strftime("%Y-%m-%d %H:%M:%S")
```

### Permission-Based Fields

```python
@fraiseql.type
class User:
    @fraiseql.field
    def email(self, info) -> str | None:
        current_user = info.context.get("user")
        if current_user and (current_user.id == self.id or current_user.is_admin):
            return self.email
        return None
```

## Migration from Other Libraries

### From Strawberry

Strawberry's `@strawberry.field` maps directly to `@fraiseql.field`:

```python
# Strawberry
@strawberry.field
def full_name(self) -> str:
    return f"{self.first_name} {self.last_name}"

# FraiseQL
@fraiseql.field
def full_name(self, info) -> str:
    return f"{self.first_name} {self.last_name}"
```

Note that FraiseQL field resolvers receive an `info` parameter containing request context.
