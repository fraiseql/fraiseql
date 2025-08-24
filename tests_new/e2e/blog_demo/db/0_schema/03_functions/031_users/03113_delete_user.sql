-- Delete User CRUD Functions
-- Following PrintOptim app/core pattern

-- ===========================================================================
-- APP LAYER: Simple wrapper
-- ===========================================================================
CREATE OR REPLACE FUNCTION app.delete_user(
    input_pk_user UUID,
    input_deleted_by UUID
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
BEGIN
    RETURN core.delete_user(input_pk_user, input_deleted_by);
END;
$$;

-- ===========================================================================
-- CORE LAYER: Business logic implementation
-- ===========================================================================
CREATE OR REPLACE FUNCTION core.delete_user(
    input_pk_user UUID,
    input_deleted_by UUID
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
DECLARE
    v_entity TEXT := 'user';
    v_fields TEXT[] := ARRAY['deleted'];

    v_payload_before JSONB;
    v_existing_user tb_user;

    v_op TEXT;
    v_status TEXT;
    v_message TEXT;
    v_reason TEXT;
    v_extra_metadata JSONB;
BEGIN
    -- Check if user exists and get current state
    SELECT * INTO v_existing_user
    FROM tb_user
    WHERE pk_user = input_pk_user;

    IF NOT FOUND THEN
        v_op := 'NOOP';
        v_status := 'noop:not_found';
        v_message := 'User not found.';
        v_reason := 'entity_not_found';
        v_fields := ARRAY[]::TEXT[];
        v_extra_metadata := jsonb_build_object(
            'trigger', 'api_delete',
            'status', v_status,
            'reason', v_reason,
            'input_id', input_pk_user
        );

        RETURN core.log_and_return_mutation(
            input_deleted_by,
            v_entity,
            input_pk_user,
            v_op,
            v_status,
            v_fields,
            v_message,
            NULL,
            NULL,
            v_extra_metadata
        );
    END IF;

    -- Get current payload before deletion
    SELECT row_to_json(v) INTO v_payload_before
    FROM v_user v
    WHERE v.id = input_pk_user;

    -- Check if user has posts (prevent deletion if they have content)
    IF EXISTS (SELECT 1 FROM tb_post WHERE fk_author = input_pk_user) THEN
        v_op := 'NOOP';
        v_status := 'noop:has_dependencies';
        v_message := 'Cannot delete user with existing posts.';
        v_reason := 'referential_constraint';
        v_fields := ARRAY[]::TEXT[];
        v_extra_metadata := jsonb_build_object(
            'trigger', 'api_delete',
            'status', v_status,
            'reason', v_reason,
            'constraint_type', 'posts_exist'
        );

        RETURN core.log_and_return_mutation(
            input_deleted_by,
            v_entity,
            input_pk_user,
            v_op,
            v_status,
            v_fields,
            v_message,
            v_payload_before,
            v_payload_before,
            v_extra_metadata
        );
    END IF;

    -- Delete user (CASCADE will handle comments)
    DELETE FROM tb_user
    WHERE pk_user = input_pk_user;

    v_op := 'DELETE';
    v_status := 'deleted';
    v_message := 'User deleted successfully.';
    v_reason := 'entity_deleted';
    v_extra_metadata := jsonb_build_object(
        'trigger', 'api_delete',
        'status', v_status,
        'reason', v_reason,
        'deleted_id', input_pk_user
    );

    RETURN core.log_and_return_mutation(
        input_deleted_by,
        v_entity,
        input_pk_user,
        v_op,
        v_status,
        v_fields,
        v_message,
        v_payload_before,
        NULL,
        v_extra_metadata
    );
END;
$$;
