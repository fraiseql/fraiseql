-- GraphQL Cascade Example Schema
-- This demonstrates cascade functionality with a simple blog

-- Create schema
CREATE SCHEMA IF NOT EXISTS graphql;

-- Users table
CREATE TABLE tb_user (
    pk_user INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    post_count INT DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT now()
);

-- Posts table
CREATE TABLE tb_post (
    pk_post INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    content TEXT,
    author_id UUID NOT NULL REFERENCES tb_user(id),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT now()
);

-- Indexes
CREATE UNIQUE INDEX idx_user_id ON tb_user(id);
CREATE UNIQUE INDEX idx_post_id ON tb_post(id);
CREATE INDEX idx_post_author ON tb_post(author_id);

-- Views for cascade (must have 'data' column with JSONB)
CREATE VIEW v_user AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'name', name,
        'post_count', post_count,
        'created_at', created_at
    ) as data
FROM tb_user;

CREATE VIEW v_post AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'title', title,
        'content', content,
        'author_id', author_id,
        'created_at', created_at
    ) as data
FROM tb_post;

-- Sample data
INSERT INTO tb_user (name) VALUES ('Alice'), ('Bob');

-- Cascade helper functions (from migration)
CREATE SCHEMA IF NOT EXISTS app;

CREATE OR REPLACE FUNCTION app.cascade_entity(
    p_typename TEXT,
    p_id UUID,
    p_operation TEXT,
    p_view_name TEXT
) RETURNS JSONB AS $$
DECLARE
    v_entity_data JSONB;
BEGIN
    EXECUTE format('SELECT data FROM %I WHERE id = $1', p_view_name)
    INTO v_entity_data
    USING p_id;

    RETURN jsonb_build_object(
        '__typename', p_typename,
        'id', p_id,
        'operation', p_operation,
        'entity', v_entity_data
    );
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION app.cascade_invalidation(
    p_query_name TEXT,
    p_strategy TEXT,
    p_scope TEXT
) RETURNS JSONB AS $$
BEGIN
    RETURN jsonb_build_object(
        'queryName', p_query_name,
        'strategy', p_strategy,
        'scope', p_scope
    );
END;
$$ LANGUAGE plpgsql;

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

CREATE OR REPLACE FUNCTION app.build_cascade(
    p_updated JSONB DEFAULT '[]'::jsonb,
    p_deleted JSONB DEFAULT '[]'::jsonb,
    p_invalidations JSONB DEFAULT '[]'::jsonb,
    p_metadata JSONB DEFAULT NULL
) RETURNS JSONB AS $$
DECLARE
    v_metadata JSONB;
BEGIN
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

-- Mutation function with cascade
CREATE OR REPLACE FUNCTION graphql.create_post(input jsonb)
RETURNS jsonb AS $$
DECLARE
    v_post_id uuid;
    v_author_id uuid;
    v_cascade jsonb;
BEGIN
    -- Validate input
    IF input->>'title' IS NULL OR trim(input->>'title') = '' THEN
        RETURN jsonb_build_object(
            'success', false,
            'error', jsonb_build_object(
                'code', 'VALIDATION_ERROR',
                'message', 'Title is required',
                'field', 'title'
            )
        );
    END IF;

    IF input->>'author_id' IS NULL THEN
        RETURN jsonb_build_object(
            'success', false,
            'error', jsonb_build_object(
                'code', 'VALIDATION_ERROR',
                'message', 'Author ID is required',
                'field', 'author_id'
            )
        );
    END IF;

    -- Check if author exists
    v_author_id := (input->>'author_id')::uuid;
    IF NOT EXISTS (SELECT 1 FROM tb_user WHERE id = v_author_id) THEN
        RETURN jsonb_build_object(
            'success', false,
            'error', jsonb_build_object(
                'code', 'NOT_FOUND',
                'message', 'Author not found',
                'field', 'author_id'
            )
        );
    END IF;

    -- Create post
    INSERT INTO tb_post (title, content, author_id)
    VALUES (
        trim(input->>'title'),
        trim(input->>'content'),
        v_author_id
    )
    RETURNING id INTO v_post_id;

    -- Update author stats
    UPDATE tb_user SET post_count = post_count + 1 WHERE id = v_author_id;

    -- Build cascade data using helper functions
    v_cascade := app.build_cascade(
        updated => jsonb_build_array(
            app.cascade_entity('Post', v_post_id, 'CREATED', 'v_post'),
            app.cascade_entity('User', v_author_id, 'UPDATED', 'v_user')
        ),
        invalidations => jsonb_build_array(
            app.cascade_invalidation('posts', 'INVALIDATE', 'PREFIX'),
            app.cascade_invalidation('userPosts', 'INVALIDATE', 'EXACT')
        )
    );

    -- Return success with cascade
    RETURN jsonb_build_object(
        'success', true,
        'data', jsonb_build_object(
            'id', v_post_id,
            'message', 'Post created successfully'
        ),
        '_cascade', v_cascade
    );
END;
$$ LANGUAGE plpgsql;

-- Query functions
CREATE OR REPLACE FUNCTION graphql.get_posts()
RETURNS jsonb AS $$
BEGIN
    RETURN jsonb_build_object(
        'success', true,
        'data', (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'id', p.id,
                    'title', p.title,
                    'content', p.content,
                    'author', jsonb_build_object(
                        'id', u.id,
                        'name', u.name
                    ),
                    'created_at', p.created_at
                )
            )
            FROM tb_post p
            JOIN tb_user u ON p.author_id = u.id
            ORDER BY p.created_at DESC
        )
    );
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION graphql.get_user(input jsonb)
RETURNS jsonb AS $$
DECLARE
    v_user_id uuid;
BEGIN
    v_user_id := (input->>'id')::uuid;

    RETURN jsonb_build_object(
        'success', true,
        'data', (
            SELECT jsonb_build_object(
                'id', u.id,
                'name', u.name,
                'post_count', u.post_count,
                'posts', (
                    SELECT jsonb_agg(
                        jsonb_build_object(
                            'id', p.id,
                            'title', p.title,
                            'created_at', p.created_at
                        )
                    )
                    FROM tb_post p
                    WHERE p.author_id = u.id
                    ORDER BY p.created_at DESC
                )
            )
            FROM tb_user u
            WHERE u.id = v_user_id
        )
    );
END;
$$ LANGUAGE plpgsql;
