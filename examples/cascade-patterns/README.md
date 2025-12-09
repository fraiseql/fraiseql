# CASCADE Patterns Examples

This directory contains comprehensive examples demonstrating different GraphQL CASCADE patterns in FraiseQL. Each example shows a specific use case with complete implementation.

## Examples Overview

### 1. Basic Create with Side Effects (`create_post/`)
- **Pattern**: Single entity creation with related entity updates
- **Use Case**: Creating a post that updates author's post count
- **Complexity**: Basic

### 2. Update with Multiple Entities (`update_user_profile/`)
- **Pattern**: Updating one entity that affects related entities
- **Use Case**: User profile update that invalidates cached data
- **Complexity**: Intermediate

### 3. Delete with Cleanup (`delete_comment/`)
- **Pattern**: Entity deletion with cascade cleanup
- **Use Case**: Deleting a comment that updates post stats
- **Complexity**: Intermediate

### 4. Complex Business Logic (`place_order/`)
- **Pattern**: Multi-entity transaction with inventory management
- **Use Case**: E-commerce order placement with inventory updates
- **Complexity**: Advanced

### 5. Batch Operations (`bulk_update_tags/`)
- **Pattern**: Bulk operations with selective CASCADE
- **Use Case**: Updating multiple tags with minimal cache invalidation
- **Complexity**: Advanced

## Running Examples

Each example is self-contained with its own schema and application:

```bash
# Navigate to example directory
cd examples/cascade-patterns/create_post

# Set up database
createdb cascade_create_post_example
psql -d cascade_create_post_example -f schema.sql

# Run application
python main.py
```

## Common Patterns

### Entity View Creation

All examples use consistent entity view patterns:

```sql
-- Standard entity view structure
CREATE VIEW v_entity_name AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'field1', field1,
        'field2', field2,
        -- ... all fields clients need
    ) as data
FROM tb_entity_name;
```

### CASCADE Helper Functions

Common helper functions used across examples:

```sql
-- Entity update entry
SELECT app.cascade_entity('EntityType', entity_id, 'CREATED|UPDATED|DELETED', 'v_entity_name');

-- Query invalidation
SELECT app.cascade_invalidation('queryName', 'INVALIDATE', 'PREFIX|EXACT|SUFFIX');

-- Complete cascade object
SELECT app.build_cascade(updated => entities, invalidations => queries);
```

### Success Type Definition

Consistent Success type patterns:

```python
@fraiseql.type
class MutationSuccess:
    # Entity field (matches entity_type case-insensitively)
    entity_name: EntityType

    # Standard fields
    message: str

    # CASCADE field (added automatically when enable_cascade=True)
    cascade: Cascade
```

## Testing CASCADE

Each example includes test cases demonstrating:

- **Entity Field Presence**: Verifying entity fields appear in GraphQL response
- **CASCADE Data Structure**: Validating cascade metadata format
- **Cache Updates**: Testing client-side cache application
- **Error Handling**: CASCADE behavior with failed mutations

## Performance Considerations

Examples demonstrate performance best practices:

- **Payload Size Limits**: Keeping CASCADE data under 50KB
- **Entity Selection**: Including only directly affected entities
- **Invalidation Strategies**: Using appropriate invalidation scopes
- **Monitoring**: Tracking CASCADE processing metrics

## Client Integration Examples

Each example includes client integration code for:

- **Apollo Client**: Automatic and manual cache updates
- **React Query**: Cache invalidation and updates
- **Relay**: Store updates and invalidation
- **Vanilla JavaScript**: Manual cache management

## Migration Examples

Examples show before/after patterns for migrating existing mutations to CASCADE:

```python
# Before (no CASCADE)
@fraiseql.mutation
class CreatePost:
    success: CreatePostSuccess  # Only has id and message

# After (with CASCADE)
@fraiseql.mutation(enable_cascade=True)
class CreatePost:
    success: CreatePostSuccess  # Now has post, message, and cascade fields
```

## Troubleshooting

Common issues and solutions demonstrated:

- **Missing Entity Fields**: Case sensitivity and field mapping
- **CASCADE Not Appearing**: enable_cascade flag and PostgreSQL function
- **Client Cache Issues**: Cache key matching and update logic
- **Performance Problems**: Payload size and invalidation scope optimization

## Related Documentation

- `docs/mutations/cascade-architecture.md` - Complete architecture overview
- `docs/mutations/migration-guide.md` - Step-by-step migration instructions
- `docs/guides/cascade-best-practices.md` - Usage best practices
- `docs/guides/troubleshooting.md` - Troubleshooting guide</content>
</xai:function_call<parameter name="filePath">examples/cascade-patterns/create_post/main.py
