# FraiseQL Partial Updates Guide

This guide explains how to properly implement partial updates in FraiseQL when working with PostgreSQL composite types and JSONB data.

## Using UNSET for Optional Fields (Recommended)

Starting with FraiseQL v0.1.0b23, the recommended approach for partial updates is to use the `UNSET` sentinel value for optional fields in your input types:

```python
from fraiseql import fraise_input
from fraiseql.types.definitions import UNSET

@fraise_input
class UpdateRouterInput:
    id: str
    hostname: str | None = UNSET  # Use UNSET, not None
    ip_address: str | None = UNSET
    mac_address: str | None = UNSET
```

When you use UNSET:
- Fields not provided in the GraphQL mutation are excluded from the JSONB payload
- Only explicitly provided fields (including those set to null) are sent to PostgreSQL
- This enables true partial updates without NULL constraint violations

### Example: Partial Update with UNSET

```graphql
# GraphQL mutation - only updating IP address
mutation {
  updateRouter(input: {
    id: "123",
    ipAddress: "192.168.1.100"
  }) {
    router { id hostname ipAddress }
  }
}

# PostgreSQL receives JSONB:
{
  "id": "123",
  "ip_address": "192.168.1.100"
}
# Note: hostname and mac_address are NOT in the JSONB at all
```

## The Challenge Without UNSET

When implementing partial updates with composite types, a common pitfall is using `jsonb_populate_record` which sets all unspecified fields to NULL:

```sql
-- ❌ INCORRECT: This will set unspecified fields to NULL
v_input := jsonb_populate_record(NULL::app.type_router_input, input_payload);
```

## Field Name Transformations

FraiseQL automatically handles field name transformations between GraphQL and PostgreSQL:

- **GraphQL**: Uses camelCase when `camel_case_fields=True` (e.g., `ipAddress`, `firstName`)
- **PostgreSQL**: Always receives snake_case (e.g., `ip_address`, `first_name`)
- **Automatic Transformation**: FraiseQL converts between formats automatically

### Important: PostgreSQL Always Gets Snake Case

Regardless of your GraphQL schema configuration, PostgreSQL functions always receive snake_case field names:

```python
# GraphQL mutation with camelCase
mutation {
  updateRouter(input: {
    id: "123",
    ipAddress: "192.168.1.100"
  })
}

# PostgreSQL receives snake_case:
{
  "id": "123", 
  "ip_address": "192.168.1.100"  # Automatically converted!
}
```

This means your PostgreSQL functions should always use snake_case when checking field existence:

```sql
-- ✅ CORRECT: Always use snake_case in PostgreSQL
IF p_input ? 'ip_address' THEN
    UPDATE routers SET ip_address = p_input->>'ip_address';
END IF;

-- ❌ INCORRECT: Don't use camelCase in PostgreSQL
IF p_input ? 'ipAddress' THEN  -- This will always be FALSE!
```

## Recommended Patterns for Partial Updates

### Pattern 1: Check JSONB Field Presence (Recommended with UNSET)

When using UNSET in your input types, check for field presence in the JSONB input before updating:

```sql
CREATE OR REPLACE FUNCTION app.update_router(
    p_input JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_result app.mutation_result;
    v_id UUID;
    v_updated_fields TEXT[] := '{}';
BEGIN
    -- Extract the ID
    v_id := (p_input->>'id')::UUID;
    
    -- Update only fields that are present in the input
    -- Note: Always use snake_case for JSONB field checks
    IF p_input ? 'ip_address' THEN
        UPDATE tenant.tb_router 
        SET ip_address = p_input->>'ip_address'
        WHERE id = v_id;
        v_updated_fields := array_append(v_updated_fields, 'ip_address');
    END IF;
    
    IF p_input ? 'hostname' THEN
        UPDATE tenant.tb_router 
        SET hostname = p_input->>'hostname'
        WHERE id = v_id;
        v_updated_fields := array_append(v_updated_fields, 'hostname');
    END IF;
    
    IF p_input ? 'mac_address' THEN
        UPDATE tenant.tb_router 
        SET mac_address = p_input->>'mac_address'
        WHERE id = v_id;
        v_updated_fields := array_append(v_updated_fields, 'mac_address');
    END IF;
    
    -- Build the result with full object data
    SELECT INTO v_result.object_data
        jsonb_build_object(
            'id', id,
            'hostname', hostname,
            'ip_address', ip_address,
            'mac_address', mac_address
        )
    FROM tenant.tb_router
    WHERE id = v_id;
    
    v_result.id := v_id;
    v_result.status := 'success';
    v_result.message := 'Router updated successfully';
    v_result.updated_fields := v_updated_fields;
    
    RETURN v_result;
END;
$$ LANGUAGE plpgsql;
```

