# FraiseQL Mutations Quick Reference

One-page guide covering 90% of mutation use cases. For complete details, see [Mutation SQL Requirements](../guides/mutation-sql-requirements.md).

---

## Minimal Mutation Template

```sql
CREATE OR REPLACE FUNCTION create_thing(input_payload jsonb)
RETURNS mutation_response AS $$
DECLARE
    result mutation_response;
BEGIN
    -- Your logic here

    -- Success
    result.status := 'created';
    result.message := 'Thing created';
    result.entity := row_to_json(NEW);
    RETURN result;
EXCEPTION
    WHEN OTHERS THEN
        result.status := 'failed:error';
        result.message := SQLERRM;
        RETURN result;
END;
$$ LANGUAGE plpgsql;
```

---

## Status Strings (Auto-Error Generation)

| Status String | HTTP Code | identifier | Use Case |
|--------------|-----------|------------|----------|
| `'created'` | 201 | - | INSERT success |
| `'updated'` | 200 | - | UPDATE success |
| `'deleted'` | 200 | - | DELETE success |
| `'validation:'` | 422 | `validation` | Invalid input |
| `'failed:permission'` | 403 | `permission` | Access denied |
| `'not_found:user'` | 404 | `user` | Resource missing |
| `'conflict:duplicate'` | 409 | `duplicate` | Unique constraint |
| `'noop:exists'` | 422 | `exists` | Already exists |

**Format:** `prefix:identifier` → Auto-generates `errors` array

---

## Error Patterns

### Pattern 1: Auto-Generated (Simple)
```sql
-- Just set status and message
result.status := 'validation:';
result.message := 'Email is required';
-- Rust auto-generates: errors[{code: 422, identifier: "validation", ...}]
```

### Pattern 2: Explicit Multiple Errors
```sql
-- Build errors array manually
result.status := 'validation:';
result.message := 'Multiple validation errors';
result.metadata := jsonb_build_object(
    'errors', jsonb_build_array(
        jsonb_build_object(
            'code', 422,
            'identifier', 'invalid_email',
            'message', 'Email format invalid',
            'details', jsonb_build_object('field', 'email')
        ),
        jsonb_build_object(
            'code', 422,
            'identifier', 'password_weak',
            'message', 'Password too short',
            'details', jsonb_build_object('field', 'password')
        )
    )
);
```

---

## Common Patterns

### Input Validation
```sql
-- Extract and validate
user_email := input_payload->>'email';
IF user_email IS NULL OR user_email !~ '@' THEN
    result.status := 'validation:';
    result.message := 'Valid email required';
    RETURN result;
END IF;
```

### Not Found Check
```sql
SELECT * INTO user_record FROM users WHERE id = user_id;
IF NOT FOUND THEN
    result.status := 'not_found:user';
    result.message := format('User %s not found', user_id);
    RETURN result;
END IF;
```

### Duplicate Check
```sql
IF EXISTS (SELECT 1 FROM users WHERE email = user_email) THEN
    result.status := 'conflict:duplicate';
    result.message := 'Email already registered';
    RETURN result;
END IF;
```

### Conditional Update (Optimistic Locking)
```sql
UPDATE machines SET status = 'running'
WHERE id = machine_id AND status = 'idle'
RETURNING * INTO machine_record;

IF NOT FOUND THEN
    result.status := 'noop:already_running';
    result.message := 'Machine already running';
    RETURN result;
END IF;
```

---

## mutation_response Fields

```sql
CREATE TYPE mutation_response AS (
    status text,           -- Required: 'created', 'failed:*', etc.
    message text,          -- Required: Human-readable message
    entity_id text,        -- Optional: ID of affected entity
    entity_type text,      -- Optional: 'User', 'Post', etc.
    entity jsonb,          -- Optional: Full entity data
    updated_fields text[], -- Optional: ['name', 'email']
    cascade jsonb,         -- Optional: Related changes
    metadata jsonb         -- Optional: Extra context, explicit errors
);
```

**What Rust Generates (You DON'T set):**
- ❌ `code` - Generated from status string
- ❌ `identifier` - Extracted from status string
- ❌ `errors` array - Auto-generated or from metadata.errors

**What You Set:**
- ✅ `status` - Status string
- ✅ `message` - Summary message
- ✅ `entity` - Entity data (use `row_to_json(NEW)`)
- ✅ `metadata.errors` - (Optional) For Pattern 2

---

## Helper Functions

**Validation Helpers** (from `sql/helpers/mutation_validation.sql`):

```sql
-- Validation functions
validate_status_format(status text) -> boolean
validate_errors_array(metadata jsonb) -> boolean
validate_mutation_response(result mutation_response) -> boolean

-- Utility functions
get_expected_code(status text) -> integer
extract_identifier(status text) -> text
build_error_object(code int, identifier text, message text, details jsonb) -> jsonb
mutation_assert(condition boolean, error_message text) -> void
```

**Usage:**
```sql
-- Validate response before returning
PERFORM mutation_assert(
    validate_mutation_response(result),
    'Response validation failed'
);

-- Build error object for metadata.errors
result.metadata := jsonb_build_object(
    'errors', jsonb_build_array(
        build_error_object(422, 'invalid_email', 'Email format invalid',
            jsonb_build_object('field', 'email'))
    )
);
```

---

## GraphQL Response Structure

```json
{
  "data": {
    "createUser": {
      "__typename": "CreateUserError",
      "code": 422,              // ← Root: Quick access
      "status": "validation:",
      "message": "Email required",
      "errors": [{             // ← Array: Structured iteration
        "code": 422,
        "identifier": "validation",
        "message": "Email required",
        "details": null
      }]
    }
  }
}
```

**Use root fields:** Quick checks, single error display
**Use errors array:** Multiple errors, form field mapping

---

## Quick Debugging

```bash
# Test function directly in psql
SELECT * FROM create_user('{"name": "John"}'::jsonb);

# Check raw JSON output
SELECT row_to_json(create_user('{"name": "John"}'::jsonb));

# Validate status string format
SELECT status ~ '^(created|updated|deleted|failed|not_found|conflict|noop)(:.+)?$';
```

---

## Next Steps

- **Complete Guide:** [Mutation SQL Requirements](../guides/mutation-sql-requirements.md)
- **Error Handling Deep Dive:** [Error Handling Patterns](../guides/error-handling-patterns.md)
- **Troubleshooting:** [Troubleshooting Guide](../guides/troubleshooting-mutations.md)
- **Examples:** [Real-World Mutations](../../examples/mutation-patterns/)
