-- Migration: Add mutation_response type and helper functions
-- Description: Creates PostgreSQL composite type and helper functions for consistent mutation results
-- Version: 0.1.0
-- Date: 2025-01-25

-- =====================================================
-- MUTATION RESPONSE TYPE AND HELPERS
-- =====================================================

-- Create the mutation_response composite type
-- This provides a standardized format for all mutation responses
CREATE TYPE mutation_response AS (
    status          text,                    -- Status: 'success', 'new', 'updated', 'deleted', 'noop:*', 'failed:*'
    message         text,                    -- Human-readable message
    entity_id       text,                    -- Optional entity ID (for updates/deletes)
    entity_type     text,                    -- Optional entity type name (e.g., 'User', 'Post')
    entity          jsonb,                   -- The entity data (for success cases)
    updated_fields  text[],                  -- Fields that were updated (for partial updates)
    cascade         jsonb,                   -- Cascade data for side effects
    metadata        jsonb                    -- Additional metadata
);

-- =====================================================
-- SUCCESS RESULT HELPERS
-- =====================================================

-- Create a success result with full entity
CREATE OR REPLACE FUNCTION mutation_success(
    message_text text,
    entity_data jsonb,
    entity_type_name text DEFAULT NULL,
    cascade_data jsonb DEFAULT NULL,
    metadata_data jsonb DEFAULT NULL
) RETURNS mutation_response AS $$
DECLARE
    entity_id_val text;
BEGIN
    -- Extract ID from entity if present
    entity_id_val := entity_data->>'id';

    RETURN ROW(
        'success'::text,
        message_text,
        entity_id_val,
        entity_type_name,
        entity_data,
        NULL::text[],  -- No specific updated fields for full success
        cascade_data,
        metadata_data
    )::mutation_response;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Create a "new" result (for insertions)
CREATE OR REPLACE FUNCTION mutation_created(
    message_text text,
    entity_data jsonb,
    entity_type_name text DEFAULT NULL,
    cascade_data jsonb DEFAULT NULL,
    metadata_data jsonb DEFAULT NULL
) RETURNS mutation_response AS $$
DECLARE
    entity_id_val text;
BEGIN
    entity_id_val := entity_data->>'id';

    RETURN ROW(
        'new'::text,
        message_text,
        entity_id_val,
        entity_type_name,
        entity_data,
        NULL::text[],
        cascade_data,
        metadata_data
    )::mutation_response;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Create an "updated" result with specific changed fields
CREATE OR REPLACE FUNCTION mutation_updated(
    message_text text,
    entity_data jsonb,
    updated_fields_list text[],
    entity_type_name text DEFAULT NULL,
    cascade_data jsonb DEFAULT NULL,
    metadata_data jsonb DEFAULT NULL
) RETURNS mutation_response AS $$
DECLARE
    entity_id_val text;
BEGIN
    entity_id_val := entity_data->>'id';

    RETURN ROW(
        'updated'::text,
        message_text,
        entity_id_val,
        entity_type_name,
        entity_data,
        updated_fields_list,
        cascade_data,
        metadata_data
    )::mutation_response;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Create a "deleted" result
CREATE OR REPLACE FUNCTION mutation_deleted(
    message_text text,
    entity_id_val text,
    entity_type_name text DEFAULT NULL,
    cascade_data jsonb DEFAULT NULL,
    metadata_data jsonb DEFAULT NULL
) RETURNS mutation_response AS $$
BEGIN
    RETURN ROW(
        'deleted'::text,
        message_text,
        entity_id_val,
        entity_type_name,
        NULL::jsonb,  -- No entity data for deletions
        NULL::text[],
        cascade_data,
        metadata_data
    )::mutation_response;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- =====================================================
-- NO-OP RESULT HELPERS
-- =====================================================

-- Create a no-op result (no changes made)
CREATE OR REPLACE FUNCTION mutation_noop(
    reason text,
    message_text text DEFAULT NULL,
    metadata_data jsonb DEFAULT NULL
) RETURNS mutation_response AS $$
BEGIN
    RETURN ROW(
        'noop:' || reason,
        COALESCE(message_text, 'No changes made: ' || reason),
        NULL::text,
        NULL::text,
        NULL::jsonb,
        NULL::text[],
        NULL::jsonb,
        metadata_data
    )::mutation_response;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- =====================================================
