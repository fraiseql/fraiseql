-- ==============================================================================
-- TYPES: Mutation result type (FraiseQL standard)
-- ==============================================================================

-- Standard mutation result type matching FraiseQL's MutationResult dataclass
CREATE TYPE mutation_result AS (
    id UUID,
    updated_fields TEXT[],
    status TEXT,
    message TEXT,
    object_data JSONB,
    extra_metadata JSONB
);

-- ==============================================================================
-- HELPER FUNCTIONS: Slug generation and utilities
-- ==============================================================================

-- Function to generate slug from title
CREATE OR REPLACE FUNCTION generate_slug(input_text TEXT)
RETURNS TEXT AS $$
BEGIN
    RETURN lower(regexp_replace(
        regexp_replace(input_text, '[^a-zA-Z0-9\s]', '', 'g'),
        '\s+', '-', 'g'
    ));
END;
$$ LANGUAGE plpgsql;

-- ==============================================================================
-- MUTATION FUNCTIONS: Following printoptim pattern
-- Database layer handles validation and returns errors via mutation_result
-- ==============================================================================

-- Update post - checks existence and ownership
CREATE OR REPLACE FUNCTION update_post(
    input_post_id UUID,
    input_user_id UUID,
    input_payload JSONB
)
RETURNS mutation_result
LANGUAGE plpgsql
AS $$
DECLARE
    v_post_pk INT;
    v_author_pk INT;
    v_user_pk INT;
    v_updated_fields TEXT[] := ARRAY[]::TEXT[];
BEGIN
    -- Get user's internal PK
    SELECT pk_user INTO v_user_pk
    FROM tb_user
    WHERE id = input_user_id;

    IF NOT FOUND THEN
        RETURN ROW(
            NULL,
            NULL,
            'noop:user_not_found',
            'User not found',
            NULL,
            jsonb_build_object('user_id', input_user_id)
        )::mutation_result;
    END IF;

    -- Get post and check existence
    SELECT pk_post, fk_author INTO v_post_pk, v_author_pk
    FROM tb_post
    WHERE id = input_post_id;

    -- Database layer returns not found error
    IF NOT FOUND THEN
        RETURN ROW(
            input_post_id,
            NULL,
            'noop:not_found',
            'Post not found',
            NULL,
            jsonb_build_object('post_id', input_post_id)
        )::mutation_result;
    END IF;

    -- Check ownership
    IF v_author_pk != v_user_pk THEN
        RETURN ROW(
            input_post_id,
            NULL,
            'error:permission_denied',
            'You can only edit your own posts',
            NULL,
            jsonb_build_object('post_id', input_post_id, 'user_id', input_user_id)
        )::mutation_result;
    END IF;

    -- Perform updates
    IF input_payload ? 'title' THEN
        UPDATE tb_post SET title = input_payload->>'title' WHERE pk_post = v_post_pk;
        v_updated_fields := array_append(v_updated_fields, 'title');
    END IF;

    IF input_payload ? 'content' THEN
        UPDATE tb_post SET content = input_payload->>'content' WHERE pk_post = v_post_pk;
        v_updated_fields := array_append(v_updated_fields, 'content');
    END IF;

    IF input_payload ? 'status' THEN
        UPDATE tb_post SET status = input_payload->>'status' WHERE pk_post = v_post_pk;
        v_updated_fields := array_append(v_updated_fields, 'status');
    END IF;

    -- Always update timestamp
    UPDATE tb_post SET updated_at = NOW() WHERE pk_post = v_post_pk;

    -- Return success
    RETURN ROW(
        input_post_id,
        v_updated_fields,
        'success:updated',
        'Post updated successfully',
        (SELECT row_to_json(tb_post.*) FROM tb_post WHERE pk_post = v_post_pk),
        NULL
    )::mutation_result;
END;
$$;

-- Create comment - checks post existence
CREATE OR REPLACE FUNCTION create_comment(
    input_user_id UUID,
    input_payload JSONB
)
RETURNS mutation_result
LANGUAGE plpgsql
AS $$
DECLARE
    v_post_id UUID;
    v_post_pk INT;
    v_user_pk INT;
    v_comment_id UUID;
    v_comment_pk INT;
BEGIN
    -- Extract post_id from payload
    v_post_id := (input_payload->>'post_id')::UUID;

    -- Get user's internal PK
    SELECT pk_user INTO v_user_pk
    FROM tb_user
    WHERE id = input_user_id;

    IF NOT FOUND THEN
        RETURN ROW(
            NULL,
            NULL,
            'noop:user_not_found',
            'User not found',
            NULL,
            jsonb_build_object('user_id', input_user_id)
        )::mutation_result;
    END IF;

    -- Database layer handles post not found
    SELECT pk_post INTO v_post_pk
    FROM tb_post
    WHERE id = v_post_id;

    IF NOT FOUND THEN
        RETURN ROW(
            NULL,
            NULL,
            'noop:not_found',
            'Post not found',
            NULL,
            jsonb_build_object('post_id', v_post_id)
        )::mutation_result;
    END IF;

    -- Create comment
    INSERT INTO tb_comment (fk_post, fk_author, content, status)
    VALUES (
        v_post_pk,
        v_user_pk,
        input_payload->>'content',
        COALESCE(input_payload->>'status', 'pending')
    )
    RETURNING id, pk_comment INTO v_comment_id, v_comment_pk;

    -- Return success
    RETURN ROW(
        v_comment_id,
        ARRAY['content', 'status'],
        'success:created',
        'Comment created successfully',
        (SELECT row_to_json(tb_comment.*) FROM tb_comment WHERE pk_comment = v_comment_pk),
        NULL
    )::mutation_result;
END;
$$;

COMMENT ON FUNCTION update_post IS 'Update post with validation - returns not_found status if post does not exist or user lacks permission';
COMMENT ON FUNCTION create_comment IS 'Create comment with validation - returns not_found status if post does not exist';
