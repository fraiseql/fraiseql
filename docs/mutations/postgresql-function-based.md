# PostgreSQL Function-Based Mutations

FraiseQL implements a sophisticated mutation system where PostgreSQL functions handle all business logic and return standardized results that are automatically parsed into typed GraphQL responses.

## Overview

Instead of writing mutation resolvers in Python, you:
1. Write PostgreSQL functions that handle business logic
2. Define mutation types with `@fraiseql.mutation`
3. FraiseQL automatically generates resolvers that call your functions

## Architecture

### PostgreSQL Side

All mutation functions should be placed in a dedicated schema (e.g., `graphql`) and return a standardized result type:

```sql
-- Create the standard mutation result type
CREATE TYPE mutation_result AS (
    id UUID,                    -- ID of affected entity (optional)
    updated_fields TEXT[],      -- Which fields were modified (optional)
    status TEXT,                -- 'success' or error status
    message TEXT,               -- Human-readable message
    object_data JSONB,          -- Main entity data
    extra_metadata JSONB        -- Additional data/fields
);

-- Example mutation function
CREATE OR REPLACE FUNCTION graphql.create_user(input_data JSONB)
RETURNS mutation_result AS $$
DECLARE
    result mutation_result;
    new_user_id UUID;
BEGIN
    -- Validation
    IF NOT (input_data ? 'email' AND input_data ? 'name') THEN
        result.status := 'validation_error';
        result.message := 'Email and name are required';
        RETURN result;
    END IF;

    -- Check for existing user
    IF EXISTS (SELECT 1 FROM users WHERE email = input_data->>'email') THEN
        result.status := 'email_exists';
        result.message := 'This email is already registered';
        result.extra_metadata := jsonb_build_object(
            'conflict_user', (
                SELECT row_to_json(u) FROM v_users u
                WHERE email = input_data->>'email'
            ),
            'suggested_email', input_data->>'email' || '2'
        );
        RETURN result;
    END IF;

    -- Create user
    INSERT INTO users (email, name, bio)
    VALUES (
        input_data->>'email',
        input_data->>'name',
        input_data->>'bio'
    )
    RETURNING id INTO new_user_id;

    -- Return success with user data
    result.id := new_user_id;
    result.status := 'success';
    result.message := 'User created successfully';
    result.updated_fields := ARRAY['email', 'name', 'bio'];

    -- Get full user data from view
    SELECT data INTO result.object_data
    FROM v_users
    WHERE id = new_user_id;

    result.extra_metadata := jsonb_build_object(
        'entity', 'user',
        'welcome_email_sent', true
    );

    RETURN result;
END;
$$ LANGUAGE plpgsql;
```

### Python Side

Define your mutations using the `@fraiseql.mutation` decorator:

```python
from fraiseql import mutation, success, failure, fraise_field
from typing import Optional

# Define input type
@fraiseql.input
class CreateUserInput:
    email: str
    name: str
    bio: Optional[str] = None

# Define success type
@fraiseql.success
class CreateUserSuccess:
    message: str
    user: User  # Will be instantiated from object_data

# Define error type
@fraiseql.failure
class CreateUserError:
    message: str
    conflict_user: Optional[User] = None  # From extra_metadata
    suggested_email: Optional[str] = None  # From extra_metadata

# Define mutation
@fraiseql.mutation
class CreateUser:
    """Create a new user account."""
    input: CreateUserInput
    success: CreateUserSuccess
    error: CreateUserError
```

The `@mutation` decorator:
- Uses naming convention: `CreateUser` → `graphql.create_user`
- Generates a resolver that calls the PostgreSQL function
- Parses the result into Success or Error types
- Handles complex object instantiation

## Complex Types

### Returning Multiple Objects

```python
@fraiseql.success
class BulkUpdateSuccess:
    message: str
    affected_orders: list[Order]      # From object_data
    skipped_orders: list[Order]       # From extra_metadata
    processing_time_ms: float         # From extra_metadata

@fraiseql.mutation
class BulkUpdateOrders:
    input: BulkUpdateInput
    success: BulkUpdateSuccess
    error: BulkUpdateError
```

PostgreSQL function:
```sql
-- Return multiple orders
result.status := 'success';
result.message := '15 orders updated, 3 skipped';

-- Main data in object_data
result.object_data := (
    SELECT json_agg(row_to_json(o))
    FROM v_orders o
    WHERE id = ANY(updated_order_ids)
);

-- Additional data in metadata
result.extra_metadata := jsonb_build_object(
    'entity', 'bulk_update',  -- Helps parser identify main field
    'skipped_orders', (
        SELECT json_agg(row_to_json(o))
        FROM v_orders o
        WHERE id = ANY(skipped_order_ids)
    ),
    'processing_time_ms', 234.5
);
```

### Error with Conflict Data

```python
@fraiseql.failure
class UpdatePostError:
    message: str
    current_post: Optional[Post] = None     # Current state
    suggested_changes: Optional[dict] = None # Suggestions
```

## How It Works

1. **Function Discovery**: The mutation name determines the PostgreSQL function
   - `CreateUser` → `graphql.create_user`
   - `UpdateUserProfile` → `graphql.update_user_profile`

2. **Result Parsing**: Based on the `status` field:
   - `status = 'success'` → Returns Success type
   - `status` starting with `error`, `failed`, `not_found` → Returns Error type

