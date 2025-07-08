# Returning Full Objects from Mutations

This guide explains how to return complete objects from GraphQL mutations in FraiseQL, following GraphQL best practices where mutations return the mutated entity.

## Overview

FraiseQL fully supports returning complete objects from mutations through its `object_data` field. When your PostgreSQL function returns entity data in the `object_data` JSONB field, FraiseQL automatically:

- Parses the JSONB into typed Python objects
- Handles nested relationships
- Supports lists of objects
- Maintains type safety

## Quick Example

Instead of returning just an ID:

```python
# ❌ Anti-pattern: Returning only ID
@fraiseql.success
class CreateLocationSuccess:
    location_id: str
    message: str
```

Return the full object:

```python
# ✅ Best practice: Return full object
@fraiseql.success
class CreateLocationSuccess:
    location: Location  # Full object with all fields
    message: str
```

## How It Works

### 1. SQL Function Returns Complete Data

Your PostgreSQL function should populate the `object_data` field with the complete entity:

```sql
CREATE OR REPLACE FUNCTION create_location_with_log(input_data JSONB)
RETURNS app.mutation_result AS $$
DECLARE
    v_result app.mutation_result;
    v_location_id UUID;
BEGIN
    -- Create the location
    INSERT INTO locations (name, identifier, address_id)
    VALUES (
        input_data->>'name',
        input_data->>'identifier',
        (input_data->>'address_id')::UUID
    )
    RETURNING id INTO v_location_id;
    
    -- Refresh materialized data
    CALL refresh_location();
    
    -- Get complete location data from view
    SELECT data INTO v_result.object_data
    FROM tv_location  -- Table view with all relationships
    WHERE id = v_location_id;
    
    -- Set other result fields
    v_result.id := v_location_id;
    v_result.status := 'success';
    v_result.message := 'Location created successfully';
    
    RETURN v_result;
END;
$$ LANGUAGE plpgsql;
```

The `tv_location` view returns complete data including relationships:

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "name": "Building A",
  "identifier": "BLDG-A",
  "address": {
    "id": "650e8400-e29b-41d4-a716-446655440000",
    "street_name": "123 Main St",
    "city": "New York",
    "postal_code": "10001"
  },
  "parent": {
    "id": "450e8400-e29b-41d4-a716-446655440000",
    "name": "Main Campus"
  }
}
```

### 2. FraiseQL Automatically Parses the Data

Define your mutation types with full objects:

```python
@fraiseql.type
class Address:
    id: str
    street_name: str
    city: str
    postal_code: str

@fraiseql.type
class Location:
    id: str
    name: str
    identifier: str
    address: Address | None = None
    parent: Location | None = None

@fraiseql.success
class CreateLocationSuccess:
    message: str
    location: Location  # FraiseQL maps object_data to this field

@fraiseql.failure
class CreateLocationError:
    message: str
    conflict_location: Location | None = None  # For duplicates

@fraiseql.mutation
class CreateLocation:
    """Create a new location."""
    input: CreateLocationInput
    success: CreateLocationSuccess
    error: CreateLocationError
```

### 3. Automatic Field Mapping

FraiseQL's parser automatically:
- Maps `object_data` to the appropriate field in your success type
- Instantiates nested objects using their type definitions
- Handles optional fields and relationships

> **Note (v0.1.0b7+)**: Object mapping works correctly regardless of which standard fields (`message`, `status`) your success type includes. The parser intelligently identifies entity fields and maps `object_data` appropriately.

## Common Patterns

### Single Object Creation

```python
@fraiseql.success
class CreateUserSuccess:
    user: User  # Complete user object
    message: str

# SQL: Return user data in object_data
result.object_data := (SELECT data FROM v_users WHERE id = new_user_id);
```

### Bulk Operations

```python
@fraiseql.success
class BulkCreateUsersSuccess:
    users: list[User]  # List of created users
    message: str
    failed_count: int

# SQL: Return array of users in object_data
result.object_data := (
    SELECT json_agg(data) 
    FROM v_users 
    WHERE id = ANY(created_user_ids)
);
```

### Update Operations

```python
@fraiseql.success
class UpdateOrderSuccess:
    order: Order  # Updated order with all fields
    previous_status: str  # From extra_metadata
    message: str

