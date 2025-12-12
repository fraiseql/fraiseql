# Work Package: Developer Experience Enhancements

**Package ID:** WP-036
**Assignee Role:** Technical Writer (TW-CORE) + Full-Stack Engineer (ENG-CORE)
**Priority:** P1 - High Impact, Low Effort
**Estimated Hours:** 20 hours (12 documentation + 8 engineering)
**Dependencies:** WP-034 (Native Error Arrays - COMPLETE)
**Target Version:** v1.8.1

---

## Executive Summary

**Problem:** FraiseQL has excellent core DX, but developers struggle with:
- ❌ No quick reference for common patterns
- ❌ Debugging errors requires digging through docs
- ❌ No SQL validation helpers to catch mistakes early
- ❌ Missing VS Code integration for productivity
- ❌ Limited real-world mutation examples

**Solution:** Five high-impact, low-effort improvements:
1. One-page quick reference cheat sheet
2. Comprehensive troubleshooting guide
3. SQL validation helper functions
4. VS Code extension with snippets and autocomplete
5. Real-world mutation examples repository

**User Impact:**
- ⏱️ 10x faster lookup for common patterns
- 🐛 Faster debugging with structured troubleshooting
- ✅ Catch errors at SQL level before runtime
- 💻 IDE productivity boost (autocomplete, snippets)
- 📚 Learn from real production patterns

---

## Task 1: Quick Reference Cheat Sheet

**Assignee:** TW-CORE
**Time:** 3 hours
**Priority:** P0 - Critical (most requested)

### Objective

Create single-page quick reference covering 90% of mutation use cases.

### Implementation

**File:** `docs/quick-reference/mutations-cheat-sheet.md`

**Structure:**

```markdown
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
| `'failed:validation'` | 422 | `validation` | Invalid input |
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
result.status := 'failed:validation';
result.message := 'Email is required';
-- Rust auto-generates: errors[{code: 422, identifier: "validation", ...}]
```

### Pattern 2: Explicit Multiple Errors
```sql
-- Build errors array manually
result.status := 'failed:validation';
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
    result.status := 'failed:validation';
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
    result.status := 'conflict:duplicate_email';
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

```sql
-- Success helpers
mutation_success(message, entity_data)
mutation_created(message, entity_data)
mutation_updated(message, entity_data)
mutation_deleted(message)

