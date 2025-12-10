-- ============================================================================
-- Pattern: Conditional Update (Optimistic Locking)
-- ============================================================================
-- Use Case: Update only if record is in expected state
-- Benefits: Prevents race conditions, ensures data consistency
--
-- This example shows:
-- - Conditional UPDATE with WHERE clause
-- - Optimistic locking pattern
-- - No-op response when condition not met
-- - Atomic state transitions
-- ============================================================================

CREATE OR REPLACE FUNCTION start_machine(input_payload jsonb)
RETURNS mutation_response AS $$
DECLARE
    result mutation_response;
    machine_id uuid;
    machine_record record;
BEGIN
    -- ========================================================================
    -- Extract Input
    -- ========================================================================

    machine_id := (input_payload->>'id')::uuid;

    -- ========================================================================
    -- Conditional Update (Optimistic Locking)
    -- ========================================================================
    -- Only start the machine if it's currently idle
    -- This prevents starting an already running machine

    UPDATE users SET status = 'running', updated_at = now()
    WHERE id = machine_id AND status = 'idle'
    RETURNING * INTO machine_record;

    -- Check if update actually happened
    IF NOT FOUND THEN
        -- Either machine doesn't exist, or it's not idle
        SELECT * INTO machine_record FROM users WHERE id = machine_id;

        IF NOT FOUND THEN
            result.status := 'not_found:machine';
            result.message := format('Machine %s not found', machine_id);
            RETURN result;
        ELSE
            -- Machine exists but is not idle
            result.status := 'noop:already_running';
            result.message := 'Machine is already running';
            result.entity := row_to_json(machine_record);
            RETURN result;
        END IF;
    END IF;

    -- ========================================================================
    -- Success Response
    -- ========================================================================

    result.status := 'updated';
    result.message := 'Machine started successfully';
    result.entity := row_to_json(machine_record);
    result.entity_id := machine_record.id::text;
    result.entity_type := 'Machine';
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
-- Alternative: Version-based Optimistic Locking
-- ============================================================================

CREATE OR REPLACE FUNCTION update_user_optimistic(input_payload jsonb)
RETURNS mutation_response AS $$
DECLARE
    result mutation_response;
    user_id uuid;
    expected_version int;
    user_record record;
BEGIN
    user_id := (input_payload->>'id')::uuid;
    expected_version := (input_payload->>'version')::int;

    -- Update only if version matches (simulating optimistic locking)
    UPDATE users SET
        name = COALESCE(input_payload->>'name', name),
        email = COALESCE(input_payload->>'email', email),
        updated_at = now()
    WHERE id = user_id AND version = expected_version
    RETURNING * INTO user_record;

    IF NOT FOUND THEN
        -- Either user doesn't exist, or version mismatch
        SELECT * INTO user_record FROM users WHERE id = user_id;

        IF NOT FOUND THEN
            result.status := 'not_found:user';
            result.message := format('User %s not found', user_id);
            RETURN result;
        ELSE
            -- Version mismatch - concurrent modification
            result.status := 'conflict:concurrent_modification';
            result.message := 'User was modified by another process. Please refresh and try again.';
            result.entity := row_to_json(user_record); -- Current state
            RETURN result;
        END IF;
    END IF;

    result.status := 'updated';
    result.message := 'User updated successfully';
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

-- Start idle machine - success
SELECT * FROM start_machine('{
    "id": "550e8400-e29b-41d4-a716-446655440000"
}'::jsonb);
-- Returns: status='updated', message='Machine started successfully'

-- Try to start already running machine - noop
SELECT * FROM start_machine('{
    "id": "550e8400-e29b-41d4-a716-446655440000"
}'::jsonb);
-- Returns: status='noop:already_running', message='Machine is already running'

-- Try to start non-existent machine - not found
SELECT * FROM start_machine('{
    "id": "00000000-0000-0000-0000-000000000000"
}'::jsonb);
-- Returns: status='not_found:machine', message='Machine 00000000-0000-0000-0000-000000000000 not found'

-- ============================================================================
-- GraphQL Usage for Optimistic UI Updates
-- ============================================================================

/*
mutation StartMachine($id: ID!) {
  startMachine(input: { id: $id }) {
    ... on StartMachineSuccess {
      machine {
        id
        status
        updatedAt
      }
      updatedFields
    }
    ... on NoOpError {
      code
      identifier  # "already_running"
      message
      entity {    # Current state
        id
        status
      }
    }
  }
}
*/
