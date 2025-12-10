-- ============================================================================
-- Pattern: Basic Update (Simple UPDATE)
-- ============================================================================
-- Use Case: Update an existing record
-- Benefits: Straightforward field updates
--
-- This example shows:
-- - Finding existing record
-- - UPDATE with RETURNING
-- - Tracking updated fields
-- - Success response
-- ============================================================================

CREATE OR REPLACE FUNCTION update_user(input_payload jsonb)
RETURNS mutation_response AS $$
DECLARE
    result mutation_response;
    user_id uuid;
    user_record record;
    updated_fields text[] := '{}';
BEGIN
    -- ========================================================================
    -- Extract Input
    -- ========================================================================

    user_id := (input_payload->>'id')::uuid;

    -- ========================================================================
    -- Find Existing Record
    -- ========================================================================

    SELECT * INTO user_record FROM users WHERE id = user_id;
    IF NOT FOUND THEN
        result.status := 'not_found:user';
        result.message := format('User %s not found', user_id);
        RETURN result;
    END IF;

    -- ========================================================================
    -- Update Fields (only if provided)
    -- ========================================================================

    -- Update email if provided
    IF input_payload ? 'email' AND input_payload->>'email' != user_record.email THEN
        UPDATE users SET email = input_payload->>'email', updated_at = now()
        WHERE id = user_id;
        updated_fields := updated_fields || 'email';
    END IF;

    -- Update name if provided
    IF input_payload ? 'name' AND input_payload->>'name' != user_record.name THEN
        UPDATE users SET name = input_payload->>'name', updated_at = now()
        WHERE id = user_id;
        updated_fields := updated_fields || 'name';
    END IF;

    -- Update age if provided
    IF input_payload ? 'age' THEN
        UPDATE users SET age = (input_payload->>'age')::int, updated_at = now()
        WHERE id = user_id;
        updated_fields := updated_fields || 'age';
    END IF;

    -- ========================================================================
    -- Get Updated Record
    -- ========================================================================

    SELECT * INTO user_record FROM users WHERE id = user_id;

    -- ========================================================================
    -- Success Response
    -- ========================================================================

    result.status := 'updated';
    result.message := 'User updated successfully';
    result.entity := row_to_json(user_record);
    result.entity_id := user_record.id::text;
    result.entity_type := 'User';
    result.updated_fields := updated_fields;

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

-- Update email and name
SELECT * FROM update_user('{
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "email": "john.doe@example.com",
    "name": "John Doe Jr"
}'::jsonb);
-- Returns: status='updated', updated_fields=['email', 'name']

-- Update only age
SELECT * FROM update_user('{
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "age": 35
}'::jsonb);
-- Returns: status='updated', updated_fields=['age']

-- User not found
SELECT * FROM update_user('{
    "id": "00000000-0000-0000-0000-000000000000",
    "name": "Ghost User"
}'::jsonb);
-- Returns: status='not_found:user'
