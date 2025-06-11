# Migrating to PostgreSQL Function-Based Mutations

This guide helps you migrate from manual mutation resolvers to FraiseQL's PostgreSQL function-based mutation system.

## Overview of Changes

### Before: Manual Python Resolvers
```python
async def create_user(info, input: CreateUserInput) -> CreateUserSuccess | CreateUserError:
    db = info.context["db"]

    # Check if email exists
    existing = await db.get_user_by_email(input.email)
    if existing:
        return CreateUserError(
            message="Email already registered",
            code="EMAIL_EXISTS"
        )

    # Hash password
    password_hash = hash_password(input.password)

    # Create user
    result = await db.create_user({
        "email": input.email,
        "name": input.name,
        "password_hash": password_hash
    })

    if not result["success"]:
        return CreateUserError(message=result["error"])

    # Fetch created user
    user_data = await db.get_user_by_id(result["user_id"])
    user = User.from_dict(user_data)

    return CreateUserSuccess(user=user)
```

### After: PostgreSQL Function + Simple Types
```python
@fraiseql.mutation
class CreateUser:
    input: CreateUserInput
    success: CreateUserSuccess
    error: CreateUserError
```

## Migration Steps

### 1. Create the Standard Result Type

First, create the standardized result type in your database:

```sql
-- Run this once in your database
CREATE TYPE mutation_result AS (
    id UUID,
    updated_fields TEXT[],
    status TEXT,
    message TEXT,
    object_data JSONB,
    extra_metadata JSONB
);
```

### 2. Move Business Logic to PostgreSQL

Convert your Python resolver logic to a PostgreSQL function:

```sql
CREATE OR REPLACE FUNCTION graphql.create_user(input_data JSONB)
RETURNS mutation_result AS $$
DECLARE
    result mutation_result;
    new_user_id UUID;
    password_hash TEXT;
BEGIN
    -- Validation
    IF NOT (input_data ? 'email' AND input_data ? 'name') THEN
        result.status := 'validation_error';
        result.message := 'Email and name are required';
        RETURN result;
    END IF;

    -- Check existing email
    IF EXISTS (SELECT 1 FROM users WHERE email = input_data->>'email') THEN
        result.status := 'email_exists';
        result.message := 'Email already registered';

        -- Include the conflicting user if needed
        result.extra_metadata := jsonb_build_object(
            'conflict_user', (
                SELECT data FROM v_users
                WHERE email = input_data->>'email'
            )
        );
        RETURN result;
    END IF;

    -- Hash password (or call a function)
    password_hash := crypt(input_data->>'password', gen_salt('bf'));

    -- Create user
    INSERT INTO users (email, name, password_hash)
    VALUES (
        input_data->>'email',
        input_data->>'name',
        password_hash
    )
    RETURNING id INTO new_user_id;

    -- Return success
    result.id := new_user_id;
    result.status := 'success';
    result.message := 'User created successfully';
    result.updated_fields := ARRAY['email', 'name', 'password_hash'];

    -- Get full user data from view
    SELECT data INTO result.object_data
    FROM v_users
    WHERE id = new_user_id;

    result.extra_metadata := jsonb_build_object(
        'entity', 'user',
        'welcome_email_queued', true
    );

    RETURN result;

EXCEPTION
    WHEN OTHERS THEN
        result.status := 'error';
        result.message := SQLERRM;
        RETURN result;
END;
$$ LANGUAGE plpgsql;
```

### 3. Update Your Python Types

Replace your resolver function with type definitions:

```python
# Keep your existing input type
@fraiseql.input
class CreateUserInput:
    email: str
    name: str
    password: str

# Update success type if needed
@fraiseql.success
class CreateUserSuccess:
    message: str
    user: User  # Will be auto-instantiated

# Update error type to handle richer responses
@fraiseql.failure
class CreateUserError:
    message: str
    conflict_user: Optional[User] = None  # From extra_metadata

# Define the mutation
@fraiseql.mutation
class CreateUser:
    input: CreateUserInput
    success: CreateUserSuccess
    error: CreateUserError
```

### 4. Update Your Schema Registration

Change from function-based to class-based mutations:

```python
# Before
app = create_fraiseql_app(
    mutations=[create_user, update_user, delete_user]
)

# After
app = create_fraiseql_app(
    mutations=[CreateUser, UpdateUser, DeleteUser]
)
```

## Common Patterns

### Handling Validation Errors

```sql
-- In PostgreSQL function
IF length(input_data->>'password') < 8 THEN
    result.status := 'validation_error';
    result.message := 'Invalid input data';
    result.extra_metadata := jsonb_build_object(
        'field_errors', jsonb_build_object(
            'password', 'Password must be at least 8 characters'
        )
    );
    RETURN result;
END IF;
```

```python
# In Python types
@fraiseql.failure
class CreateUserError:
    message: str
    field_errors: Optional[dict[str, str]] = None
```

### Returning Multiple Entities

```sql
-- Return multiple affected entities
result.object_data := (
    SELECT json_agg(data)
    FROM v_orders
    WHERE id = ANY(updated_order_ids)
);

result.extra_metadata := jsonb_build_object(
    'entity', 'affected_orders',
    'failed_orders', (
        SELECT json_agg(data)
        FROM v_orders
        WHERE id = ANY(failed_order_ids)
    )
);
```

```python
@fraiseql.success
class BulkUpdateSuccess:
    message: str
    affected_orders: list[Order]  # From object_data
    failed_orders: list[Order]    # From extra_metadata
```

### Handling Permissions

```sql
-- Check permissions in PostgreSQL
IF NOT has_permission(input_data->>'user_id', 'create_post') THEN
    result.status := 'forbidden';
    result.message := 'You do not have permission to create posts';
    RETURN result;
END IF;
```

## Benefits of Migration

1. **Single Source of Truth**: All business logic in PostgreSQL
2. **Better Performance**: Single round-trip to database
3. **Transactional Safety**: ACID guarantees for complex operations
4. **Less Code**: ~80% reduction in mutation code
5. **Type Safety**: Automatic object instantiation from JSON
6. **Testability**: PostgreSQL functions can be tested independently

## Testing Your Migration

### 1. Test PostgreSQL Functions Directly

```sql
-- Test success case
SELECT * FROM graphql.create_user(
    '{"email": "test@example.com", "name": "Test User", "password": "securepass"}'::jsonb
);

-- Test error case
SELECT * FROM graphql.create_user(
    '{"email": "existing@example.com", "name": "Test"}'::jsonb
);
```

### 2. Test Through GraphQL

```graphql
mutation CreateUser($input: CreateUserInput!) {
    createUser(input: $input) {
        ... on CreateUserSuccess {
            message
            user {
                id
                email
                name
            }
        }
        ... on CreateUserError {
            message
            conflictUser {
                id
                email
            }
        }
    }
}
```

## Gradual Migration

You can migrate incrementally:

1. Start with simple CRUD mutations
2. Move complex business logic mutations next
3. Keep manual resolvers for special cases temporarily

The system supports both approaches during the transition period.