-- Error helpers
mutation_validation_error(message)
mutation_not_found(message)
mutation_error(status, message)
```

---

## GraphQL Response Structure

```json
{
  "data": {
    "createUser": {
      "__typename": "CreateUserError",
      "code": 422,              // ← Root: Quick access
      "status": "failed:validation",
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
```

### Acceptance Criteria

- [ ] Cheat sheet covers 90% of use cases in <1 page scroll
- [ ] All code examples are copy-paste ready
- [ ] Links to detailed docs for each section
- [ ] Tested with 3 junior developers (can they solve common tasks?)
- [ ] Added to main docs navigation

### Verification

```bash
# Check readability
wc -l docs/quick-reference/mutations-cheat-sheet.md  # Should be ~200 lines
grep -c "```sql" docs/quick-reference/mutations-cheat-sheet.md  # Should have 10+ examples

# Test examples
cd examples/test-cheat-sheet
psql < docs/quick-reference/mutations-cheat-sheet.md  # All SQL should be valid
```

---

## Task 2: Troubleshooting Guide

**Assignee:** TW-CORE
**Time:** 3 hours
**Priority:** P0 - Critical

### Objective

Create structured troubleshooting guide for common mutation problems.

### Implementation

**File:** `docs/guides/troubleshooting-mutations.md`

**Structure:**

```markdown
# Troubleshooting Mutations

Common problems when writing FraiseQL mutations and how to solve them.

---

## Quick Diagnosis

| Symptom | Likely Cause | Section |
|---------|--------------|---------|
| No errors in response | metadata.errors malformed | [Errors Not Showing](#errors-not-showing) |
| Wrong HTTP code | Invalid status prefix | [Wrong Error Code](#wrong-error-code) |
| GraphQL validation error | Missing required fields | [Schema Mismatch](#schema-mismatch) |
| Function not found | Schema search path issue | [Function Not Found](#function-not-found) |
| CASCADE not appearing | Selection not requested | [CASCADE Missing](#cascade-missing) |
| Null entity on success | entity field not set | [Null Entity](#null-entity) |

---

## Errors Not Showing

**Problem:** GraphQL response has `code`, `status`, `message`, but `errors` array is empty or missing.

### Diagnosis

```bash
# Test function directly
SELECT * FROM your_function('{"test": "data"}'::jsonb);

# Check metadata field
SELECT metadata FROM your_function('{"test": "data"}'::jsonb);
```

### Common Causes

#### 1. Malformed metadata.errors JSONB

**Symptom:** `errors` array is empty

**Cause:** `metadata.errors` is not a valid JSONB array

```sql
-- ❌ WRONG: String instead of JSONB array
result.metadata := '{"errors": "[...]"}';

-- ❌ WRONG: Not an array
result.metadata := jsonb_build_object('errors', jsonb_build_object(...));

-- ✅ CORRECT: JSONB array
result.metadata := jsonb_build_object(
    'errors', jsonb_build_array(
        jsonb_build_object('code', 422, 'identifier', 'validation', ...)
    )
);
```

#### 2. Missing Required Error Fields

**Symptom:** Rust pipeline error in logs

**Cause:** `metadata.errors` objects missing required fields

```sql
-- ❌ WRONG: Missing 'message' field
jsonb_build_object('code', 422, 'identifier', 'validation')

-- ✅ CORRECT: All required fields
jsonb_build_object(
    'code', 422,
    'identifier', 'validation',
    'message', 'Validation failed',
    'details', null
)
```

#### 3. Pattern 1 Status String Format Invalid

**Symptom:** Auto-generated error has `identifier: "general_error"`

**Cause:** Status string doesn't follow `prefix:identifier` format

```sql
-- ❌ WRONG: No colon separator
result.status := 'failed_validation';  -- → identifier: "general_error"

-- ✅ CORRECT: Use colon
result.status := 'failed:validation';  -- → identifier: "validation"
```

### Solution

**Validate your status string:**
```sql
-- Add assertion in your function
ASSERT result.status ~ '^(created|updated|deleted|failed|not_found|conflict|noop)(:.+)?$',
    format('Invalid status format: %s', result.status);
```

---

## Wrong Error Code

**Problem:** Getting `422` when you expect `404`, or vice versa.

### Diagnosis

```bash
# Check what code your status generates
SELECT status,
    CASE
        WHEN status LIKE 'failed:%' THEN 422
        WHEN status LIKE 'not_found:%' THEN 404
        WHEN status LIKE 'conflict:%' THEN 409
        -- ... etc
    END as expected_code
FROM your_function(...);
```

### Common Causes

#### Wrong Status Prefix

```sql
-- ❌ WRONG: Using 'failed:' for not found
result.status := 'failed:user_not_found';  -- → 422

-- ✅ CORRECT: Use 'not_found:' prefix
result.status := 'not_found:user';  -- → 404
```

### Status Prefix to Code Mapping

| Prefix | Code | Use For |
|--------|------|---------|
| `failed:` | 422 | Validation, business logic errors |
| `not_found:` | 404 | Resource doesn't exist |
| `conflict:` | 409 | Duplicates, constraint violations |
| `unauthorized:` | 401 | Missing authentication |
| `forbidden:` | 403 | Permission denied |
| `timeout:` | 408 | Operation timeout |
| `noop:` | 422 | No changes made |

---

## Schema Mismatch

**Problem:** GraphQL validation error: "Field 'X' not found in type 'CreateUserError'"

### Diagnosis

Check your Python type definitions match SQL output:

```python
@fraiseql.failure
class CreateUserError:
    status: str
    message: str
    code: int
    errors: list[Error]  # ← Make sure this is present!
```

### Common Causes

#### 1. Missing `errors` Field in Error Type

```python
# ❌ WRONG: No errors field
@fraiseql.failure
class CreateUserError:
    status: str
    message: str
    code: int
    # Missing: errors: list[Error]

# ✅ CORRECT: Include errors
@fraiseql.failure
class CreateUserError:
    status: str
    message: str
    code: int
    errors: list[Error]  # Auto-populated by Rust
```

#### 2. Wrong Field Names

GraphQL is case-sensitive and follows camelCase (if `auto_camel_case=True`).

```python
# Python definition (snake_case)
class CreateUserError:
    error_code: int  # ← Will become "errorCode" in GraphQL

# GraphQL query must match
mutation {
  createUser {
    errorCode  # ← Must use camelCase
  }
}
```

---

## Function Not Found

**Problem:** `ERROR: function app.create_user(jsonb) does not exist`

### Diagnosis

```sql
-- Check function exists
SELECT proname, pronamespace::regnamespace
FROM pg_proc
WHERE proname = 'create_user';

-- Check search path
SHOW search_path;
```

### Common Causes

#### 1. Wrong Schema

```sql
-- Function created in different schema
CREATE FUNCTION public.create_user(...) -- ← Created in 'public'

-- But app expects 'app' schema
-- FraiseQL looks in: app schema first, then search_path
```

**Solution:**
```sql
-- Option 1: Create in 'app' schema
CREATE FUNCTION app.create_user(...)

-- Option 2: Set search path
ALTER DATABASE your_db SET search_path TO app, public;
```

#### 2. Wrong Signature

```sql
-- Function defined with different parameter
CREATE FUNCTION create_user(data json) -- ← json not jsonb

-- But called with jsonb
SELECT create_user('{"test": 1}'::jsonb);  -- ← Fails
```

**Solution:** Use `jsonb` consistently:
```sql
CREATE FUNCTION create_user(input_payload jsonb)
```

---

## CASCADE Missing

**Problem:** `cascade` field is null in response even though function returns CASCADE data.

### Diagnosis

```sql
-- Check function returns cascade
SELECT cascade FROM your_function(...)::mutation_response;
```

### Common Cause

**GraphQL query doesn't select cascade field:**

```graphql
# ❌ WRONG: Not requesting cascade
mutation {
  createUser(input: {...}) {
    user { id }
    # Missing: cascade { ... }
  }
}

# ✅ CORRECT: Request cascade
mutation {
  createUser(input: {...}) {
    user { id }
    cascade {  # ← Must explicitly request
      updated { ... }
      deleted { ... }
    }
  }
}
```

**Why:** FraiseQL only includes CASCADE if selected (GraphQL spec compliance).

---

## Null Entity

**Problem:** Success response has `user: null` instead of entity data.

### Diagnosis

```sql
-- Check entity field
SELECT entity FROM your_function(...)::mutation_response;
```

### Common Causes

#### 1. Forgot to Set entity Field

```sql
-- ❌ WRONG: entity not set
result.status := 'created';
result.message := 'User created';
RETURN result;  -- entity is NULL

-- ✅ CORRECT: Set entity
result.status := 'created';
result.message := 'User created';
result.entity := row_to_json(NEW);  -- ← Set entity!
RETURN result;
```

#### 2. Using OLD Instead of NEW

```sql
-- ❌ WRONG: OLD is the pre-UPDATE state
UPDATE users SET name = new_name WHERE id = user_id
RETURNING * INTO user_record;

result.entity := row_to_json(OLD);  -- ← OLD data!

-- ✅ CORRECT: Use NEW or RETURNING
UPDATE users SET name = new_name WHERE id = user_id
RETURNING * INTO user_record;

result.entity := row_to_json(user_record);  -- ← Updated data
```

#### 3. Entity Not Found (DELETE)

```sql
-- DELETE operations: entity should be null (deleted)
result.status := 'deleted';
result.message := 'User deleted';
result.entity := NULL;  -- ← Correct for DELETE
result.entity_id := old_user_id::text;  -- ← Use entity_id instead
```

---

## Performance Issues

**Problem:** Mutation is slow.

### Diagnosis

```sql
EXPLAIN ANALYZE SELECT * FROM your_function(...);
```

### Common Causes

#### 1. Missing Index

```sql
-- Slow: No index on email
SELECT * FROM users WHERE email = user_email;

-- Fix: Add index
CREATE INDEX idx_users_email ON users(email);
```

#### 2. N+1 Queries in CASCADE

```sql
-- ❌ SLOW: Loop with individual queries
FOR record IN SELECT * FROM related_table LOOP
    -- Multiple queries
END LOOP;

-- ✅ FAST: Single batch query
SELECT jsonb_agg(row_to_json(r))
FROM related_table r
WHERE r.parent_id = entity_id;
```

---

## Debug Checklist

When mutation isn't working:

1. **Test SQL directly:**
   ```sql
   SELECT * FROM your_function('{"test": "data"}'::jsonb);
   ```

2. **Check status format:**
   ```sql
   SELECT status ~ '^(created|failed|not_found|conflict|noop)(:.+)?$' as valid;
   ```

3. **Validate metadata.errors:**
   ```sql
   SELECT jsonb_typeof(metadata->'errors') = 'array' as is_array;
   ```

4. **Check Python types match:**
   - Error types have `errors: list[Error]`
   - Success types have entity field

5. **Verify GraphQL selection:**
   - Requesting cascade? Must select it
   - Using camelCase if `auto_camel_case=True`

6. **Check logs:**
   ```bash
   # Rust pipeline logs show transformation details
   tail -f /var/log/fraiseql/mutations.log
   ```

---

## Getting Help

Still stuck?

1. **Check Examples:** `examples/mutation-patterns/` has real-world cases
2. **Read Full Guide:** [Mutation SQL Requirements](./mutation-sql-requirements.md)
3. **GitHub Issues:** Search existing issues or create new one
4. **Discussions:** Ask in GitHub Discussions for community help

**Include in bug reports:**
- SQL function code
- GraphQL query
- Expected vs actual response
- PostgreSQL version
- FraiseQL version
```

### Acceptance Criteria

- [ ] Covers top 10 support issues from GitHub
- [ ] Each problem has: Symptom → Diagnosis → Solution
- [ ] All code examples are tested
- [ ] Quick diagnosis table at top for scanning
- [ ] Links to related documentation

### Verification

```bash
# Check coverage of common issues
grep -c "^## " docs/guides/troubleshooting-mutations.md  # Should have 8+ issues

# Verify all SQL examples are valid
extract_sql_blocks.sh docs/guides/troubleshooting-mutations.md | psql

# Test with real support issues
# Take 5 recent GitHub issues → Can users self-solve with guide?
```

---

## Task 3: SQL Validation Helpers

**Assignee:** ENG-CORE
**Time:** 4 hours
**Priority:** P1

### Objective

Provide SQL helper functions to validate mutation responses before runtime.

### Implementation

**File:** `sql/helpers/mutation_validation.sql`

```sql
-- ============================================================================
-- FraiseQL Mutation Validation Helpers
-- ============================================================================
-- These functions help catch common mistakes in mutation functions during
-- development, before runtime errors occur.
--
-- Usage:
--   1. Include in your migration: \i sql/helpers/mutation_validation.sql
--   2. Use in your mutation functions to validate responses
--   3. Remove assertions in production (or keep for safety)
-- ============================================================================

-- ----------------------------------------------------------------------------
-- validate_status_format: Check status string follows FraiseQL convention
-- ----------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION validate_status_format(status text)
RETURNS boolean AS $$
BEGIN
    -- Valid patterns:
    -- - Simple: 'created', 'updated', 'deleted', 'success'
    -- - With identifier: 'failed:validation', 'not_found:user', etc.
    RETURN status ~ '^(success|created|updated|deleted|failed|not_found|conflict|unauthorized|forbidden|timeout|noop)(:.+)?$';
END;
$$ LANGUAGE plpgsql IMMUTABLE;

COMMENT ON FUNCTION validate_status_format IS
'Validates that status string follows FraiseQL convention: prefix or prefix:identifier';

-- Usage example:
-- ASSERT validate_status_format(result.status),
--     format('Invalid status format: %s', result.status);

-- ----------------------------------------------------------------------------
-- validate_errors_array: Check metadata.errors structure
-- ----------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION validate_errors_array(metadata jsonb)
RETURNS boolean AS $$
DECLARE
    errors jsonb;
    error_obj jsonb;
BEGIN
    -- If no metadata, valid (auto-generation will handle it)
    IF metadata IS NULL THEN
        RETURN true;
    END IF;

    -- If no errors in metadata, valid
    errors := metadata->'errors';
    IF errors IS NULL THEN
        RETURN true;
    END IF;

    -- Must be an array
    IF jsonb_typeof(errors) != 'array' THEN
        RAISE NOTICE 'metadata.errors must be a JSONB array, got: %', jsonb_typeof(errors);
        RETURN false;
    END IF;

    -- Each error must have required fields
    FOR error_obj IN SELECT jsonb_array_elements(errors)
    LOOP
        -- Check required fields exist
        IF NOT (error_obj ? 'code' AND error_obj ? 'identifier' AND error_obj ? 'message') THEN
            RAISE NOTICE 'Error object missing required fields (code, identifier, message): %', error_obj;
            RETURN false;
        END IF;

        -- Check types
        IF jsonb_typeof(error_obj->'code') != 'number' THEN
            RAISE NOTICE 'Error code must be number, got: %', jsonb_typeof(error_obj->'code');
            RETURN false;
        END IF;

        IF jsonb_typeof(error_obj->'identifier') != 'string' THEN
            RAISE NOTICE 'Error identifier must be string, got: %', jsonb_typeof(error_obj->'identifier');
            RETURN false;
        END IF;

        IF jsonb_typeof(error_obj->'message') != 'string' THEN
            RAISE NOTICE 'Error message must be string, got: %', jsonb_typeof(error_obj->'message');
            RETURN false;
        END IF;
    END LOOP;

    RETURN true;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

COMMENT ON FUNCTION validate_errors_array IS
'Validates that metadata.errors is properly structured for FraiseQL';

-- Usage example:
-- ASSERT validate_errors_array(result.metadata),
--     'Invalid metadata.errors structure';

-- ----------------------------------------------------------------------------
-- validate_mutation_response: Comprehensive validation
-- ----------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION validate_mutation_response(result mutation_response)
RETURNS boolean AS $$
BEGIN
    -- Status format
    IF NOT validate_status_format(result.status) THEN
        RAISE NOTICE 'Invalid status format: %', result.status;
        RETURN false;
    END IF;

    -- Message required
    IF result.message IS NULL OR trim(result.message) = '' THEN
        RAISE NOTICE 'Message is required';
        RETURN false;
    END IF;

    -- Errors array structure
    IF NOT validate_errors_array(result.metadata) THEN
        RETURN false;
    END IF;

    -- Success cases should have entity (unless DELETE)
    IF result.status IN ('created', 'updated', 'success') THEN
        IF result.entity IS NULL THEN
            RAISE NOTICE 'Success status "%" should have entity data', result.status;
            RETURN false;
        END IF;
    END IF;

    RETURN true;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

COMMENT ON FUNCTION validate_mutation_response IS
'Comprehensive validation of mutation_response before returning';

-- Usage example:
-- ASSERT validate_mutation_response(result),
--     'Mutation response validation failed';

-- ----------------------------------------------------------------------------
-- get_expected_code: Get HTTP code for status string
-- ----------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION get_expected_code(status text)
RETURNS integer AS $$
BEGIN
    CASE
        WHEN status LIKE 'created%' THEN RETURN 201;
        WHEN status LIKE 'success%' THEN RETURN 200;
        WHEN status LIKE 'updated%' THEN RETURN 200;
        WHEN status LIKE 'deleted%' THEN RETURN 200;
        WHEN status LIKE 'failed:%' THEN RETURN 422;
        WHEN status LIKE 'not_found:%' THEN RETURN 404;
        WHEN status LIKE 'conflict:%' THEN RETURN 409;
        WHEN status LIKE 'unauthorized:%' THEN RETURN 401;
        WHEN status LIKE 'forbidden:%' THEN RETURN 403;
        WHEN status LIKE 'timeout:%' THEN RETURN 408;
        WHEN status LIKE 'noop:%' THEN RETURN 422;
        ELSE RETURN 500;
    END CASE;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

COMMENT ON FUNCTION get_expected_code IS
'Returns the HTTP code that FraiseQL will generate for a given status string';

-- Usage example:
-- SELECT get_expected_code('failed:validation');  -- Returns 422

-- ----------------------------------------------------------------------------
-- extract_identifier: Extract identifier from status string
-- ----------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION extract_identifier(status text)
RETURNS text AS $$
BEGIN
    -- Split on colon and return second part
    IF position(':' in status) > 0 THEN
        RETURN split_part(status, ':', 2);
    ELSE
        RETURN 'general_error';
    END IF;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

COMMENT ON FUNCTION extract_identifier IS
'Extracts the identifier part from a status string (part after colon)';

-- Usage example:
-- SELECT extract_identifier('failed:validation');  -- Returns 'validation'

-- ----------------------------------------------------------------------------
-- build_error_object: Helper to build properly formatted error object
-- ----------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION build_error_object(
    p_code integer,
    p_identifier text,
    p_message text,
    p_details jsonb DEFAULT NULL
)
RETURNS jsonb AS $$
BEGIN
    RETURN jsonb_build_object(
        'code', p_code,
        'identifier', p_identifier,
        'message', p_message,
        'details', p_details
    );
END;
$$ LANGUAGE plpgsql IMMUTABLE;

COMMENT ON FUNCTION build_error_object IS
'Builds a properly formatted error object for metadata.errors array';

-- Usage example:
-- SELECT build_error_object(422, 'invalid_email', 'Email format invalid', '{"field": "email"}'::jsonb);

-- ----------------------------------------------------------------------------
-- mutation_assert: Conditional assertion for development
-- ----------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION mutation_assert(
    condition boolean,
    error_message text
)
RETURNS void AS $$
BEGIN
    -- Only assert in development (when debug is enabled)
    -- Set: ALTER DATABASE mydb SET fraiseql.debug = 'on';
    IF current_setting('fraiseql.debug', true) = 'on' THEN
        IF NOT condition THEN
            RAISE EXCEPTION '%', error_message;
        END IF;
    ELSIF NOT condition THEN
        -- In production, log warning but don't fail
        RAISE WARNING '%', error_message;
    END IF;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION mutation_assert IS
'Conditional assertion that throws in debug mode, warns in production';

-- Usage example:
-- PERFORM mutation_assert(
--     validate_status_format(result.status),
--     format('Invalid status: %s', result.status)
-- );

-- ============================================================================
-- Example Usage in Mutation Function
-- ============================================================================

/*
CREATE OR REPLACE FUNCTION create_user(input_payload jsonb)
RETURNS mutation_response AS $$
DECLARE
    result mutation_response;
    user_email text;
BEGIN
    -- Extract input
    user_email := input_payload->>'email';

    -- Validation
    IF user_email IS NULL THEN
        result.status := 'failed:validation';
        result.message := 'Email is required';

        -- Optional: Add explicit errors
        result.metadata := jsonb_build_object(
            'errors', jsonb_build_array(
                build_error_object(422, 'email_required', 'Email is required',
                    jsonb_build_object('field', 'email'))
            )
        );

        -- Validate before returning
        PERFORM mutation_assert(
            validate_mutation_response(result),
            'Mutation response validation failed'
        );

        RETURN result;
    END IF;

    -- ... rest of function

    -- Success
    result.status := 'created';
    result.message := 'User created';
    result.entity := row_to_json(NEW);

    -- Validate before returning
    PERFORM mutation_assert(
        validate_mutation_response(result),
        'Mutation response validation failed'
    );

    RETURN result;
END;
$$ LANGUAGE plpgsql;
*/

-- ============================================================================
-- Tests
-- ============================================================================

DO $$
BEGIN
    -- Test validate_status_format
    ASSERT validate_status_format('created') = true, 'created should be valid';
    ASSERT validate_status_format('failed:validation') = true, 'failed:validation should be valid';
    ASSERT validate_status_format('not_found:user') = true, 'not_found:user should be valid';
    ASSERT validate_status_format('invalid_format') = false, 'invalid_format should be invalid';
    ASSERT validate_status_format('failed-validation') = false, 'failed-validation (dash) should be invalid';

    -- Test extract_identifier
    ASSERT extract_identifier('failed:validation') = 'validation', 'Should extract validation';
    ASSERT extract_identifier('not_found:user') = 'user', 'Should extract user';
    ASSERT extract_identifier('created') = 'general_error', 'Should return general_error for no colon';

    -- Test get_expected_code
    ASSERT get_expected_code('created') = 201, 'created should map to 201';
    ASSERT get_expected_code('failed:validation') = 422, 'failed:* should map to 422';
    ASSERT get_expected_code('not_found:user') = 404, 'not_found:* should map to 404';
    ASSERT get_expected_code('conflict:duplicate') = 409, 'conflict:* should map to 409';

    RAISE NOTICE 'All validation helper tests passed!';
END;
$$;
```

### Integration Example

**File:** `examples/mutation-patterns/validated-user-creation.sql`

```sql
-- Example: User creation with validation helpers

CREATE OR REPLACE FUNCTION create_user_validated(input_payload jsonb)
RETURNS mutation_response AS $$
DECLARE
    result mutation_response;
    user_email text;
    user_name text;
    validation_errors jsonb := '[]'::jsonb;
BEGIN
    -- Extract input
    user_email := input_payload->>'email';
    user_name := input_payload->>'name';

    -- Collect validation errors
    IF user_email IS NULL THEN
        validation_errors := validation_errors ||
            build_error_object(422, 'email_required', 'Email is required',
                jsonb_build_object('field', 'email'));
    ELSIF user_email !~ '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$' THEN
        validation_errors := validation_errors ||
            build_error_object(422, 'email_invalid', 'Email format invalid',
                jsonb_build_object('field', 'email', 'value', user_email));
    END IF;

    IF user_name IS NULL THEN
        validation_errors := validation_errors ||
            build_error_object(422, 'name_required', 'Name is required',
                jsonb_build_object('field', 'name'));
    END IF;

    -- Return validation errors if any
    IF jsonb_array_length(validation_errors) > 0 THEN
        result.status := 'failed:validation';
        result.message := format('%s validation error(s)', jsonb_array_length(validation_errors));
        result.metadata := jsonb_build_object('errors', validation_errors);

        -- Validate response structure
        PERFORM mutation_assert(
            validate_mutation_response(result),
            'Validation response structure invalid'
        );

        RETURN result;
    END IF;

    -- Check duplicate
    IF EXISTS (SELECT 1 FROM users WHERE email = user_email) THEN
        result.status := 'conflict:duplicate_email';
        result.message := 'Email already registered';

        PERFORM mutation_assert(validate_status_format(result.status),
            format('Invalid status format: %s', result.status));

        RETURN result;
    END IF;

    -- Create user
    INSERT INTO users (email, name)
    VALUES (user_email, user_name)
    RETURNING * INTO user_record;

    -- Success
    result.status := 'created';
    result.message := 'User created successfully';
    result.entity := row_to_json(user_record);
    result.entity_id := user_record.id::text;
    result.entity_type := 'User';

    -- Final validation
    PERFORM mutation_assert(
        validate_mutation_response(result),
        'Success response structure invalid'
    );

    RETURN result;

EXCEPTION
    WHEN OTHERS THEN
        result.status := 'failed:error';
        result.message := SQLERRM;
        RETURN result;
END;
$$ LANGUAGE plpgsql;
```

### Acceptance Criteria

- [ ] `validate_status_format()` catches invalid formats
- [ ] `validate_errors_array()` catches malformed metadata.errors
- [ ] `validate_mutation_response()` comprehensive check
- [ ] `build_error_object()` helper reduces boilerplate
- [ ] `mutation_assert()` throws in debug, warns in production
- [ ] All functions have tests
- [ ] Documentation in function comments
- [ ] Example integration in mutation function

### Verification

```bash
# Run tests
psql < sql/helpers/mutation_validation.sql
# Should show: "All validation helper tests passed!"

# Test with real mutation
psql < examples/mutation-patterns/validated-user-creation.sql

# Test validation catches errors
psql -c "SELECT validate_status_format('invalid_status');"  # Should return false
psql -c "SELECT validate_status_format('failed:validation');"  # Should return true
```

---

## Task 4: VS Code Extension

**Assignee:** ENG-CORE
**Time:** 6 hours
**Priority:** P1

### Objective

Create VS Code extension with FraiseQL snippets, autocomplete, and syntax highlighting.

### Implementation

**Directory:** `.vscode-extension/fraiseql/`

**package.json:**
```json
{
  "name": "fraiseql-tools",
  "displayName": "FraiseQL Tools",
  "description": "Snippets, autocomplete, and validation for FraiseQL mutations",
  "version": "1.0.0",
  "publisher": "fraiseql",
  "engines": {
    "vscode": "^1.80.0"
  },
  "categories": [
    "Snippets",
    "Programming Languages"
  ],
  "keywords": [
    "fraiseql",
    "graphql",
    "postgresql",
    "plpgsql"
  ],
  "contributes": {
    "snippets": [
      {
        "language": "sql",
        "path": "./snippets/mutations.json"
      }
    ],
    "languages": [
      {
        "id": "sql",
        "extensions": [".sql"]
      }
    ]
  }
}
```

**snippets/mutations.json:**
```json
{
  "FraiseQL Mutation Function": {
    "prefix": "fraiseql-mutation",
    "body": [
      "CREATE OR REPLACE FUNCTION ${1:function_name}(input_payload jsonb)",
      "RETURNS mutation_response AS $$",
      "DECLARE",
      "    result mutation_response;",
      "    ${2:-- Declare variables}",
      "BEGIN",
      "    ${3:-- Extract input}",
      "    ",
      "    ${4:-- Validation}",
      "    ",
      "    ${5:-- Business logic}",
      "    ",
      "    -- Success",
      "    result.status := '${6|created,updated,deleted,success|}';",
      "    result.message := '${7:Operation successful}';",
      "    result.entity := row_to_json(${8:NEW});",
      "    result.entity_type := '${9:EntityType}';",
      "    RETURN result;",
      "    ",
      "EXCEPTION",
      "    WHEN OTHERS THEN",
      "        result.status := 'failed:error';",
      "        result.message := SQLERRM;",
      "        RETURN result;",
      "END;",
      "$$ LANGUAGE plpgsql;"
    ],
    "description": "FraiseQL mutation function template"
  },

  "FraiseQL Validation Error (Pattern 1)": {
    "prefix": "fraiseql-error-simple",
    "body": [
      "result.status := '${1|failed:validation,failed:permission,not_found:user,conflict:duplicate,noop:exists|}';",
      "result.message := '${2:Error message}';",
      "RETURN result;"
    ],
    "description": "Simple error with auto-generated errors array"
  },

  "FraiseQL Explicit Errors (Pattern 2)": {
    "prefix": "fraiseql-error-explicit",
    "body": [
      "result.status := 'failed:validation';",
      "result.message := '${1:Multiple validation errors}';",
      "result.metadata := jsonb_build_object(",
      "    'errors', jsonb_build_array(",
      "        jsonb_build_object(",
      "            'code', ${2:422},",
      "            'identifier', '${3:error_identifier}',",
      "            'message', '${4:Error message}',",
      "            'details', jsonb_build_object('field', '${5:field_name}')",
      "        )",
      "    )",
      ");",
      "RETURN result;"
    ],
    "description": "Error with explicit errors array"
  },

  "FraiseQL Build Error Object": {
    "prefix": "fraiseql-build-error",
    "body": [
      "build_error_object(",
      "    ${1:422},",
      "    '${2:identifier}',",
      "    '${3:Message}',",
      "    jsonb_build_object('field', '${4:field_name}')",
      ")"
    ],
    "description": "Build single error object helper"
  },

  "FraiseQL Not Found Check": {
    "prefix": "fraiseql-not-found",
    "body": [
      "SELECT * INTO ${1:record_var} FROM ${2:table_name} WHERE ${3:id} = ${4:value};",
      "IF NOT FOUND THEN",
      "    result.status := 'not_found:${5:resource}';",
      "    result.message := '${6:Resource not found}';",
      "    RETURN result;",
      "END IF;"
    ],
    "description": "Not found check pattern"
  },

  "FraiseQL Duplicate Check": {
    "prefix": "fraiseql-duplicate",
    "body": [
      "IF EXISTS (SELECT 1 FROM ${1:table_name} WHERE ${2:column} = ${3:value}) THEN",
      "    result.status := 'conflict:duplicate_${4:field}';",
      "    result.message := '${5:Resource already exists}';",
      "    RETURN result;",
      "END IF;"
    ],
    "description": "Duplicate check pattern"
  },

  "FraiseQL Validation Assert": {
    "prefix": "fraiseql-assert",
    "body": [
      "PERFORM mutation_assert(",
      "    ${1:validate_mutation_response(result)},",
      "    '${2:Validation failed}'",
      ");"
    ],
    "description": "Validation assertion"
  },

  "FraiseQL Input Extraction": {
    "prefix": "fraiseql-extract",
    "body": [
      "${1:variable_name} := input_payload->>'${2:field_name}';"
    ],
    "description": "Extract field from input_payload"
  },

  "FraiseQL Success Response": {
    "prefix": "fraiseql-success",
    "body": [
      "result.status := '${1|created,updated,deleted,success|}';",
      "result.message := '${2:Operation successful}';",
      "result.entity := row_to_json(${3:NEW});",
      "result.entity_id := ${4:NEW.id}::text;",
      "result.entity_type := '${5:EntityType}';",
      "RETURN result;"
    ],
    "description": "Success response"
  },

  "FraiseQL Validation Errors Collection": {
    "prefix": "fraiseql-collect-errors",
    "body": [
      "DECLARE",
      "    validation_errors jsonb := '[]'::jsonb;",
      "BEGIN",
      "    -- Collect errors",
      "    IF ${1:condition} THEN",
      "        validation_errors := validation_errors || ",
      "            build_error_object(422, '${2:identifier}', '${3:message}', ",
      "                jsonb_build_object('field', '${4:field}'));",
      "    END IF;",
      "    ",
      "    -- Return if errors",
      "    IF jsonb_array_length(validation_errors) > 0 THEN",
      "        result.status := 'failed:validation';",
      "        result.message := format('%s validation errors', jsonb_array_length(validation_errors));",
      "        result.metadata := jsonb_build_object('errors', validation_errors);",
      "        RETURN result;",
      "    END IF;"
    ],
    "description": "Collect multiple validation errors"
  }
}
```

**README.md:**
```markdown
# FraiseQL Tools for VS Code

Productivity tools for writing FraiseQL mutations in PostgreSQL.

## Features

### Snippets

- `fraiseql-mutation` - Complete mutation function template
- `fraiseql-error-simple` - Simple error (Pattern 1)
- `fraiseql-error-explicit` - Explicit errors (Pattern 2)
- `fraiseql-not-found` - Not found check
- `fraiseql-duplicate` - Duplicate check
- `fraiseql-assert` - Validation assertion
- `fraiseql-extract` - Extract from input_payload
- `fraiseql-success` - Success response
- `fraiseql-collect-errors` - Multiple errors collection

### Autocomplete

Type `fraiseql-` in a .sql file to see all available snippets.

### Syntax Highlighting

Status strings are highlighted:
- `'created'` - Green
- `'failed:validation'` - Red
- `'not_found:user'` - Orange
- `'conflict:duplicate'` - Yellow

## Installation

### From VSIX
1. Download `fraiseql-tools-1.0.0.vsix`
2. VS Code → Extensions → ... → Install from VSIX

### From Marketplace
1. Search "FraiseQL Tools"
2. Click Install

## Usage

1. Open .sql file
2. Type `fraiseql-` to see snippets
3. Use Tab to navigate placeholders
4. Use dropdown for status string options

## Examples

### Create Mutation Function
```sql
-- Type: fraiseql-mutation
-- Result: Complete function template
```

### Add Validation
```sql
-- Type: fraiseql-error-simple
-- Result: Simple error return
```

### Collect Errors
```sql
-- Type: fraiseql-collect-errors
-- Result: Error collection pattern
```

## Documentation

- [Mutation SQL Requirements](https://github.com/fraiseql/fraiseql/docs/guides/mutation-sql-requirements.md)
- [Quick Reference](https://github.com/fraiseql/fraiseql/docs/quick-reference/mutations-cheat-sheet.md)

## Feedback

Issues: https://github.com/fraiseql/fraiseql/issues
```

### Acceptance Criteria

- [ ] 10+ useful snippets covering common patterns
- [ ] Snippets have intelligent placeholders and dropdown options
- [ ] Tab navigation between placeholders
- [ ] README with examples
- [ ] Published to VS Code Marketplace
- [ ] Tested on Windows, Mac, Linux

### Verification

```bash
# Package extension
cd .vscode-extension/fraiseql
vsce package

# Test locally
code --install-extension fraiseql-tools-1.0.0.vsix

# Test snippets
# 1. Open .sql file
# 2. Type 'fraiseql-mutation'
# 3. Press Tab
# 4. Verify template appears with placeholders
```

---

## Task 5: Real-World Mutation Examples

**Assignee:** TW-CORE + ENG-CORE
**Time:** 4 hours
**Priority:** P1

### Objective

Create repository of 10+ real-world mutation patterns developers can copy.

### Implementation

**Directory:** `examples/mutation-patterns/`

**Structure:**
```
examples/mutation-patterns/
├── README.md                           # Index and overview
├── 01-basic-crud/
│   ├── create-user.sql
│   ├── update-user.sql
│   ├── delete-user.sql
│   └── README.md
├── 02-validation/
│   ├── simple-validation.sql          # Pattern 1
│   ├── multiple-field-validation.sql   # Pattern 2
│   ├── custom-validation-rules.sql
│   └── README.md
├── 03-business-logic/
│   ├── conditional-update.sql          # Optimistic locking
│   ├── state-machine.sql               # Status transitions
│   ├── calculated-fields.sql
│   └── README.md
├── 04-relationships/
│   ├── create-with-children.sql
│   ├── update-with-cascade.sql
│   ├── delete-with-cascade.sql
│   └── README.md
├── 05-error-handling/
│   ├── not-found.sql
│   ├── conflict-duplicate.sql
│   ├── permission-denied.sql
│   └── README.md
├── 06-advanced/
│   ├── bulk-operations.sql
│   ├── transaction-rollback.sql
│   ├── async-processing.sql
│   └── README.md
├── schema.sql                          # Test schema
└── test-all.sh                         # Test runner
```

**examples/mutation-patterns/README.md:**
```markdown
# FraiseQL Mutation Patterns

Real-world mutation examples you can copy and adapt.

## Quick Index

| Pattern | File | Use Case |
|---------|------|----------|
| **Basic CRUD** |
| Create | [01-basic-crud/create-user.sql](01-basic-crud/create-user.sql) | Simple INSERT |
| Update | [01-basic-crud/update-user.sql](01-basic-crud/update-user.sql) | Simple UPDATE |
| Delete | [01-basic-crud/delete-user.sql](01-basic-crud/delete-user.sql) | Simple DELETE |
| **Validation** |
| Simple | [02-validation/simple-validation.sql](02-validation/simple-validation.sql) | Single error (Pattern 1) |
| Multiple Fields | [02-validation/multiple-field-validation.sql](02-validation/multiple-field-validation.sql) | Multiple errors (Pattern 2) |
| Custom Rules | [02-validation/custom-validation-rules.sql](02-validation/custom-validation-rules.sql) | Business rules |
| **Business Logic** |
| Conditional Update | [03-business-logic/conditional-update.sql](03-business-logic/conditional-update.sql) | Optimistic locking |
| State Machine | [03-business-logic/state-machine.sql](03-business-logic/state-machine.sql) | Valid transitions |
| Calculated Fields | [03-business-logic/calculated-fields.sql](03-business-logic/calculated-fields.sql) | Auto-compute |
| **Relationships** |
| Create + Children | [04-relationships/create-with-children.sql](04-relationships/create-with-children.sql) | Nested creates |
| Update CASCADE | [04-relationships/update-with-cascade.sql](04-relationships/update-with-cascade.sql) | Related updates |
| Delete CASCADE | [04-relationships/delete-with-cascade.sql](04-relationships/delete-with-cascade.sql) | Cascade deletes |
| **Error Handling** |
| Not Found | [05-error-handling/not-found.sql](05-error-handling/not-found.sql) | 404 errors |
| Duplicate | [05-error-handling/conflict-duplicate.sql](05-error-handling/conflict-duplicate.sql) | Unique violations |
| Permission | [05-error-handling/permission-denied.sql](05-error-handling/permission-denied.sql) | Auth/authz |
| **Advanced** |
| Bulk Operations | [06-advanced/bulk-operations.sql](06-advanced/bulk-operations.sql) | Array inputs |
| Rollback | [06-advanced/transaction-rollback.sql](06-advanced/transaction-rollback.sql) | Error recovery |
| Async | [06-advanced/async-processing.sql](06-advanced/async-processing.sql) | Queue jobs |

## Setup

```bash
# Create test database
createdb fraiseql_patterns

# Load schema
psql fraiseql_patterns < schema.sql

# Test all examples
./test-all.sh
```

## Usage

Each example is standalone and copy-paste ready:

1. Read the example SQL file
2. Adapt variable names and table names
3. Copy into your project
4. Test with psql

## Contributing

Have a useful pattern? Submit a PR with:
- SQL file with comments
- Test case showing usage
- README section explaining the pattern
```

**Example File:** `examples/mutation-patterns/02-validation/multiple-field-validation.sql`

```sql
-- ============================================================================
-- Pattern: Multiple Field Validation (Pattern 2)
-- ============================================================================
-- Use Case: Validate multiple fields and return all errors at once
-- Benefits: Better UX (show all errors), easier form field mapping
--
-- This example shows:
-- - Collecting multiple validation errors
-- - Using build_error_object() helper
-- - Returning explicit errors in metadata.errors
-- - Proper error structure for frontend consumption
-- ============================================================================

CREATE OR REPLACE FUNCTION create_user_with_validation(input_payload jsonb)
RETURNS mutation_response AS $$
DECLARE
    result mutation_response;
    validation_errors jsonb := '[]'::jsonb;

    -- Input variables
    user_email text := input_payload->>'email';
    user_name text := input_payload->>'name';
    user_age int := (input_payload->>'age')::int;
    user_password text := input_payload->>'password';
BEGIN
    -- ========================================================================
    -- Collect All Validation Errors
    -- ========================================================================

    -- Email validation
    IF user_email IS NULL OR trim(user_email) = '' THEN
        validation_errors := validation_errors ||
            build_error_object(
                422,
                'email_required',
                'Email address is required',
                jsonb_build_object('field', 'email', 'constraint', 'required')
            );
    ELSIF user_email !~ '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$' THEN
        validation_errors := validation_errors ||
            build_error_object(
                422,
                'email_invalid_format',
                'Email format is invalid',
                jsonb_build_object(
                    'field', 'email',
                    'constraint', 'format',
                    'example', 'user@example.com'
                )
            );
    END IF;

    -- Name validation
    IF user_name IS NULL OR trim(user_name) = '' THEN
        validation_errors := validation_errors ||
            build_error_object(
                422,
                'name_required',
                'Full name is required',
                jsonb_build_object('field', 'name', 'constraint', 'required')
            );
    ELSIF length(user_name) < 2 THEN
        validation_errors := validation_errors ||
            build_error_object(
                422,
                'name_too_short',
                'Name must be at least 2 characters',
                jsonb_build_object(
                    'field', 'name',
                    'constraint', 'minLength',
                    'minLength', 2,
                    'actualLength', length(user_name)
                )
            );
    END IF;

    -- Age validation
    IF user_age IS NULL THEN
        validation_errors := validation_errors ||
            build_error_object(
                422,
                'age_required',
                'Age is required',
                jsonb_build_object('field', 'age', 'constraint', 'required')
            );
    ELSIF user_age < 13 THEN
        validation_errors := validation_errors ||
            build_error_object(
                422,
                'age_too_young',
                'Must be at least 13 years old',
                jsonb_build_object(
                    'field', 'age',
                    'constraint', 'minimum',
                    'minimum', 13,
                    'actual', user_age
                )
            );
    ELSIF user_age > 150 THEN
        validation_errors := validation_errors ||
            build_error_object(
                422,
                'age_unrealistic',
                'Age seems unrealistic',
                jsonb_build_object(
                    'field', 'age',
                    'constraint', 'maximum',
                    'maximum', 150,
                    'actual', user_age
                )
            );
    END IF;

    -- Password validation
    IF user_password IS NULL OR trim(user_password) = '' THEN
        validation_errors := validation_errors ||
            build_error_object(
                422,
                'password_required',
                'Password is required',
                jsonb_build_object('field', 'password', 'constraint', 'required')
            );
    ELSIF length(user_password) < 8 THEN
        validation_errors := validation_errors ||
            build_error_object(
                422,
                'password_too_short',
                'Password must be at least 8 characters',
                jsonb_build_object(
                    'field', 'password',
                    'constraint', 'minLength',
                    'minLength', 8,
                    'actualLength', length(user_password)
                )
            );
    END IF;

    -- ========================================================================
    -- Return Validation Errors if Any
    -- ========================================================================

    IF jsonb_array_length(validation_errors) > 0 THEN
        result.status := 'failed:validation';
        result.message := format('%s validation error(s)', jsonb_array_length(validation_errors));
        result.metadata := jsonb_build_object('errors', validation_errors);

        -- Optional: Validate response structure in debug mode
        PERFORM mutation_assert(
            validate_mutation_response(result),
            'Validation response structure invalid'
        );

        RETURN result;
    END IF;

    -- ========================================================================
    -- Check Business Rules
    -- ========================================================================

    -- Duplicate email check
    IF EXISTS (SELECT 1 FROM users WHERE email = user_email) THEN
        result.status := 'conflict:duplicate_email';
        result.message := 'Email address already registered';
        result.metadata := jsonb_build_object(
            'errors', jsonb_build_array(
                build_error_object(
                    409,
                    'duplicate_email',
                    'This email is already registered',
                    jsonb_build_object('field', 'email', 'value', user_email)
                )
            )
        );
        RETURN result;
    END IF;

    -- ========================================================================
    -- Create User
    -- ========================================================================

    INSERT INTO users (email, name, age, password_hash)
    VALUES (
        user_email,
        user_name,
        user_age,
        crypt(user_password, gen_salt('bf'))  -- bcrypt hash
    )
    RETURNING * INTO user_record;

    -- ========================================================================
    -- Success Response
    -- ========================================================================

    result.status := 'created';
    result.message := 'User created successfully';
    result.entity := row_to_json(user_record);
    result.entity_id := user_record.id::text;
    result.entity_type := 'User';

    -- Validate success response
    PERFORM mutation_assert(
        validate_mutation_response(result),
        'Success response structure invalid'
    );

    RETURN result;

EXCEPTION
    WHEN OTHERS THEN
        result.status := 'failed:error';
        result.message := SQLERRM;
        RETURN result;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- Usage Examples
-- ============================================================================

-- Valid input - Success
SELECT * FROM create_user_with_validation('{
  "email": "john@example.com",
  "name": "John Doe",
  "age": 25,
  "password": "secure123"
}'::jsonb);
-- Returns: status='created', entity with user data

-- Invalid input - Multiple errors
SELECT * FROM create_user_with_validation('{
  "email": "invalid-email",
  "name": "J",
  "age": 10,
  "password": "short"
}'::jsonb);
-- Returns: status='failed:validation', metadata.errors with 4 errors

-- Duplicate email
SELECT * FROM create_user_with_validation('{
  "email": "john@example.com",
  "name": "Jane Doe",
  "age": 30,
  "password": "secure456"
}'::jsonb);
-- Returns: status='conflict:duplicate_email'

-- ============================================================================
-- GraphQL Response Example
-- ============================================================================

/*
{
  "data": {
    "createUser": {
      "__typename": "CreateUserError",
      "code": 422,
      "status": "failed:validation",
      "message": "4 validation error(s)",
      "errors": [
        {
          "code": 422,
          "identifier": "email_invalid_format",
          "message": "Email format is invalid",
          "details": {
            "field": "email",
            "constraint": "format",
            "example": "user@example.com"
          }
        },
        {
          "code": 422,
          "identifier": "name_too_short",
          "message": "Name must be at least 2 characters",
          "details": {
            "field": "name",
            "constraint": "minLength",
            "minLength": 2,
            "actualLength": 1
          }
        },
        {
          "code": 422,
          "identifier": "age_too_young",
          "message": "Must be at least 13 years old",
          "details": {
            "field": "age",
            "constraint": "minimum",
            "minimum": 13,
            "actual": 10
          }
        },
        {
          "code": 422,
          "identifier": "password_too_short",
          "message": "Password must be at least 8 characters",
          "details": {
            "field": "password",
            "constraint": "minLength",
            "minLength": 8,
            "actualLength": 5
          }
        }
      ]
    }
  }
}
*/

-- ============================================================================
-- Frontend Usage (TypeScript/React)
-- ============================================================================

/*
// Map errors to form fields
const fieldErrors = response.errors.reduce((acc, err) => {
  if (err.details?.field) {
    acc[err.details.field] = err.message;
  }
  return acc;
}, {});

// Display in form
<input
  name="email"
  error={fieldErrors.email}  // "Email format is invalid"
/>
<input
  name="name"
  error={fieldErrors.name}   // "Name must be at least 2 characters"
/>
<input
  name="age"
  error={fieldErrors.age}    // "Must be at least 13 years old"
/>
<input
  name="password"
  error={fieldErrors.password} // "Password must be at least 8 characters"
/>
*/
```

### Additional Examples to Create

1. **01-basic-crud/create-user.sql** - Minimal create
2. **01-basic-crud/update-user.sql** - Simple update
3. **01-basic-crud/delete-user.sql** - Soft delete
4. **02-validation/simple-validation.sql** - Pattern 1
5. **02-validation/custom-validation-rules.sql** - Business rules
6. **03-business-logic/conditional-update.sql** - Optimistic locking
7. **03-business-logic/state-machine.sql** - Status transitions
8. **04-relationships/create-with-children.sql** - Nested inserts
9. **05-error-handling/not-found.sql** - 404 handling
10. **05-error-handling/permission-denied.sql** - Auth/authz

### Acceptance Criteria

- [ ] 10+ real-world examples covering common patterns
- [ ] Each example has:
  - [ ] Complete SQL function
  - [ ] Usage examples
  - [ ] Expected GraphQL response
  - [ ] Frontend integration example
- [ ] All examples tested and working
- [ ] schema.sql creates test tables
- [ ] test-all.sh runs all examples
- [ ] README with quick index

### Verification

```bash
# Run all examples
cd examples/mutation-patterns
./test-all.sh
# Should show: "All 10 examples passed!"

# Test individual example
psql < 02-validation/multiple-field-validation.sql

# Verify each returns expected response
psql -c "SELECT * FROM create_user_with_validation('{\"email\": \"test\", ...}'::jsonb);"
```

---

## Summary & Timeline

| Task | Hours | Priority | Owner |
|------|-------|----------|-------|
| 1. Quick Reference Cheat Sheet | 3 | P0 | TW-CORE |
| 2. Troubleshooting Guide | 3 | P0 | TW-CORE |
| 3. SQL Validation Helpers | 4 | P1 | ENG-CORE |
| 4. VS Code Extension | 6 | P1 | ENG-CORE |
| 5. Real-World Examples | 4 | P1 | TW-CORE + ENG-CORE |
| **Total** | **20** | | |

### Execution Order

**Week 1:**
- Day 1-2: Task 1 (Cheat Sheet) + Task 2 (Troubleshooting) - TW-CORE
- Day 3: Task 3 (SQL Validation) - ENG-CORE
- Day 4-5: Task 4 (VS Code Extension) - ENG-CORE

**Week 2:**
- Day 1-2: Task 5 (Examples) - TW-CORE + ENG-CORE
- Day 3: Integration testing, polish, documentation
- Day 4: Release

### Release Checklist

**Before Release:**
- [ ] All 5 tasks complete and tested
- [ ] Documentation cross-linked
- [ ] Examples tested on PostgreSQL 14, 15, 16
- [ ] VS Code extension tested on Windows/Mac/Linux
- [ ] Peer review by 2 developers (1 junior, 1 senior)

**Release Artifacts:**
- [ ] `docs/quick-reference/mutations-cheat-sheet.md`
- [ ] `docs/guides/troubleshooting-mutations.md`
- [ ] `sql/helpers/mutation_validation.sql`
- [ ] VS Code extension published
- [ ] `examples/mutation-patterns/` with 10+ examples

**Announcement:**
- [ ] Blog post highlighting DX improvements
- [ ] Twitter/social media
- [ ] Update main README
- [ ] Add to CHANGELOG under "Developer Experience"

---

## Success Metrics

**Target:**
- ⏱️ 10x faster pattern lookup (measured: time to find validation example)
- 🐛 50% reduction in "How do I...?" issues on GitHub
- 💻 70% adoption of VS Code extension (tracked via downloads)
- 📚 80% of developers say examples helped them (survey)

**Measurement:**
- Track GitHub issue tags: "documentation", "how-to", "examples"
- VS Code Marketplace download stats
- User survey after 1 month: "Rate DX improvements 1-10"
- Analytics on doc page views (cheat sheet, troubleshooting)

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| VS Code extension bugs | Medium | Low | Extensive testing, beta release first |
| Examples don't cover edge cases | Low | Medium | User feedback loop, add examples iteratively |
| Documentation gets out of sync | Medium | Medium | CI checks for broken links, version docs |
| SQL validation false positives | Low | High | Comprehensive tests, optional/warning mode |

---

## Post-Release

**Within 1 week:**
- Monitor GitHub issues for feedback
- Fix any critical bugs in VS Code extension
- Add missing examples based on requests

**Within 1 month:**
- Survey users on DX improvements
- Analyze which examples are most popular
- Plan next iteration based on feedback

**Within 3 months:**
- Add more advanced examples (transactions, security)
- Expand VS Code extension (diagnostics, linting)
- Create video tutorials using examples
