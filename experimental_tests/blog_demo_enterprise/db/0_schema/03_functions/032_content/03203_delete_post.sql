-- Delete Post Function (Multi-tenant)
-- Delete blog post with cascade handling for tenant isolation

CREATE OR REPLACE FUNCTION app.delete_post(
    input_pk_post UUID,
    input_pk_organization UUID,
    input_deleted_by UUID
) RETURNS app.mutation_result AS $$
DECLARE
    v_existing_post tenant.tb_post;
    v_comments_count INTEGER;
    v_tags_count INTEGER;
BEGIN
    -- Validate tenant context and check if post exists
    SELECT * INTO v_existing_post
    FROM tenant.tb_post
    WHERE pk_post = input_pk_post
      AND fk_organization = input_pk_organization;

    IF v_existing_post.pk_post IS NULL THEN
        RETURN app.log_and_return_mutation(
            false,
            'Post not found or access denied',
            NULL,
            'POST_NOT_FOUND',
            jsonb_build_object(
                'post_id', input_pk_post,
                'organization_id', input_pk_organization
            ),
            'delete_post',
            input_deleted_by,
            input_pk_organization
        );
    END IF;

    -- Check associated content counts
    SELECT COUNT(*) INTO v_comments_count
    FROM tenant.tb_comment
    WHERE fk_post = input_pk_post
      AND fk_organization = input_pk_organization;

    SELECT COUNT(*) INTO v_tags_count
    FROM tenant.tb_post_tag
    WHERE fk_post = input_pk_post
      AND fk_organization = input_pk_organization;

    -- Delete associated post-tag relationships first
    DELETE FROM tenant.tb_post_tag
    WHERE fk_post = input_pk_post
      AND fk_organization = input_pk_organization;

    -- Delete associated comments (cascade)
    DELETE FROM tenant.tb_comment
    WHERE fk_post = input_pk_post
      AND fk_organization = input_pk_organization;

    -- Delete the post
    DELETE FROM tenant.tb_post
    WHERE pk_post = input_pk_post
      AND fk_organization = input_pk_organization;

    -- Return success with metadata about what was deleted
    RETURN app.log_and_return_mutation(
        true,
        'Post deleted successfully',
        jsonb_build_object(
            'deleted_id', input_pk_post,
            'deleted_comments_count', v_comments_count,
            'deleted_tag_associations_count', v_tags_count,
            'title', v_existing_post.data->>'title',
            'slug', v_existing_post.identifier
        ),
        NULL,
        jsonb_build_object(
            'post_id', input_pk_post,
            'organization_id', input_pk_organization,
            'cascade_deleted', jsonb_build_object(
                'comments', v_comments_count,
                'tag_associations', v_tags_count
            )
        ),
        'delete_post',
        input_deleted_by,
        input_pk_organization
    );

EXCEPTION
    WHEN OTHERS THEN
        RETURN app.log_and_return_mutation(
            false,
            'Failed to delete post: ' || SQLERRM,
            NULL,
            'INTERNAL_ERROR',
            jsonb_build_object(
                'sql_error', SQLERRM,
                'sql_state', SQLSTATE
            ),
            'delete_post',
            input_deleted_by,
            input_pk_organization
        );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