### Pattern 2: Dynamic UPDATE with CASE Statements

Build a single UPDATE statement dynamically:

```sql
CREATE OR REPLACE FUNCTION app.update_router_dynamic(
    p_input JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_result app.mutation_result;
    v_id UUID;
BEGIN
    v_id := (p_input->>'id')::UUID;
    
    -- Single UPDATE with conditional assignments
    UPDATE tenant.tb_router
    SET 
        ip_address = CASE 
            WHEN p_input ? 'ip_address' THEN p_input->>'ip_address'
            ELSE ip_address 
        END,
        hostname = CASE 
            WHEN p_input ? 'hostname' THEN p_input->>'hostname'
            ELSE hostname 
        END,
        mac_address = CASE 
            WHEN p_input ? 'mac_address' THEN p_input->>'mac_address'
            ELSE mac_address 
        END,
        updated_at = CURRENT_TIMESTAMP
    WHERE id = v_id;
    
    -- Rest of function...
END;
$$ LANGUAGE plpgsql;
```

### Pattern 3: Simple NULL Check for Explicit Updates

When you need to support explicit NULL updates (to clear a field), add a NULL check:

```sql
-- Check if field exists AND is not null (for non-nullable fields)
IF p_input ? 'hostname' AND p_input->>'hostname' IS NOT NULL THEN
    UPDATE tenant.tb_router 
    SET hostname = p_input->>'hostname'
    WHERE id = v_id;
END IF;

-- For nullable fields, allow NULL updates
IF p_input ? 'location' THEN
    UPDATE tenant.tb_router 
    SET location = p_input->>'location'  -- Can be NULL
    WHERE id = v_id;
END IF;
```

### Pattern 4: Composite Type Partial Updates

For updating composite types partially:

```sql
CREATE TYPE app.address_type AS (
    street TEXT,
    city TEXT,
    state TEXT,
    zip_code TEXT
);

-- Update only specified address fields
IF p_input ? 'address' THEN
    UPDATE users
    SET address = ROW(
        COALESCE(
            p_input->'address'->>'street',
            (address).street
        ),
        COALESCE(
            p_input->'address'->>'city',
            (address).city
        ),
        COALESCE(
            p_input->'address'->>'state',
            (address).state
        ),
        COALESCE(
            p_input->'address'->>'zip_code',
            (address).zip_code
        )
    )::app.address_type
    WHERE id = v_user_id;
END IF;
```

## Best Practices

1. **Always check field presence**: Use the `?` operator to check if a field exists in the JSONB input
2. **Use snake_case for JSONB checks**: FraiseQL always sends snake_case field names to PostgreSQL
3. **Use UNSET for optional fields**: This excludes unprovided fields from the JSONB entirely
4. **Track updated fields**: Maintain an array of updated field names for the mutation result
5. **Handle NULLs explicitly**: Distinguish between "field not provided" and "field set to NULL"
6. **Return full object data**: Always return the complete updated object in `object_data`

## Common Pitfalls to Avoid

### ❌ Don't use jsonb_populate_record for partial updates
```sql
-- This sets all unspecified fields to NULL
v_input := jsonb_populate_record(NULL::app.type_input, p_input);
```

### ❌ Don't use default values of None for optional fields
```python
# Wrong: Using None as default
@fraise_input
class UpdateInput:
    name: str | None = None  # Will be sent as null!
    
# Correct: Using UNSET
@fraise_input  
class UpdateInput:
    name: str | None = UNSET  # Excluded if not provided
```

### ❌ Don't update fields that weren't provided
```sql
-- Wrong: Updates all fields even if not provided
UPDATE table SET 
    field1 = COALESCE(p_input->>'field1', field1),  -- Still triggers update
    field2 = COALESCE(p_input->>'field2', field2)
WHERE id = v_id;
```

## Complete Example: Router Update

Here's a complete example implementing partial updates for a router entity:

```python
# Python GraphQL definitions
@fraiseql.input
class UpdateRouterInput:
    id: UUID
    hostname: str | None = None
    ip_address: str | None = None
    mac_address: str | None = None
    location: str | None = None

@fraiseql.type
class Router:
    id: UUID
    hostname: str
    ip_address: str
    mac_address: str
    location: str | None
    created_at: datetime
    updated_at: datetime
```

