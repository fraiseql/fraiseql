-- ============================================================================
-- Pattern: Basic Create (Simple INSERT)
-- ============================================================================
-- Use Case: Create a new record with basic validation
-- Benefits: Straightforward, minimal boilerplate
--
-- This example shows:
-- - Basic input extraction
-- - Simple validation
-- - INSERT with RETURNING
-- - Success response
-- ============================================================================

CREATE OR REPLACE FUNCTION create_user(input_payload jsonb)
RETURNS mutation_response AS $$
DECLARE
    result mutation_response;
    user_email text;
    user_name text;
    user_record record;
BEGIN
    -- ========================================================================
    -- Extract Input
    -- ========================================================================

    user_email := input_payload->>'email';
    user_name := input_payload->>'name';

    -- ========================================================================
    -- Basic Validation
    -- ========================================================================

    IF user_email IS NULL OR trim(user_email) = '' THEN
        result.status := 'failed:validation';
        result.message := 'Email is required';
        RETURN result;
    END IF;

    IF user_name IS NULL OR trim(user_name) = '' THEN
        result.status := 'failed:validation';
        result.message := 'Name is required';
        RETURN result;
    END IF;

    -- ========================================================================
    -- Check for Duplicates
    -- ========================================================================

    IF EXISTS (SELECT 1 FROM users WHERE email = user_email) THEN
        result.status := 'conflict:duplicate_email';
        result.message := 'Email already registered';
        RETURN result;
    END IF;

    -- ========================================================================
    -- Create User
    -- ========================================================================

    INSERT INTO users (email, name)
    VALUES (user_email, user_name)
    RETURNING * INTO user_record;

    -- ========================================================================
    -- Success Response
    -- ========================================================================

    result.status := 'created';
    result.message := 'User created successfully';
    result.entity := row_to_json(user_record);
    result.entity_id := user_record.id::text;
    result.entity_type := 'User';

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

-- Success case
SELECT * FROM create_user('{
    "email": "alice@example.com",
    "name": "Alice Johnson"
}'::jsonb);
-- Returns: status='created', entity with user data

-- Validation error
SELECT * FROM create_user('{
    "email": "",
    "name": "Alice Johnson"
}'::jsonb);
-- Returns: status='failed:validation', message='Email is required'

-- Duplicate error
SELECT * FROM create_user('{
    "email": "john@example.com",
    "name": "John Doe"
}'::jsonb);
-- Returns: status='conflict:duplicate_email'