-- ERROR RESULT HELPERS
-- =====================================================

-- Create a validation error
CREATE OR REPLACE FUNCTION mutation_validation_error(
    message_text text,
    field_name text DEFAULT NULL,
    metadata_data jsonb DEFAULT NULL
) RETURNS mutation_response AS $$
DECLARE
    error_metadata jsonb;
BEGIN
    -- Add field information to metadata if provided
    IF field_name IS NOT NULL THEN
        error_metadata := jsonb_build_object(
            'field', field_name,
            'type', 'validation'
        );
        IF metadata_data IS NOT NULL THEN
            error_metadata := error_metadata || metadata_data;
        END IF;
    ELSE
        error_metadata := metadata_data;
    END IF;

    RETURN ROW(
        'failed:validation'::text,
        message_text,
        NULL::text,
        NULL::text,
        NULL::jsonb,
        NULL::text[],
        NULL::jsonb,
        error_metadata
    )::mutation_response;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Create a not found error
CREATE OR REPLACE FUNCTION mutation_not_found(
    resource_type text DEFAULT 'Resource',
    resource_id text DEFAULT NULL,
    metadata_data jsonb DEFAULT NULL
) RETURNS mutation_response AS $$
DECLARE
    message_text text;
    error_metadata jsonb;
BEGIN
    IF resource_id IS NOT NULL THEN
        message_text := resource_type || ' with ID ' || resource_id || ' not found';
    ELSE
        message_text := resource_type || ' not found';
    END IF;

    error_metadata := jsonb_build_object('resource_type', resource_type);
    IF resource_id IS NOT NULL THEN
        error_metadata := error_metadata || jsonb_build_object('resource_id', resource_id);
    END IF;
    IF metadata_data IS NOT NULL THEN
        error_metadata := error_metadata || metadata_data;
    END IF;

    RETURN ROW(
        'failed:not_found'::text,
        message_text,
        NULL::text,
        NULL::text,
        NULL::jsonb,
        NULL::text[],
        NULL::jsonb,
        error_metadata
    )::mutation_response;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Create a conflict/duplicate error
CREATE OR REPLACE FUNCTION mutation_conflict(
    message_text text,
    conflict_type text DEFAULT 'duplicate',
    metadata_data jsonb DEFAULT NULL
) RETURNS mutation_response AS $$
DECLARE
    error_metadata jsonb;
BEGIN
    error_metadata := jsonb_build_object('conflict_type', conflict_type);
    IF metadata_data IS NOT NULL THEN
        error_metadata := error_metadata || metadata_data;
    END IF;

    RETURN ROW(
        'failed:' || conflict_type,
        message_text,
        NULL::text,
        NULL::text,
        NULL::jsonb,
        NULL::text[],
        NULL::jsonb,
        error_metadata
    )::mutation_response;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Create a generic error
CREATE OR REPLACE FUNCTION mutation_error(
    error_type text,
    message_text text,
    metadata_data jsonb DEFAULT NULL
) RETURNS mutation_response AS $$
BEGIN
    RETURN ROW(
        'failed:' || error_type,
        message_text,
        NULL::text,
        NULL::text,
        NULL::jsonb,
        NULL::text[],
        NULL::jsonb,
        metadata_data
    )::mutation_response;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- =====================================================
-- UTILITY FUNCTIONS
-- =====================================================