3. **Object Instantiation**:
   - Simple fields come from the result directly
   - Complex types (with `@fraise_type`) are instantiated using `from_dict()`
   - Lists of complex types are handled recursively
   - Nested objects are fully instantiated

4. **Field Mapping**:
   - `object_data` typically contains the main entity
   - `extra_metadata` contains additional fields
   - The parser maps these to your Success/Error type fields

## Returning Full Objects

FraiseQL fully supports returning complete objects from mutations. When your SQL function populates the `object_data` field with entity data, FraiseQL automatically instantiates the corresponding typed objects.

### Why Return Full Objects?

- **Better DX**: Clients get all data in one request
- **Cache Updates**: Apollo/Relay can update caches immediately
- **GraphQL Best Practices**: Follows patterns used by GitHub, Shopify APIs
- **No Extra Queries**: Data is already fetched by your SQL function

### Quick Example

```python
# Instead of returning just an ID
@fraiseql.success
class CreateUserSuccess:
    user_id: str  # ❌ Requires another query

# Return the full object
@fraiseql.success
class CreateUserSuccess:
    user: User  # ✅ Complete object with all fields
```

Your SQL function already returns the data:
```sql
-- The object_data field contains the complete entity
SELECT data INTO result.object_data FROM v_users WHERE id = new_user_id;
```

For more details, see the [Returning Full Objects](./returning-full-objects.md) guide.

## Best Practices

### 1. Status Conventions

Use descriptive status values that indicate the outcome:
- Success: `'success'`
- Validation errors: `'validation_error'`, `'invalid_input'`
- Not found: `'not_found'`, `'entity_not_found'`
- Conflicts: `'conflict'`, `'email_exists'`, `'duplicate_name'`
- Business errors: `'insufficient_funds'`, `'quota_exceeded'`

### 2. Entity Hints

When returning multiple entities, use the `entity` field in metadata:
```sql
result.extra_metadata := jsonb_build_object(
    'entity', 'order',  -- Tells parser which field maps to object_data
    'other_field', other_data
);
```

### 3. Consistent View Usage

Always return data from views (not tables) for consistency:
```sql
-- Good: Returns denormalized data from view
SELECT data INTO result.object_data FROM v_users WHERE id = new_id;

-- Bad: Returns raw table data
SELECT row_to_json(u) INTO result.object_data FROM users u WHERE id = new_id;
```

### 4. Error Details

Provide helpful error information:
```sql
result.status := 'validation_error';
result.message := 'Invalid input data';
result.extra_metadata := jsonb_build_object(
    'field_errors', jsonb_build_object(
        'email', 'Invalid email format',
        'age', 'Must be 18 or older'
    ),
    'submitted_data', input_data  -- Help with debugging
);
```

## Complete Example

Here's a full example showing a complex mutation:

```python
# types.py
@fraiseql.type
class Order:
    id: str
    order_number: str
    total_amount: float
    status: OrderStatus
    customer: Customer
    items: list[OrderItem]

@fraiseql.input
class TransferOrdersInput:
    order_ids: list[str]
    from_warehouse_id: str
    to_warehouse_id: str
    transfer_date: datetime

@fraiseql.success
class TransferOrdersSuccess:
    message: str
    transferred_orders: list[Order]
    from_warehouse: Warehouse
    to_warehouse: Warehouse
    transfer_receipt: TransferReceipt

@fraiseql.failure
class TransferOrdersError:
    message: str
    failed_orders: list[Order] = None
    reason: str = None

@fraiseql.mutation
class TransferOrders:
    input: TransferOrdersInput
    success: TransferOrdersSuccess
    error: TransferOrdersError
```

```sql
-- PostgreSQL function
CREATE OR REPLACE FUNCTION graphql.transfer_orders(input_data JSONB)
RETURNS mutation_result AS $$
DECLARE
    result mutation_result;
    transferred_ids UUID[];
BEGIN
    -- Complex business logic here...

    -- Success case
    result.status := 'success';
    result.message := format('Successfully transferred %s orders', array_length(transferred_ids, 1));

    -- Return transferred orders
    result.object_data := (
        SELECT json_agg(data)
        FROM v_orders
        WHERE id = ANY(transferred_ids)
    );

    -- Additional data
    result.extra_metadata := jsonb_build_object(
        'entity', 'transferred_orders',
        'from_warehouse', (SELECT data FROM v_warehouses WHERE id = (input_data->>'from_warehouse_id')::uuid),
        'to_warehouse', (SELECT data FROM v_warehouses WHERE id = (input_data->>'to_warehouse_id')::uuid),
        'transfer_receipt', jsonb_build_object(
            'receipt_number', 'TR-' || to_char(now(), 'YYYYMMDD-HH24MISS'),
            'transfer_date', input_data->>'transfer_date',
            'item_count', array_length(transferred_ids, 1)
        )
    );

    RETURN result;
END;
$$ LANGUAGE plpgsql;
```

## Migration from Manual Mutations

If you're migrating from manual mutation resolvers:

1. Move business logic to PostgreSQL functions
2. Ensure functions return the standard `mutation_result` type
3. Replace resolver functions with `@mutation` classes
4. Test that object instantiation works correctly

The benefit is that all your business logic lives in the database where it can be:
- Tested independently
- Optimized by PostgreSQL
- Transactionally safe
- Reused by other systems
