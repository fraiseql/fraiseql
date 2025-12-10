-- ============================================================================
-- Pattern: Permission/Authorization Errors
-- ============================================================================
-- Use Case: Enforce access control in mutations
-- Benefits: Centralized authz, clear error messages, audit trail
--
-- This example shows:
-- - Role-based access control (RBAC)
-- - Resource ownership checks
-- - Permission error responses
-- - Audit logging of denied attempts
-- ============================================================================

CREATE OR REPLACE FUNCTION delete_post_with_authz(input_payload jsonb)
RETURNS mutation_response AS $$
DECLARE
    result mutation_response;
    post_record record;
    post_id uuid := (input_payload->>'id')::uuid;
    user_id uuid := (input_payload->>'user_id')::uuid;
    user_role text;
    is_author boolean;
BEGIN
    -- ========================================================================
    -- Validate Input
    -- ========================================================================

    IF post_id IS NULL THEN
        result.status := 'failed:validation';
        result.message := 'Post ID is required';
        RETURN result;
    END IF;

    IF user_id IS NULL THEN
        result.status := 'unauthorized:missing_auth';
        result.message := 'Authentication required';
        RETURN result;
    END IF;

    -- ========================================================================
    -- Find Post
    -- ========================================================================

    SELECT * INTO post_record FROM posts WHERE id = post_id;
    IF NOT FOUND THEN
        result.status := 'not_found:post';
        result.message := 'Post not found';
        RETURN result;
    END IF;

    -- ========================================================================
    -- Check Authorization
    -- ========================================================================

    -- Get user's role
    SELECT role INTO user_role FROM users WHERE id = user_id;
    IF user_role IS NULL THEN
        result.status := 'not_found:user';
        result.message := 'User not found';
        RETURN result;
    END IF;

    -- Check if user is the author
    is_author := post_record.author_id = user_id;

    -- Authorization rules:
    -- 1. Admins can delete any post
    -- 2. Moderators can delete any post
    -- 3. Authors can delete their own posts
    -- 4. Regular users cannot delete others' posts

    IF user_role = 'admin' THEN
        -- Admins can delete anything
        NULL;
    ELSIF user_role = 'moderator' THEN
        -- Moderators can delete anything
        NULL;
    ELSIF is_author THEN
        -- Authors can delete their own posts
        NULL;
    ELSE
        -- Permission denied
        result.status := 'forbidden:not_authorized';
        result.message := 'You do not have permission to delete this post';
        result.metadata := jsonb_build_object(
            'required_permission', 'delete_post',
            'user_role', user_role,
            'is_author', is_author,
            'post_id', post_id,
            'post_author_id', post_record.author_id
        );

        -- Audit log: Record denied attempt
        INSERT INTO audit_log (user_id, action, resource_type, resource_id, result, details)
        VALUES (
            user_id,
            'delete_post',
            'Post',
            post_id,
            'denied',
            jsonb_build_object('reason', 'not_authorized', 'user_role', user_role)
        );

        RETURN result;
    END IF;

    -- ========================================================================
    -- Perform Deletion
    -- ========================================================================

    DELETE FROM posts WHERE id = post_id;

    -- Audit log: Record successful deletion
    INSERT INTO audit_log (user_id, action, resource_type, resource_id, result, details)
    VALUES (
        user_id,
        'delete_post',
        'Post',
        post_id,
        'success',
        jsonb_build_object('user_role', user_role, 'is_author', is_author)
    );

    -- ========================================================================
    -- Success Response
    -- ========================================================================

    result.status := 'deleted';
    result.message := 'Post deleted successfully';
    result.entity_id := post_id::text;
    result.entity_type := 'Post';

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

-- Admin deletes any post (allowed)
SELECT * FROM delete_post_with_authz('{
    "id": "660e8400-e29b-41d4-a716-446655440000",
    "user_id": "550e8400-e29b-41d4-a716-446655440000"
}'::jsonb);
-- Returns: status='deleted' (if user is admin)

-- User deletes their own post (allowed)
SELECT * FROM delete_post_with_authz('{
    "id": "660e8400-e29b-41d4-a716-446655440000",
    "user_id": "660e8400-e29b-41d4-a716-446655440000"
}'::jsonb);
-- Returns: status='deleted' (if user is author)

-- User tries to delete someone else's post (denied)
SELECT * FROM delete_post_with_authz('{
    "id": "660e8400-e29b-41d4-a716-446655440000",
    "user_id": "770e8400-e29b-41d4-a716-446655440000"
}'::jsonb);
-- Returns: status='forbidden:not_authorized'
-- Error: "You do not have permission to delete this post"

-- Missing authentication
SELECT * FROM delete_post_with_authz('{
    "id": "660e8400-e29b-41d4-a716-446655440000"
}'::jsonb);
-- Returns: status='unauthorized:missing_auth'

-- ============================================================================
-- Advanced: Field-Level Permissions
-- ============================================================================

CREATE OR REPLACE FUNCTION update_user_with_field_authz(input_payload jsonb)
RETURNS mutation_response AS $$
DECLARE
    result mutation_response;
    target_user_id uuid := (input_payload->>'id')::uuid;
    current_user_id uuid := (input_payload->>'current_user_id')::uuid;
    current_user_role text;
    is_self boolean;
BEGIN
    SELECT role INTO current_user_role FROM users WHERE id = current_user_id;
    is_self := target_user_id = current_user_id;

    -- Check each field permission
    IF input_payload ? 'role' AND current_user_role != 'admin' THEN
        result.status := 'forbidden:insufficient_privileges';
        result.message := 'Only admins can change user roles';
        RETURN result;
    END IF;

    IF input_payload ? 'email' AND NOT is_self AND current_user_role != 'admin' THEN
        result.status := 'forbidden:not_authorized';
        result.message := 'You can only change your own email';
        RETURN result;
    END IF;

    -- Perform update...
    result.status := 'updated';
    result.message := 'User updated successfully';
    RETURN result;
END;
$$ LANGUAGE plpgsql;