# SQL: Return updated data
result.object_data := (SELECT data FROM v_orders WHERE id = order_id);
result.extra_metadata := jsonb_build_object(
    'previous_status', old_status
);
```

### Error Responses with Context

```python
@fraiseql.failure
class CreateProductError:
    message: str
    duplicate_product: Product | None = None  # Existing product
    suggestions: list[str] | None = None

# SQL: Include conflict data in extra_metadata
IF product_exists THEN
    result.status := 'duplicate';
    result.extra_metadata := jsonb_build_object(
        'duplicate_product', (
            SELECT data FROM v_products 
            WHERE sku = input_data->>'sku'
        ),
        'suggestions', ARRAY['Add variant', 'Update existing']
    );
END IF;
```

## Benefits

1. **Reduced Network Calls**: Clients get all data in one request
2. **Cache Updates**: Clients can update their caches immediately
3. **Better DX**: Follows GraphQL best practices
4. **No Additional Queries**: Data is already fetched by SQL function

## Migration Guide

To update existing mutations that only return IDs:

### Step 1: Update Success Types

```python
# Before
@fraiseql.success
class CreateLocationSuccess:
    location_id: str
    message: str

# After
@fraiseql.success
class CreateLocationSuccess:
    location: Location
    message: str
    location_id: str | None = None  # Deprecated, keep for compatibility
```

### Step 2: Ensure SQL Functions Return Full Data

Most SQL functions already return complete data in `object_data`. Verify your function:

```sql
-- Check what your function returns
SELECT object_data FROM your_mutation_function('{}'::jsonb);
```

### Step 3: Test the Migration

```graphql
mutation {
  createLocation(input: { name: "Test", identifier: "TEST" }) {
    ... on CreateLocationSuccess {
      location {  # Now available!
        id
        name
        identifier
        address {
          city
          postalCode
        }
      }
      locationId  # Still works during migration
    }
  }
}
```

### Step 4: Update Clients

Once clients are updated to use the full objects, remove the deprecated ID fields.

## Performance Considerations

- **No Extra Queries**: Data comes from the same SQL function call
- **View Performance**: Use materialized views (`tv_*` tables) for complex joins
- **Field Selection**: FraiseQL respects GraphQL field selection
- **Partial Loading**: Use FraiseQL's partial instantiation for large objects

## Best Practices

1. **Always Use Views**: Return data from views, not raw tables
   ```sql
   -- Good: Includes all relationships
   SELECT data FROM tv_users WHERE id = user_id;
   
   -- Bad: Missing relationships
   SELECT row_to_json(u) FROM users u WHERE id = user_id;
   ```

2. **Handle Missing Data**: Make related objects optional
   ```python
   @fraiseql.type
   class Order:
       id: str
       customer: Customer | None = None  # Handle deleted customers
   ```

3. **Consistent Naming**: Use singular names for single objects
   ```python
   # Good
   user: User
   order: Order
   
   # Bad
   users: User  # Confusing
   order_data: Order  # Redundant
   ```

4. **Include Metadata**: Use `extra_metadata` for additional context
   ```sql
   result.extra_metadata := jsonb_build_object(
       'cache_key', 'user:' || user_id,
       'ttl', 3600,
       'version', 2
   );
   ```

## Troubleshooting

### Object Not Populated

If your object field is `null`:

1. Check the SQL function returns data in `object_data`
2. Verify field names match between SQL and Python types
3. Ensure the status indicates success

### Type Instantiation Errors

If you get type errors:

1. Verify all required fields are present in the JSONB
2. Check that nested types are properly decorated with `@fraiseql.type`
3. Use `from_dict` classmethod for custom instantiation

### Performance Issues

If mutations are slow:

1. Check your view performance with `EXPLAIN ANALYZE`
2. Consider using materialized views
3. Ensure proper indexes exist

## See Also

- [PostgreSQL Function-Based Mutations](./postgresql-function-based.md)
- [Type System](../type-system.md)
- [Partial Instantiation](../PARTIAL_INSTANTIATION.md)