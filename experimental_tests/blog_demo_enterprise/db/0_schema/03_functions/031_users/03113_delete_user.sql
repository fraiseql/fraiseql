-- Delete User Function (Multi-tenant)
-- Soft delete with tenant isolation

CREATE OR REPLACE FUNCTION app.delete_user(
    input_pk_user UUID,
    input_pk_organization UUID,
    input_deleted_by UUID
) RETURNS app.mutation_result AS $$
DECLARE
    v_existing_user tenant.tb_user;
    v_posts_count INTEGER;
    v_comments_count INTEGER;
BEGIN
    -- Validate tenant context and check if user exists
    SELECT * INTO v_existing_user
    FROM tenant.tb_user
    WHERE pk_user = input_pk_user
      AND fk_organization = input_pk_organization;

    IF v_existing_user.pk_user IS NULL THEN
        RETURN app.log_and_return_mutation(
            false,
            'User not found or access denied',
            NULL,
            'USER_NOT_FOUND',
            jsonb_build_object(
                'user_id', input_pk_user,
                'organization_id', input_pk_organization
            ),
            'delete_user',
            input_deleted_by,
            input_pk_organization
        );
    END IF;

    -- Check if user has associated content
    SELECT COUNT(*) INTO v_posts_count
    FROM tenant.tb_post
    WHERE fk_author = input_pk_user
      AND fk_organization = input_pk_organization;

    SELECT COUNT(*) INTO v_comments_count
    FROM tenant.tb_comment
    WHERE fk_author = input_pk_user
      AND fk_organization = input_pk_organization;

    -- If user has content, perform soft delete (deactivate)
    IF v_posts_count > 0 OR v_comments_count > 0 THEN
        UPDATE tenant.tb_user SET
            is_active = false,
            updated_by = input_deleted_by
        WHERE pk_user = input_pk_user
          AND fk_organization = input_pk_organization;

        RETURN app.log_and_return_mutation(
            true,
            'User deactivated successfully (user has associated content)',
            jsonb_build_object(
                'deleted_id', input_pk_user,
                'soft_delete', true,
                'posts_count', v_posts_count,
                'comments_count', v_comments_count
            ),
            NULL,
            jsonb_build_object(
                'user_id', input_pk_user,
                'organization_id', input_pk_organization,
                'deletion_type', 'soft_delete'
            ),
            'delete_user',
            input_deleted_by,
            input_pk_organization
        );
    ELSE
        -- Hard delete if no associated content
        DELETE FROM tenant.tb_user
        WHERE pk_user = input_pk_user
          AND fk_organization = input_pk_organization;

        RETURN app.log_and_return_mutation(
            true,
            'User deleted successfully',
            jsonb_build_object(
                'deleted_id', input_pk_user,
                'soft_delete', false
            ),
            NULL,
            jsonb_build_object(
                'user_id', input_pk_user,
                'organization_id', input_pk_organization,
                'deletion_type', 'hard_delete'
            ),
            'delete_user',
            input_deleted_by,
            input_pk_organization
        );
    END IF;

EXCEPTION
    WHEN OTHERS THEN
        RETURN app.log_and_return_mutation(
            false,
            'Failed to delete user: ' || SQLERRM,
            NULL,
            'INTERNAL_ERROR',
            jsonb_build_object(
                'sql_error', SQLERRM,
                'sql_state', SQLSTATE
            ),
            'delete_user',
            input_deleted_by,
            input_pk_organization
        );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