-- Check if a mutation result is successful
CREATE OR REPLACE FUNCTION mutation_is_success(result mutation_response) RETURNS boolean AS $$
BEGIN
    RETURN result.status NOT LIKE 'failed:%' AND result.status NOT LIKE 'noop:%';
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Check if a mutation result is an error
CREATE OR REPLACE FUNCTION mutation_is_error(result mutation_response) RETURNS boolean AS $$
BEGIN
    RETURN result.status LIKE 'failed:%';
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Check if a mutation result is a no-op
CREATE OR REPLACE FUNCTION mutation_is_noop(result mutation_response) RETURNS boolean AS $$
BEGIN
    RETURN result.status LIKE 'noop:%';
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Get error type from a failed status (e.g., 'validation' from 'failed:validation')
CREATE OR REPLACE FUNCTION mutation_error_type(result mutation_response) RETURNS text AS $$
BEGIN
    IF result.status LIKE 'failed:%' THEN
        RETURN substring(result.status from 8); -- Remove 'failed:' prefix
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Get noop reason from a noop status (e.g., 'unchanged' from 'noop:unchanged')
CREATE OR REPLACE FUNCTION mutation_noop_reason(result mutation_response) RETURNS text AS $$
BEGIN
    IF result.status LIKE 'noop:%' THEN
        RETURN substring(result.status from 6); -- Remove 'noop:' prefix
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- =====================================================
-- CASCADE DATA HELPERS
-- =====================================================
-- Per GraphQL Cascade spec (02_cascade_model.md):
-- - UpdatedEntity: { __typename, id, operation, entity }
-- - DeletedEntity: { __typename, id, deleted_at }
-- - CascadeMetadata: { timestamp, transaction_id?, depth, affected_count }
-- - CascadeOperation: CREATED | UPDATED | DELETED
--
-- NOTE: SQL uses snake_case. Rust transforms to camelCase for GraphQL.
-- Exception: __typename stays as-is (GraphQL introspection field)

-- Create cascade data for entity creation (operation = CREATED)
-- Per spec: created entities go in 'updated' array with operation='CREATED'
CREATE OR REPLACE FUNCTION cascade_entity_created(
    entity_type text,
    entity_id text,
    entity_data jsonb DEFAULT NULL
) RETURNS jsonb AS $$
BEGIN
    RETURN jsonb_build_object(
        'updated', jsonb_build_array(
            jsonb_build_object(
                '__typename', entity_type,
                'id', entity_id,
                'operation', 'CREATED',
                'entity', COALESCE(entity_data, '{}'::jsonb)
            )
        )
    );
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Create cascade data for entity updates (operation = UPDATED)
CREATE OR REPLACE FUNCTION cascade_entity_update(
    entity_type text,
    entity_id text,
    entity_data jsonb
) RETURNS jsonb AS $$
BEGIN
    RETURN jsonb_build_object(
        'updated', jsonb_build_array(
            jsonb_build_object(
                '__typename', entity_type,
                'id', entity_id,
                'operation', 'UPDATED',
                'entity', entity_data
            )
        )
    );
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Create cascade data for updated counts (convenience wrapper)
-- Use when only updating a count field on a related entity
CREATE OR REPLACE FUNCTION cascade_count_update(
    entity_type text,
    entity_id text,
    field_name text,
    previous_value integer,
    current_value integer
) RETURNS jsonb AS $$
BEGIN
    RETURN jsonb_build_object(
        'updated', jsonb_build_array(
            jsonb_build_object(
                '__typename', entity_type,
                'id', entity_id,
                'operation', 'UPDATED',
                'entity', jsonb_build_object(
                    'id', entity_id,
                    field_name, current_value,
                    '_previous_' || field_name, previous_value
                )
            )
        )
    );
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Create cascade data for entity deletion
-- Per spec: DeletedEntity requires deleted_at timestamp
CREATE OR REPLACE FUNCTION cascade_entity_deleted(
    entity_type text,
    entity_id text,
    deleted_at timestamptz DEFAULT NOW()
) RETURNS jsonb AS $$
BEGIN
    RETURN jsonb_build_object(
        'deleted', jsonb_build_array(
            jsonb_build_object(
                '__typename', entity_type,
                'id', entity_id,
                'deleted_at', to_jsonb(deleted_at)
            )
        )
    );
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Create cascade data for cache invalidation
CREATE OR REPLACE FUNCTION cascade_invalidate_cache(
    query_names text[],
    strategy text DEFAULT 'INVALIDATE'
) RETURNS jsonb AS $$
BEGIN
    RETURN jsonb_build_object(
        'invalidations', (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'query_name', query_name,
                    'strategy', strategy
                )
            )
            FROM unnest(query_names) AS query_name
        )
    );
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Create cascade metadata
-- Per spec: CascadeMetadata { timestamp, transaction_id?, depth, affected_count }
CREATE OR REPLACE FUNCTION cascade_metadata(
    affected_count integer,
    depth integer DEFAULT 1,
    transaction_id text DEFAULT NULL
) RETURNS jsonb AS $$
BEGIN
    RETURN jsonb_build_object(
        'metadata', jsonb_build_object(
            'timestamp', to_jsonb(NOW()),
            'transaction_id', transaction_id,
            'depth', depth,
            'affected_count', affected_count
        )
    );
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Merge multiple cascade data objects
-- Per spec: CascadeUpdates has { updated, deleted, invalidations, metadata }
CREATE OR REPLACE FUNCTION cascade_merge(cascade1 jsonb, cascade2 jsonb) RETURNS jsonb AS $$
DECLARE
    result jsonb := coalesce(cascade1, '{}'::jsonb);
