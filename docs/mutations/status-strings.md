# FraiseQL Status String Conventions

FraiseQL uses status strings in PostgreSQL functions to indicate mutation outcomes. These strings are parsed by the Rust layer and mapped to GraphQL Success/Error types.

## Status Categories

### 1. Success Statuses (No Colon)

Simple keywords indicating successful operations:

| Status | Meaning | GraphQL Type |
|--------|---------|--------------|
| `success` | Generic success | Success |
| `created` | Entity created | Success |
| `updated` | Entity modified | Success |
| `deleted` | Entity removed | Success |

**Example:**
```sql
RETURN ('created', 'User created successfully', v_user_id, 'User', v_user_json, ...)::mutation_result_v2;
```

### 2. Error Prefixes (Colon-Separated)

Prefixes indicating operation failures. These map to the Error type in GraphQL.

| Prefix | Meaning | HTTP Code | Example |
|--------|---------|-----------|---------|
| `failed:` | Generic failure | 500 | `failed:validation_error` |
| `unauthorized:` | Authentication required | 401 | `unauthorized:token_expired` |
| `forbidden:` | Insufficient permissions | 403 | `forbidden:admin_only` |
| `not_found:` | Resource doesn't exist | 404 | `not_found:user_missing` |
| `conflict:` | Resource conflict | 409 | `conflict:duplicate_email` |
| `timeout:` | Operation timeout | 408 | `timeout:external_api` |

**Example:**
```sql
IF EXISTS (SELECT 1 FROM users WHERE email = v_email) THEN
    RETURN ('conflict:duplicate_email', 'Email already exists', ...)::mutation_result_v2;
END IF;
```

### 3. Noop Prefix (Success with No Changes)

Indicates no change was made, but it's not an error. Maps to Success type.

| Prefix | Meaning | GraphQL Type |
|--------|---------|--------------|
| `noop:` | No operation performed | Success |

**Common noop reasons:**
- `noop:duplicate` - Entity already exists (idempotent operation)
- `noop:unchanged` - No fields changed
- `noop:blocked` - Blocked by business rules

**Example:**
```sql
INSERT INTO subscriptions (user_id, plan_id)
VALUES (v_user_id, v_plan_id)
ON CONFLICT DO NOTHING;

IF NOT FOUND THEN
    RETURN ('noop:duplicate', 'Already subscribed', v_user_id, ...)::mutation_result_v2;
END IF;
```

## Case Insensitivity

All status strings are matched **case-insensitively**:

```sql
'SUCCESS' = 'success' = 'Success'  ✅
'FAILED:validation' = 'failed:validation'  ✅
'Conflict:DUPLICATE' = 'conflict:duplicate'  ✅
```

## Complete Example

```sql
CREATE FUNCTION create_user(input_data JSONB)
RETURNS mutation_result_v2 AS $$
DECLARE
    v_email TEXT;
    v_user_id UUID;
    v_user_json JSONB;
BEGIN
    v_email := input_data->>'email';

    -- Validation error
    IF v_email IS NULL OR v_email = '' THEN
        RETURN (
            'failed:validation_error',
            'Email is required',
            NULL, NULL, NULL, NULL, NULL, NULL
        )::mutation_result_v2;
    END IF;

    -- Conflict error (duplicate)
    IF EXISTS (SELECT 1 FROM users WHERE email = v_email) THEN
        RETURN (
            'conflict:duplicate_email',
            'Email already exists',
            NULL, NULL, NULL, NULL, NULL, NULL
        )::mutation_result_v2;
    END IF;

    -- Success - create user
    INSERT INTO users (email, name)
    VALUES (v_email, input_data->>'name')
    RETURNING id, row_to_json(users.*) INTO v_user_id, v_user_json;

    RETURN (
        'created',
        'User created successfully',
        v_user_id::TEXT,
        'User',
        v_user_json,
        ARRAY['email', 'name'],
        NULL,
        NULL
    )::mutation_result_v2;
END;
$$ LANGUAGE plpgsql;
```

## GraphQL Response Mapping

| PostgreSQL Status | GraphQL Type | HTTP | Example Response |
|-------------------|--------------|------|------------------|
| `created` | Success | 200 | `{ "__typename": "CreateUserSuccess", ... }` |
| `failed:validation` | Error | 422 | `{ "__typename": "CreateUserError", ... }` |
| `conflict:duplicate` | Error | 409 | `{ "__typename": "CreateUserError", ... }` |
| `noop:duplicate` | Success | 200 | `{ "__typename": "CreateUserSuccess", ... }` |
| `timeout:database` | Error | 408 | `{ "__typename": "CreateUserError", ... }` |

## Best Practices

### ✅ DO

- Use specific error prefixes (`conflict:`, `not_found:`) over generic `failed:`
- Include descriptive reasons after the colon: `failed:email_format_invalid`
- Use `noop:` for idempotent operations that encounter existing data
- Return appropriate entity data even for noop/error cases when available

### ❌ DON'T

- Don't use `duplicate:` as a prefix - use `conflict:duplicate` (error) or `noop:duplicate` (success)
- Don't mix prefix categories: `failed:noop:...` is confusing
- Don't include sensitive information in status strings (use message field)
- Don't create custom prefixes - use the standard ones

## Migration from Old Patterns

If you have existing functions using custom statuses:

| Old Pattern | New Pattern | Type |
|-------------|-------------|------|
| `validation:field_required` | `failed:validation_error` | Error |
| `error:database` | `failed:database_error` | Error |
| `duplicate:email` | `conflict:duplicate_email` | Error |
| `already_exists` | `noop:duplicate` | Success |
