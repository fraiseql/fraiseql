-- ============================================================================
-- Pattern: Delete with CASCADE Reporting
-- ============================================================================
-- Use Case: Delete parent and report all cascade-deleted children
-- Benefits: Audit trail, undo capability, user awareness of consequences
--
-- This example shows:
-- - Soft delete vs hard delete
-- - Reporting cascade-deleted children
-- - Preventing accidental data loss
-- - Using CASCADE for transparency
-- ============================================================================

CREATE OR REPLACE FUNCTION delete_user_with_cascade(input_payload jsonb)
RETURNS mutation_response AS $$
DECLARE
    result mutation_response;
    user_record record;
    user_id uuid := (input_payload->>'id')::uuid;
    hard_delete boolean := COALESCE((input_payload->>'hard_delete')::boolean, false);
    deleted_posts jsonb;
    deleted_comments jsonb;
    posts_count int;
    comments_count int;
BEGIN
    -- ========================================================================
    -- Find User
    -- ========================================================================

    SELECT * INTO user_record FROM users WHERE id = user_id;
    IF NOT FOUND THEN
        result.status := 'not_found:user';
        result.message := 'User not found';
        RETURN result;
    END IF;

    -- ========================================================================
    -- Collect CASCADE Deletions
    -- ========================================================================

    -- Get posts that will be deleted
    SELECT jsonb_agg(row_to_json(p))
    INTO deleted_posts
    FROM posts p
    WHERE p.author_id = user_id;

    SELECT COUNT(*) INTO posts_count
    FROM posts WHERE author_id = user_id;

    -- Get comments that will be deleted
    SELECT jsonb_agg(row_to_json(c))
    INTO deleted_comments
    FROM comments c
    WHERE c.user_id = user_id;

    SELECT COUNT(*) INTO comments_count
    FROM comments WHERE user_id = user_id;

    -- ========================================================================
    -- Perform Delete (Soft or Hard)
    -- ========================================================================

    IF hard_delete THEN
        -- Hard delete: Permanent removal
        DELETE FROM comments WHERE user_id = user_id;
        DELETE FROM posts WHERE author_id = user_id;
        DELETE FROM users WHERE id = user_id;
    ELSE
        -- Soft delete: Mark as deleted (preserves data)
        UPDATE comments
        SET deleted_at = now()
        WHERE user_id = user_id AND deleted_at IS NULL;

        UPDATE posts
        SET deleted_at = now()
        WHERE author_id = user_id AND deleted_at IS NULL;

        UPDATE users
        SET deleted_at = now()
        WHERE id = user_id;
    END IF;

    -- ========================================================================
    -- Success Response
    -- ========================================================================

    result.status := 'deleted';
    result.message := format(
        'User %s deleted. Cascade: %s post(s), %s comment(s)',
        CASE WHEN hard_delete THEN 'permanently' ELSE 'soft' END,
        posts_count,
        comments_count
    );
    result.entity_id := user_id::text;
    result.entity_type := 'User';

    -- Report what was cascade-deleted
    result.cascade := jsonb_build_object(
        'deleted', jsonb_build_object(
            'posts', jsonb_build_object(
                'count', posts_count,
                'data', COALESCE(deleted_posts, '[]'::jsonb)
            ),
            'comments', jsonb_build_object(
                'count', comments_count,
                'data', COALESCE(deleted_comments, '[]'::jsonb)
            )
        ),
        'hard_delete', hard_delete
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

-- Soft delete (default)
SELECT * FROM delete_user_with_cascade('{
    "id": "550e8400-e29b-41d4-a716-446655440000"
}'::jsonb);
-- Returns: status='deleted', cascade.deleted.posts.count=5, cascade.deleted.comments.count=23
-- Data still in database with deleted_at timestamp

-- Hard delete (permanent)
SELECT * FROM delete_user_with_cascade('{
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "hard_delete": true
}'::jsonb);
-- Returns: status='deleted', cascade.deleted contains removed data
-- Data permanently removed from database