```sql
-- PostgreSQL function
CREATE OR REPLACE FUNCTION app.update_router(
    p_input JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_result app.mutation_result;
    v_id UUID;
    v_updated_fields TEXT[] := '{}';
    v_update_count INT := 0;
BEGIN
    -- Validate input
    IF NOT p_input ? 'id' THEN
        v_result.status := 'error';
        v_result.message := 'Router ID is required';
        RETURN v_result;
    END IF;
    
    v_id := (p_input->>'id')::UUID;
    
    -- Check if router exists
    IF NOT EXISTS (SELECT 1 FROM tenant.tb_router WHERE id = v_id) THEN
        v_result.status := 'error';
        v_result.message := 'Router not found';
        RETURN v_result;
    END IF;
    
    -- Update only provided fields
    IF p_input ? 'hostname' THEN
        UPDATE tenant.tb_router 
        SET hostname = p_input->>'hostname',
            updated_at = CURRENT_TIMESTAMP
        WHERE id = v_id;
        v_updated_fields := array_append(v_updated_fields, 'hostname');
        v_update_count := v_update_count + 1;
    END IF;
    
    IF p_input ? 'ip_address' THEN
        UPDATE tenant.tb_router 
        SET ip_address = p_input->>'ip_address',
            updated_at = CURRENT_TIMESTAMP
        WHERE id = v_id;
        v_updated_fields := array_append(v_updated_fields, 'ip_address');
        v_update_count := v_update_count + 1;
    END IF;
    
    IF p_input ? 'mac_address' THEN
        UPDATE tenant.tb_router 
        SET mac_address = p_input->>'mac_address',
            updated_at = CURRENT_TIMESTAMP
        WHERE id = v_id;
        v_updated_fields := array_append(v_updated_fields, 'mac_address');
        v_update_count := v_update_count + 1;
    END IF;
    
    IF p_input ? 'location' THEN
        UPDATE tenant.tb_router 
        SET location = NULLIF(p_input->>'location', ''),
            updated_at = CURRENT_TIMESTAMP
        WHERE id = v_id;
        v_updated_fields := array_append(v_updated_fields, 'location');
        v_update_count := v_update_count + 1;
    END IF;
    
    -- Build result
    SELECT INTO v_result.object_data
        jsonb_build_object(
            'id', id,
            'hostname', hostname,
            'ip_address', ip_address,
            'mac_address', mac_address,
            'location', location,
            'created_at', created_at,
            'updated_at', updated_at
        )
    FROM tenant.tb_router
    WHERE id = v_id;
    
    v_result.id := v_id;
    v_result.status := 'success';
    v_result.message := format('Router updated successfully (%s fields)', v_update_count);
    v_result.updated_fields := v_updated_fields;
    v_result.extra_metadata := jsonb_build_object(
        'entity', 'router',
        'operation', 'update',
        'fields_updated', v_update_count
    );
    
    RETURN v_result;
EXCEPTION
    WHEN OTHERS THEN
        v_result.status := 'error';
        v_result.message := format('Update failed: %s', SQLERRM);
        RETURN v_result;
END;
$$ LANGUAGE plpgsql;
```

## Testing Your Implementation

Always test partial updates thoroughly:

```python
# Test 1: Update single field
result = await graphql_execute("""
    mutation {
        updateRouter(input: {
            id: "123e4567-e89b-12d3-a456-426614174000",
            ipAddress: "192.168.1.100"
        }) {
            ... on UpdateRouterSuccess {
                router {
                    id
                    hostname  # Should remain unchanged
                    ipAddress  # Should be updated
                    macAddress  # Should remain unchanged
                }
                updatedFields
            }
        }
    }
""")

# Test 2: Update multiple fields
result = await graphql_execute("""
    mutation {
        updateRouter(input: {
            id: "123e4567-e89b-12d3-a456-426614174000",
            hostname: "router-02",
            location: "Server Room B"
        }) {
            ... on UpdateRouterSuccess {
                router { hostname location ipAddress }
                updatedFields  # Should be ["hostname", "location"]
            }
        }
    }
""")

# Test 3: Update with NULL
result = await graphql_execute("""
    mutation {
        updateRouter(input: {
            id: "123e4567-e89b-12d3-a456-426614174000",
            location: null  # Explicitly set to NULL
        }) {
            ... on UpdateRouterSuccess {
                router { location }  # Should be NULL
            }
        }
    }
""")
```

## Summary

The key to successful partial updates in FraiseQL is understanding:
1. Use `UNSET` as the default value for optional fields in input types
2. FraiseQL always sends snake_case field names to PostgreSQL
3. Only fields explicitly provided in the mutation are included in the JSONB
4. Check field presence before updating using the `?` operator
5. Don't use `jsonb_populate_record` for partial updates
6. Track which fields were actually updated
7. Always return the full updated object

By following these patterns, you can implement robust partial updates that work correctly with NOT NULL constraints and provide a great GraphQL API experience.