BEGIN
    -- Merge updated arrays (includes CREATED and UPDATED operations)
    IF cascade2 ? 'updated' THEN
        result := jsonb_set(
            result,
            '{updated}',
            coalesce(result->'updated', '[]'::jsonb) || coalesce(cascade2->'updated', '[]'::jsonb)
        );
    END IF;

    -- Merge deleted arrays
    IF cascade2 ? 'deleted' THEN
        result := jsonb_set(
            result,
            '{deleted}',
            coalesce(result->'deleted', '[]'::jsonb) || coalesce(cascade2->'deleted', '[]'::jsonb)
        );
    END IF;

    -- Merge invalidations arrays
    IF cascade2 ? 'invalidations' THEN
        result := jsonb_set(
            result,
            '{invalidations}',
            coalesce(result->'invalidations', '[]'::jsonb) || coalesce(cascade2->'invalidations', '[]'::jsonb)
        );
    END IF;

    -- Merge metadata (last one wins for scalar values, sum for counts)
    IF cascade2 ? 'metadata' THEN
        IF result ? 'metadata' THEN
            -- Combine metadata: use latest timestamp, sum affected counts
            result := jsonb_set(
                result,
                '{metadata}',
                jsonb_build_object(
                    'timestamp', cascade2->'metadata'->'timestamp',
                    'transaction_id', COALESCE(cascade2->'metadata'->'transaction_id', result->'metadata'->'transaction_id'),
                    'depth', GREATEST(
                        COALESCE((result->'metadata'->>'depth')::int, 1),
                        COALESCE((cascade2->'metadata'->>'depth')::int, 1)
                    ),
                    'affected_count', COALESCE((result->'metadata'->>'affected_count')::int, 0)
                                   + COALESCE((cascade2->'metadata'->>'affected_count')::int, 0)
                )
            );
        ELSE
            result := jsonb_set(result, '{metadata}', cascade2->'metadata');
        END IF;
    END IF;

    RETURN result;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Check if cascade data contains specific entity type
-- Per spec: only 'updated' and 'deleted' arrays contain entities
CREATE OR REPLACE FUNCTION cascade_has_entity_type(cascade_data jsonb, entity_type text) RETURNS boolean AS $$
BEGIN
    RETURN (
        (cascade_data->'updated' @> jsonb_build_array(jsonb_build_object('__typename', entity_type))) OR
        (cascade_data->'deleted' @> jsonb_build_array(jsonb_build_object('__typename', entity_type)))
    );
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- =====================================================
-- EXAMPLE USAGE
-- =====================================================

