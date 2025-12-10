-- ============================================================================
-- Pattern: State Machine Transitions
-- ============================================================================
-- Use Case: Valid state transitions with business rules
-- Benefits: Prevents invalid state changes, enforces business logic
--
-- This example shows:
-- - Valid state transition validation
-- - Business rule enforcement
-- - Clear error messages for invalid transitions
-- ============================================================================

CREATE OR REPLACE FUNCTION transition_post_status(input_payload jsonb)
RETURNS mutation_response AS $$
DECLARE
    result mutation_response;
    post_id uuid;
    new_status text;
    current_status text;
    post_record record;
BEGIN
    -- ========================================================================
    -- Extract Input
    -- ========================================================================

    post_id := (input_payload->>'id')::uuid;
    new_status := input_payload->>'status';

    -- ========================================================================
    -- Validate New Status
    -- ========================================================================

    IF new_status NOT IN ('draft', 'published', 'archived') THEN
        result.status := 'failed:validation';
        result.message := 'Invalid status. Must be: draft, published, or archived';
        RETURN result;
    END IF;

    -- ========================================================================
    -- Find Current Post
    -- ========================================================================

    SELECT * INTO post_record FROM posts WHERE id = post_id;
    IF NOT FOUND THEN
        result.status := 'not_found:post';
        result.message := format('Post %s not found', post_id);
        RETURN result;
    END IF;

    current_status := post_record.status;

    -- ========================================================================
    -- Validate State Transition
    -- ========================================================================

    -- Define valid transitions
    CASE
        WHEN current_status = 'draft' AND new_status = 'published' THEN
            -- Valid: draft → published
            NULL;
        WHEN current_status = 'draft' AND new_status = 'archived' THEN
            -- Valid: draft → archived
            NULL;
        WHEN current_status = 'published' AND new_status = 'archived' THEN
            -- Valid: published → archived
            NULL;
        WHEN current_status = 'published' AND new_status = 'draft' THEN
            -- Invalid: published → draft (can't unpublish)
            result.status := 'failed:invalid_transition';
            result.message := 'Cannot change published post back to draft';
            RETURN result;
        WHEN current_status = 'archived' THEN
            -- Invalid: archived posts cannot be changed
            result.status := 'failed:invalid_transition';
            result.message := 'Archived posts cannot be modified';
            RETURN result;
        WHEN current_status = new_status THEN
            -- No change needed
            result.status := 'noop:no_change';
            result.message := format('Post is already %s', current_status);
            result.entity := row_to_json(post_record);
            RETURN result;
        ELSE
            -- Any other transition is invalid
            result.status := 'failed:invalid_transition';
            result.message := format('Cannot transition from %s to %s', current_status, new_status);
            RETURN result;
    END CASE;

    -- ========================================================================
    -- Apply Transition
    -- ========================================================================

    UPDATE posts SET
        status = new_status,
        updated_at = now()
    WHERE id = post_id
    RETURNING * INTO post_record;

    -- ========================================================================
    -- Success Response
    -- ========================================================================

    result.status := 'updated';
    result.message := format('Post status changed from %s to %s', current_status, new_status);
    result.entity := row_to_json(post_record);
    result.entity_id := post_record.id::text;
    result.entity_type := 'Post';
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

-- Valid transition: draft → published
SELECT * FROM transition_post_status('{
    "id": "660e8400-e29b-41d4-a716-446655440001",
    "status": "published"
}'::jsonb);
-- Returns: status='updated', message='Post status changed from draft to published'

-- Invalid transition: published → draft
SELECT * FROM transition_post_status('{
    "id": "660e8400-e29b-41d4-a716-446655440000",
    "status": "draft"
}'::jsonb);
-- Returns: status='failed:invalid_transition', message='Cannot change published post back to draft'

-- Invalid transition: archived post
-- First archive a post, then try to change it
-- Returns: status='failed:invalid_transition', message='Archived posts cannot be modified'

-- No change needed
SELECT * FROM transition_post_status('{
    "id": "660e8400-e29b-41d4-a716-446655440000",
    "status": "published"
}'::jsonb);
-- Returns: status='noop:no_change', message='Post is already published'

-- Post not found
SELECT * FROM transition_post_status('{
    "id": "00000000-0000-0000-0000-000000000000",
    "status": "published"
}'::jsonb);
-- Returns: status='not_found:post'

-- ============================================================================
-- State Machine Diagram
-- ============================================================================

/*
Draft ──────→ Published ──────→ Archived
  │                │
  └───────────────→┘ (invalid)

Valid Transitions:
- draft → published
- draft → archived
- published → archived

Invalid Transitions:
- published → draft (can't unpublish)
- archived → * (immutable)
- archived → published
- archived → draft
*/
