-- GraphQL Cascade Helper Functions
--
-- These functions help PostgreSQL mutation functions build cascade metadata
-- for GraphQL cache updates and side effect tracking.

BEGIN;

-- ============================================================================
-- CASCADE HELPER FUNCTIONS
-- ============================================================================

-- Create app schema if it doesn't exist
CREATE SCHEMA IF NOT EXISTS app;

-- Helper: Build cascade entity object
CREATE OR REPLACE FUNCTION app.cascade_entity(
    p_typename TEXT,
    p_id UUID,
    p_operation TEXT,  -- 'CREATED', 'UPDATED', 'DELETED'
    p_view_name TEXT   -- Name of the view to get entity data from
) RETURNS JSONB AS $$
DECLARE
    v_entity_data JSONB;
BEGIN
    -- Get entity data from view
    EXECUTE format('SELECT data FROM %I WHERE id = $1', p_view_name)
    INTO v_entity_data
    USING p_id;

    -- Return cascade entity structure
    RETURN jsonb_build_object(
        '__typename', p_typename,
        'id', p_id,
        'operation', p_operation,
        'entity', v_entity_data
    );
END;
$$ LANGUAGE plpgsql;

-- Helper: Build cascade invalidation hint
CREATE OR REPLACE FUNCTION app.cascade_invalidation(
    p_query_name TEXT,
    p_strategy TEXT,    -- 'INVALIDATE', 'REFETCH'
    p_scope TEXT        -- 'PREFIX', 'EXACT', 'ALL'
) RETURNS JSONB AS $$
BEGIN
    RETURN jsonb_build_object(
        'queryName', p_query_name,
        'strategy', p_strategy,
        'scope', p_scope
    );
END;
$$ LANGUAGE plpgsql;

-- Helper: Build cascade metadata
CREATE OR REPLACE FUNCTION app.cascade_metadata(
    p_affected_count INT DEFAULT 1
) RETURNS JSONB AS $$
BEGIN
    RETURN jsonb_build_object(
        'timestamp', now(),
        'affectedCount', p_affected_count
    );
END;
$$ LANGUAGE plpgsql;

-- Helper: Build complete cascade object
CREATE OR REPLACE FUNCTION app.build_cascade(
    p_updated JSONB DEFAULT '[]'::jsonb,
    p_deleted JSONB DEFAULT '[]'::jsonb,
    p_invalidations JSONB DEFAULT '[]'::jsonb,
    p_metadata JSONB DEFAULT NULL
) RETURNS JSONB AS $$
DECLARE
    v_metadata JSONB;
BEGIN
    -- Use provided metadata or generate default
    v_metadata := p_metadata;
    IF v_metadata IS NULL THEN
        v_metadata := app.cascade_metadata(
            (jsonb_array_length(p_updated) + jsonb_array_length(p_deleted))::INT
        );
    END IF;

    RETURN jsonb_build_object(
        'updated', p_updated,
        'deleted', p_deleted,
        'invalidations', p_invalidations,
        'metadata', v_metadata
    );
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- USAGE EXAMPLES
-- ============================================================================

/*
-- Example usage in a mutation function:

CREATE OR REPLACE FUNCTION graphql.create_post(input jsonb)
RETURNS jsonb AS $$
DECLARE
    v_post_id uuid;
    v_author_id uuid;
    v_cascade jsonb;
BEGIN
    -- Create post
    INSERT INTO tb_post (title, content, author_id)
    VALUES (input->>'title', input->>'content', (input->>'author_id')::uuid)
    RETURNING id INTO v_post_id;

    v_author_id := (input->>'author_id')::uuid;

    -- Update author stats
    UPDATE tb_user SET post_count = post_count + 1 WHERE id = v_author_id;

    -- Build cascade data
    v_cascade := app.build_cascade(
        updated => jsonb_build_array(
            app.cascade_entity('Post', v_post_id, 'CREATED', 'v_post'),
            app.cascade_entity('User', v_author_id, 'UPDATED', 'v_user')
        ),
        invalidations => jsonb_build_array(
            app.cascade_invalidation('posts', 'INVALIDATE', 'PREFIX')
        )
    );

    -- Return with cascade
    RETURN jsonb_build_object(
        'success', true,
        'data', jsonb_build_object('id', v_post_id, 'message', 'Post created'),
        '_cascade', v_cascade
    );
END;
$$ LANGUAGE plpgsql;
*/

COMMIT;
