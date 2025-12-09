-- ============================================================================
-- Pattern: Basic Delete (Simple DELETE)
-- ============================================================================
-- Use Case: Delete an existing record
-- Benefits: Clean removal with proper error handling
--
-- This example shows:
-- - Finding existing record before delete
-- - Soft delete vs hard delete options
-- - Proper success response for deletes
-- ============================================================================

CREATE OR REPLACE FUNCTION delete_user(input_payload jsonb)
RETURNS mutation_response AS $$
DECLARE
    result mutation_response;
    user_id uuid;
    user_record record;
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
    -- Delete User
    -- ========================================================================

    DELETE FROM users WHERE id = user_id;

    -- ========================================================================
    -- Success Response
    -- ========================================================================
    -- Note: For DELETE operations, entity is typically null since the record is gone

    result.status := 'deleted';
    result.message := 'User deleted successfully';
    result.entity_id := user_id::text;
    result.entity_type := 'User';
    -- entity is null for deletes (record is gone)

    RETURN result;

EXCEPTION
    WHEN OTHERS THEN
        result.status := 'failed:error';
        result.message := SQLERRM;
        RETURN result;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- Alternative: Soft Delete Pattern
-- ============================================================================

CREATE OR REPLACE FUNCTION soft_delete_user(input_payload jsonb)
RETURNS mutation_response AS $$
DECLARE
    result mutation_response;
    user_id uuid;
    user_record record;
BEGIN
    user_id := (input_payload->>'id')::uuid;

    -- Find existing record
    SELECT * INTO user_record FROM users WHERE id = user_id;
    IF NOT FOUND THEN
        result.status := 'not_found:user';
        result.message := format('User %s not found', user_id);
        RETURN result;
    END IF;

    -- Soft delete (mark as inactive)
    UPDATE users SET status = 'inactive', updated_at = now()
    WHERE id = user_id
    RETURNING * INTO user_record;

    -- Success response with updated entity
    result.status := 'updated';
    result.message := 'User deactivated successfully';
    result.entity := row_to_json(user_record);
    result.entity_id := user_record.id::text;
    result.entity_type := 'User';
    result.updated_fields := ARRAY['status'];

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

-- Hard delete
SELECT * FROM delete_user('{
    "id": "550e8400-e29b-41d4-a716-446655440000"
}'::jsonb);
-- Returns: status='deleted', entity=null, entity_id="550e8400-e29b-41d4-a716-446655440000"

-- Soft delete
SELECT * FROM soft_delete_user('{
    "id": "550e8400-e29b-41d4-a716-446655440001"
}'::jsonb);
-- Returns: status='updated', entity with status='inactive'

-- User not found
SELECT * FROM delete_user('{
    "id": "00000000-0000-0000-0000-000000000000"
}'::jsonb);
-- Returns: status='not_found:user'
