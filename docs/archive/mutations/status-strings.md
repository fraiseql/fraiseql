# FraiseQL Status String Conventions

FraiseQL uses status strings in PostgreSQL functions to indicate mutation outcomes. These strings are parsed by the Rust layer and mapped to GraphQL Success/Error types with HTTP-like status codes.

## Status Categories

### 1. Success Statuses (No Colon)

Simple keywords indicating successful operations:

| Status | Meaning | GraphQL Type | HTTP Code |
|--------|---------|--------------|-----------|
| `success` | Generic success | Success | 200 |
| `created` | Entity created | Success | 200 |
| `updated` | Entity modified | Success | 200 |
| `deleted` | Entity removed | Success | 200 |

**Example:**
```sql
RETURN ('created', 'User created successfully', v_user_id, 'User', v_user_json, ...)::mutation_response;
```

### 2. Error Prefixes (Colon-Separated)

Semantic prefixes indicating operation failures. These map to the Error type in GraphQL with specific HTTP-like codes.

| Prefix | Meaning | HTTP Code | Example |
|--------|---------|-----------|---------|
| `validation:` | Input validation failure | 422 | `validation:invalid_email` |
| `not_found:` | Resource doesn't exist | 404 | `not_found:user_missing` |
| `conflict:` | Resource conflict | 409 | `conflict:duplicate_email` |
| `unauthorized:` | Authentication required | 401 | `unauthorized:token_expired` |
| `forbidden:` | Insufficient permissions | 403 | `forbidden:admin_only` |
| `timeout:` | Operation timeout | 408 | `timeout:external_api` |
| `failed:` | System/database failure | 500 | `failed:database_error` |

**Examples:**
```sql
-- Validation error
IF v_email IS NULL OR v_email = '' THEN
    RETURN ('validation:invalid_email', 'Email is required', ...)::mutation_response;
END IF;

-- Not found error
IF NOT FOUND THEN
    RETURN ('not_found:user_missing', 'User not found', ...)::mutation_response;
END IF;

-- Conflict error
IF EXISTS (SELECT 1 FROM users WHERE email = v_email) THEN
    RETURN ('conflict:duplicate_email', 'Email already exists', ...)::mutation_response;
END IF;
```

### 3. Noop Prefix (Business Rules)

Indicates no change was made due to business rules, but it's not an error. Maps to Error type with 422 code.

| Prefix | Meaning | GraphQL Type | HTTP Code |
|--------|---------|--------------|-----------|
| `noop:` | No operation performed | Error | 422 |

**Common noop reasons:**
- `noop:already_exists` - Entity already exists (idempotent operation)
- `noop:no_changes` - No fields changed
- `noop:already_deleted` - Entity already soft-deleted

**Example:**
```sql
INSERT INTO subscriptions (user_id, plan_id)
VALUES (v_user_id, v_plan_id)
ON CONFLICT DO NOTHING;

IF NOT FOUND THEN
    RETURN ('noop:already_exists', 'Already subscribed', v_user_id, ...)::mutation_response;
END IF;
```

## Case Insensitivity

All status strings are matched **case-insensitively**:

```sql
'SUCCESS' = 'success' = 'Success'  ✅
'VALIDATION:invalid' = 'validation:invalid'  ✅
'Conflict:DUPLICATE' = 'conflict:duplicate'  ✅
```

## Complete Example

```sql
CREATE FUNCTION create_user(input_data JSONB)
RETURNS mutation_response AS $$
DECLARE
    v_email TEXT;
    v_user_id UUID;
    v_user_json JSONB;
BEGIN
    v_email := input_data->>'email';

    -- Validation error
    IF v_email IS NULL OR v_email = '' THEN
        RETURN (
            'validation:invalid_email',
            'Email is required',
            NULL, NULL, NULL, NULL, NULL, NULL
        )::mutation_response;
    END IF;

    -- Conflict error (duplicate)
    IF EXISTS (SELECT 1 FROM users WHERE email = v_email) THEN
        RETURN (
            'conflict:duplicate_email',
            'Email already exists',
            NULL, NULL, NULL, NULL, NULL, NULL
        )::mutation_response;
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
    )::mutation_response;
END;
$$ LANGUAGE plpgsql;
```