/*
-- Example: Create user mutation with cascade
CREATE OR REPLACE FUNCTION graphql.create_user(input jsonb)
RETURNS mutation_response AS $$
DECLARE
    user_data jsonb;
    user_id uuid;
    cascade_data jsonb;
BEGIN
    -- Check if email exists
    IF EXISTS (SELECT 1 FROM users WHERE email = input->>'email') THEN
        RETURN mutation_conflict('Email address already exists', 'duplicate',
            jsonb_build_object('field', 'email', 'value', input->>'email'));
    END IF;

    -- Create user
    user_id := gen_random_uuid();
    INSERT INTO users (id, name, email, created_at)
    VALUES (user_id, input->>'name', input->>'email', now());

    -- Build response entity
    user_data := jsonb_build_object(
        'id', user_id,
        'name', input->>'name',
        'email', input->>'email',
        'created_at', to_jsonb(now())
    );

    -- Build cascade data: update user count on organization
    cascade_data := cascade_count_update(
        'Organization',
        input->>'organization_id',
        'user_count',
        5,  -- previous count (would be queried)
        6   -- new count
    );

    RETURN mutation_created(
        'User created successfully',
        user_data,
        'User',
        cascade_data
    );
END;
$$ LANGUAGE plpgsql;

-- Example: Create post with complex cascade
CREATE OR REPLACE FUNCTION graphql.create_post(input jsonb)
RETURNS mutation_response AS $$
DECLARE
    post_data jsonb;
    post_id uuid;
    cascade_data jsonb;
    author_cascade jsonb;
    tag_cascade jsonb;
BEGIN
    -- Create post
    post_id := gen_random_uuid();
    INSERT INTO posts (id, title, content, author_id, created_at)
    VALUES (post_id, input->>'title', input->>'content', (input->>'author_id')::uuid, now());

    -- Build response entity
    post_data := jsonb_build_object(
        'id', post_id,
        'title', input->>'title',
        'content', input->>'content',
        'author_id', input->>'author_id',
        'created_at', to_jsonb(now())
    );

    -- Cascade 1: Update author's post count
    author_cascade := cascade_count_update(
        'User',
        input->>'author_id',
        'post_count',
        10, 11
    );

    -- Cascade 2: Create tag associations (if tags provided)
    IF input ? 'tags' AND jsonb_array_length(input->'tags') > 0 THEN
        tag_cascade := jsonb_build_object(
            'created', (
                SELECT jsonb_agg(
                    jsonb_build_object(
                        '__typename', 'PostTag',
                        'post_id', post_id,
                        'tag_id', tag_id,
                        'tagged_at', to_jsonb(now())
                    )
                )
                FROM jsonb_array_elements_text(input->'tags') AS tag_id
            )
        );
    END IF;

    -- Merge all cascade data
    cascade_data := cascade_merge(author_cascade, tag_cascade);

    -- Add cache invalidation
    cascade_data := cascade_merge(
        cascade_data,
        cascade_invalidate_cache(ARRAY['posts', 'user_posts'], 'INVALIDATE')
    );

    RETURN mutation_created(
        'Post created successfully',
        post_data,
        'Post',
        cascade_data,
        jsonb_build_object('word_count', json_length(post_data->'content', '$.words'))
    );
END;
$$ LANGUAGE plpgsql;

-- Example: Update user with cascade
CREATE OR REPLACE FUNCTION graphql.update_user(user_id uuid, input jsonb)
RETURNS mutation_response AS $$
DECLARE
    updated_fields text[] := ARRAY[]::text[];
    user_data jsonb;
    current_user record;
    cascade_data jsonb;
BEGIN
    -- Get current user
    SELECT * INTO current_user FROM users WHERE id = user_id;
    IF NOT FOUND THEN
        RETURN mutation_not_found('User', user_id::text);
    END IF;

    -- Check for email uniqueness if email is being updated
    IF input ? 'email' AND input->>'email' != current_user.email THEN
        IF EXISTS (SELECT 1 FROM users WHERE email = input->>'email' AND id != user_id) THEN
            RETURN mutation_validation_error('Email address already exists', 'email');
        END IF;
        UPDATE users SET email = input->>'email' WHERE id = user_id;
        updated_fields := array_append(updated_fields, 'email');
    END IF;

    -- Update name if provided
    IF input ? 'name' AND input->>'name' != current_user.name THEN
        UPDATE users SET name = input->>'name' WHERE id = user_id;
        updated_fields := array_append(updated_fields, 'name');
    END IF;

    -- Check if anything was updated
    IF array_length(updated_fields, 1) = 0 THEN
        RETURN mutation_noop('unchanged', 'No fields were updated');
    END IF;

    -- Update timestamp
    UPDATE users SET updated_at = now() WHERE id = user_id;

    -- Return updated user data
    SELECT jsonb_build_object(
        'id', id,
        'name', name,
        'email', email,
        'updated_at', to_jsonb(updated_at)
    ) INTO user_data FROM users WHERE id = user_id;

    -- Create cascade data for cache invalidation
    cascade_data := cascade_invalidate_cache(ARRAY['user_profile', 'users'], 'INVALIDATE');

    RETURN mutation_updated(
        'User updated successfully',
        user_data,
        updated_fields,
        'User',
        cascade_data
    );
END;
$$ LANGUAGE plpgsql;
*/
