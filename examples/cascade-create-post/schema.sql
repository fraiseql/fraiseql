-- CASCADE Create Post Example Schema
-- Demonstrates the basic CASCADE pattern: create an entity with side effects.
--
-- Run `fraiseql setup --database-url <url>` FIRST — it installs the shipped
-- builders this schema uses: fraiseql.mutation_ok / mutation_err and
-- fraiseql.build_cascade / cascade_entity / deleted_entity / cascade_invalidation.

CREATE SCHEMA IF NOT EXISTS graphql;
CREATE SCHEMA IF NOT EXISTS app;

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

CREATE UNIQUE INDEX idx_user_id ON tb_user(id);
CREATE UNIQUE INDEX idx_post_id ON tb_post(id);
CREATE INDEX idx_post_author ON tb_post(author_id);

-- Read views. The `data` JSONB uses snake_case source keys; FraiseQL projects
-- them to camelCase on the GraphQL surface (post_count → postCount). Cascade
-- entities are read from these views (never tb_*) so row-visibility follows RLS.
-- `security_invoker = true` is REQUIRED for that: it runs the view as the querying
-- role so base-table RLS applies; a default view would run as the owner and leak
-- cross-tenant rows into cascades.
CREATE VIEW v_user WITH (security_invoker = true) AS
SELECT id, jsonb_build_object(
    'id', id, 'name', name, 'post_count', post_count, 'created_at', created_at
) AS data
FROM tb_user;

CREATE VIEW v_post WITH (security_invoker = true) AS
SELECT id, jsonb_build_object(
    'id', id, 'title', title, 'content', content,
    'author_id', author_id, 'created_at', created_at
) AS data
FROM tb_post;

-- The mutation-response composite the runtime parses. Columns match
-- fraiseql.mutation_ok / mutation_err exactly (succeeded/state_changed/error_class
-- model, not a string status).
CREATE TYPE app.mutation_response AS (
    succeeded      boolean,
    state_changed  boolean,
    error_class    text,
    status_detail  text,
    http_status    smallint,
    message        text,
    entity_id      uuid,
    entity_type    text,
    entity         jsonb,
    updated_fields text[],
    cascade        jsonb,
    error_detail   jsonb,
    metadata       jsonb
);

-- Mutation function with CASCADE.
CREATE OR REPLACE FUNCTION graphql.create_post(input jsonb)
RETURNS SETOF app.mutation_response AS $$
DECLARE
    v_post_id   uuid;
    v_author_id uuid := (input->>'author_id')::uuid;
BEGIN
    -- Validate.
    IF input->>'title' IS NULL OR trim(input->>'title') = '' THEN
        RETURN QUERY SELECT * FROM fraiseql.mutation_err('validation', 'Title is required');
        RETURN;
    END IF;
    IF v_author_id IS NULL OR NOT EXISTS (SELECT 1 FROM tb_user WHERE id = v_author_id) THEN
        RETURN QUERY SELECT * FROM fraiseql.mutation_err('validation', 'Author not found', NULL, 422::smallint);
        RETURN;
    END IF;

    -- Create the post and bump the author's post count.
    INSERT INTO tb_post (title, content, author_id)
    VALUES (trim(input->>'title'), trim(input->>'content'), v_author_id)
    RETURNING id INTO v_post_id;

    UPDATE tb_user SET post_count = post_count + 1 WHERE id = v_author_id;

    -- Success + cascade, assembled from the RLS views with the shipped builders.
    RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
        p_entity      := (SELECT data FROM v_post WHERE id = v_post_id),
        p_entity_id   := v_post_id,
        p_entity_type := 'Post',
        p_cascade     := fraiseql.build_cascade(
            p_updated := jsonb_build_array(
                fraiseql.cascade_entity('Post', v_post_id,  'CREATED', 'v_post'),
                fraiseql.cascade_entity('User', v_author_id, 'UPDATED', 'v_user')
            ),
            p_invalidations := jsonb_build_array(
                fraiseql.cascade_invalidation('posts',     'INVALIDATE', 'PREFIX'),
                fraiseql.cascade_invalidation('userPosts', 'INVALIDATE', 'EXACT')
            )
        )
    );
END;
$$ LANGUAGE plpgsql;

-- Sample data
INSERT INTO tb_user (name) VALUES ('Alice'), ('Bob'), ('Charlie');