## GraphQL Response Mapping

| PostgreSQL Status | GraphQL Type | HTTP Code | Example Response |
|-------------------|--------------|-----------|------------------|
| `created` | Success | 200 | `{ "__typename": "CreateUserSuccess", ... }` |
| `validation:invalid_email` | Error | 422 | `{ "__typename": "CreateUserError", code: 422, ... }` |
| `not_found:user_missing` | Error | 404 | `{ "__typename": "CreateUserError", code: 404, ... }` |
| `conflict:duplicate` | Error | 409 | `{ "__typename": "CreateUserError", code: 409, ... }` |
| `noop:already_exists` | Error | 422 | `{ "__typename": "CreateUserError", code: 422, ... }` |
| `timeout:database` | Error | 408 | `{ "__typename": "CreateUserError", code: 408, ... }` |
| `failed:database_error` | Error | 500 | `{ "__typename": "CreateUserError", code: 500, ... }` |

## Best Practices

### ✅ DO

- Use specific semantic prefixes (`validation:`, `not_found:`, `conflict:`) over generic `failed:`
- Include descriptive reasons after the colon: `validation:invalid_email_format`
- Use `noop:` for idempotent operations that encounter existing data
- Return appropriate entity data even for noop/error cases when available
- Use `failed:` only for system/database errors in exception handlers

### ❌ DON'T

- Don't use old patterns like `failed:validation` or `failed:not_found` (use semantic prefixes directly)
- Don't use `validation_error:` (legacy - use `validation:` instead)
- Don't create custom prefixes - use the standard semantic ones
- Don't mix prefix categories: `failed:noop:...` is invalid
- Don't include sensitive information in status strings (use message field)

## Semantic Prefix Philosophy

**"One Obvious Way"** (Zen of Python):

- Each error category has ONE semantic prefix
- Status string directly indicates the HTTP-like error code
- Self-documenting: `not_found:` → 404, `validation:` → 422
- Follows industry standards (Stripe, GitHub, AWS APIs)

### Migration from Old Patterns

If you have legacy code using old patterns:

| Old Pattern (❌ Don't Use) | New Pattern (✅ Use) | Type |
|---------------------------|---------------------|------|
| `failed:validation` | `validation:invalid_input` | Error (422) |
| `failed:invalid_*` | `validation:invalid_*` | Error (422) |
| `failed:not_found` | `not_found:*` | Error (404) |
| `failed:*_not_found` | `not_found:*` | Error (404) |
| `failed:duplicate` | `conflict:duplicate` | Error (409) |
| `failed:*_exists` | `conflict:*_exists` | Error (409) |
| `validation_error:*` | `validation:*` | Error (422) |
| `already_exists` (bare) | `noop:already_exists` | Error (422) |

## Exception Handlers

Always use `failed:` prefix for system errors in exception blocks:

```sql
EXCEPTION
    WHEN OTHERS THEN
        RETURN (
            'failed:database_error',  -- System error (500)
            'Database error: ' || SQLERRM,
            NULL, NULL, NULL, NULL, NULL, NULL
        )::mutation_response;
```

## HTTP Code Reference

FraiseQL maps semantic prefixes to HTTP-like codes for better developer experience:

| HTTP Code | Prefix | Meaning |
|-----------|--------|---------|
| 200 | *(no prefix)* | Success |
| 401 | `unauthorized:` | Authentication failure |
| 403 | `forbidden:` | Permission denied |
| 404 | `not_found:` | Resource missing |
| 408 | `timeout:` | Operation timeout |
| 409 | `conflict:` | Resource conflict |
| 422 | `validation:`, `noop:` | Validation failure or business rule |
| 500 | `failed:` | System error |

**Note:** GraphQL always returns HTTP 200 at the transport layer. These codes are application-level metadata for categorization and client-side handling.
