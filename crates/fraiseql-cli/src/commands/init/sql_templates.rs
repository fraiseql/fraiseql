//! PostgreSQL DDL templates for the `fraiseql init` scaffold.
//!
//! The scaffold follows the runtime's contracts exactly, because the runtime
//! executes against what these templates create:
//!
//! - **Read path**: every `v_*` view exposes a `data` JSONB column built with `jsonb_build_object`
//!   — the adapter's only read shape is `SELECT data FROM "<view>"` (#823). Keys are snake_case
//!   **storage keys**: the runtime's canonical mapping (`fraiseql_db::utils::to_snake_case`)
//!   recases the schema's camelCase GraphQL field names onto them at every read site (projection,
//!   order-by, filters).
//! - **Write path**: every `fn_*` mutation function returns the 13-column v2.2 `mutation_response`
//!   row via the `fraiseql.mutation_ok` / `fraiseql.mutation_err` builders that `fraiseql setup`
//!   installs (#569).
//! - **Trinity naming**: `pk_<entity>` (internal), `id` (public UUID), `identifier` (URL slug);
//!   singular `tb_`/`v_` names.
//!
//! FraiseQL is PostgreSQL-only (non-PG backends were removed in v2.15.0,
//! #374), so these templates have no dialect parameter.

/// Generate a single-file schema for the XS size (blog project).
pub(super) fn generate_single_schema_sql() -> String {
    let mut sql = String::with_capacity(8192);
    sql.push_str(
        "-- FraiseQL Blog Schema (PostgreSQL)\n\
         -- Trinity pattern: pk (internal), id (public UUID), identifier (URL slug)\n\
         -- Views expose a `data` JSONB column — the read shape the runtime executes.\n\
         -- Mutation functions require `fraiseql setup` to have been run first\n\
         -- (installs fraiseql.mutation_ok/mutation_err and core.tb_entity_change_log).\n\n",
    );
    for entity in super::ENTITIES {
        let (table, view, functions) = generate_blog_entity_sql(entity);
        sql.push_str(&table);
        sql.push('\n');
        sql.push_str(&view);
        sql.push('\n');
        sql.push_str(&functions);
        sql.push('\n');
    }
    sql.push_str(JUNCTION_TABLE);
    sql
}

/// Generate per-entity SQL split into (table, view, functions) for S/M layouts.
pub(super) fn generate_blog_entity_sql(entity: &str) -> (String, String, String) {
    match entity {
        "author" => (
            ENTITY_AUTHOR_TABLE.to_string(),
            ENTITY_AUTHOR_VIEW.to_string(),
            author_functions(),
        ),
        "post" => (ENTITY_POST_TABLE.to_string(), ENTITY_POST_VIEW.to_string(), post_functions()),
        "comment" => (
            ENTITY_COMMENT_TABLE.to_string(),
            ENTITY_COMMENT_VIEW.to_string(),
            comment_functions(),
        ),
        "tag" => (ENTITY_TAG_TABLE.to_string(), ENTITY_TAG_VIEW.to_string(), tag_functions()),
        _ => (format!("-- Unknown entity: {entity}\n"), String::new(), String::new()),
    }
}

/// The 13-column v2.2 `mutation_response` row type every mutation function
/// returns. Matches `fraiseql.mutation_ok` / `fraiseql.mutation_err` exactly,
/// so function bodies can `RETURN QUERY SELECT * FROM fraiseql.mutation_ok(…)`.
const MUTATION_RESPONSE_COLUMNS: &str = "\
    succeeded BOOLEAN,
    state_changed BOOLEAN,
    error_class TEXT,
    status_detail TEXT,
    http_status SMALLINT,
    message TEXT,
    entity_id UUID,
    entity_type TEXT,
    entity JSONB,
    updated_fields TEXT[],
    cascade JSONB,
    error_detail JSONB,
    metadata JSONB";

const JUNCTION_TABLE: &str = "\
-- Post-Tag junction
CREATE TABLE IF NOT EXISTS tb_post_tag (
    post_id UUID NOT NULL REFERENCES tb_post(id) ON DELETE CASCADE,
    tag_id  UUID NOT NULL REFERENCES tb_tag(id) ON DELETE CASCADE,
    PRIMARY KEY (post_id, tag_id)
);
";

// --- Per-entity PostgreSQL templates ---

const ENTITY_AUTHOR_TABLE: &str = "\
-- Table: author
-- Trinity pattern: pk (internal), id (public UUID), identifier (URL slug)

CREATE TABLE IF NOT EXISTS tb_author (
    pk_author   SERIAL PRIMARY KEY,
    id          UUID NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    identifier  TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    email       TEXT NOT NULL UNIQUE,
    bio         TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_tb_author_email ON tb_author (email);
";

const ENTITY_AUTHOR_VIEW: &str = "\
-- View: author (read-optimized)
-- The runtime reads `SELECT data FROM v_author`; JSONB keys are snake_case
-- storage keys (the runtime maps camelCase GraphQL field names onto them).

CREATE OR REPLACE VIEW v_author AS
SELECT
    pk_author,
    id,
    jsonb_build_object(
        'pk', pk_author,
        'id', id,
        'identifier', identifier,
        'name', name,
        'email', email,
        'bio', bio,
        'created_at', created_at,
        'updated_at', updated_at
    ) AS data
FROM tb_author;
";

const ENTITY_POST_TABLE: &str = "\
-- Table: post
-- Trinity pattern: pk (internal), id (public UUID), identifier (URL slug)

CREATE TABLE IF NOT EXISTS tb_post (
    pk_post     SERIAL PRIMARY KEY,
    id          UUID NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    identifier  TEXT NOT NULL UNIQUE,
    title       TEXT NOT NULL,
    body        TEXT NOT NULL,
    published   BOOLEAN NOT NULL DEFAULT false,
    author_id   UUID NOT NULL REFERENCES tb_author(id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_tb_post_author ON tb_post (author_id);
CREATE INDEX IF NOT EXISTS idx_tb_post_published ON tb_post (published) WHERE published = true;
";

const ENTITY_POST_VIEW: &str = "\
-- View: post (read-optimized)
-- The runtime reads `SELECT data FROM v_post`; JSONB keys are snake_case
-- storage keys (the runtime maps camelCase GraphQL field names onto them).

CREATE OR REPLACE VIEW v_post AS
SELECT
    pk_post,
    id,
    jsonb_build_object(
        'pk', pk_post,
        'id', id,
        'identifier', identifier,
        'title', title,
        'body', body,
        'published', published,
        'author_id', author_id,
        'created_at', created_at,
        'updated_at', updated_at
    ) AS data
FROM tb_post;
";

const ENTITY_COMMENT_TABLE: &str = "\
-- Table: comment

CREATE TABLE IF NOT EXISTS tb_comment (
    pk_comment  SERIAL PRIMARY KEY,
    id          UUID NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    body        TEXT NOT NULL,
    author_name TEXT NOT NULL,
    post_id     UUID NOT NULL REFERENCES tb_post(id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_tb_comment_post ON tb_comment (post_id);
";

const ENTITY_COMMENT_VIEW: &str = "\
-- View: comment (read-optimized)
-- The runtime reads `SELECT data FROM v_comment`; JSONB keys are snake_case
-- storage keys (the runtime maps camelCase GraphQL field names onto them).

CREATE OR REPLACE VIEW v_comment AS
SELECT
    pk_comment,
    id,
    jsonb_build_object(
        'pk', pk_comment,
        'id', id,
        'body', body,
        'author_name', author_name,
        'post_id', post_id,
        'created_at', created_at
    ) AS data
FROM tb_comment;
";

const ENTITY_TAG_TABLE: &str = "\
-- Table: tag
-- Trinity pattern: pk (internal), id (public UUID), identifier (URL slug)

CREATE TABLE IF NOT EXISTS tb_tag (
    pk_tag      SERIAL PRIMARY KEY,
    id          UUID NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    identifier  TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL UNIQUE
);
";

const ENTITY_TAG_VIEW: &str = "\
-- View: tag (read-optimized)
-- The runtime reads `SELECT data FROM v_tag`; JSONB keys are snake_case
-- storage keys (the runtime maps camelCase GraphQL field names onto them).

CREATE OR REPLACE VIEW v_tag AS
SELECT
    pk_tag,
    id,
    jsonb_build_object(
        'pk', pk_tag,
        'id', id,
        'identifier', identifier,
        'name', name
    ) AS data
FROM tb_tag;
";

/// Author create/delete — the v2.2 `mutation_response` contract (#569): the
/// runtime wraps every mutation in a change-log CTE that reads the 13-column
/// row (`r.entity_type`, `r.succeeded`, …) from the function's result. The
/// entity payload is read back from the view so the write path and read path
/// can never disagree about the JSON shape.
fn author_functions() -> String {
    format!(
        "\
-- Mutation functions for author (v2.2 mutation_response contract)
-- Requires `fraiseql setup` (installs fraiseql.mutation_ok/mutation_err).

CREATE OR REPLACE FUNCTION fn_author_create(
    p_identifier TEXT,
    p_name TEXT,
    p_email TEXT,
    p_bio TEXT DEFAULT NULL
) RETURNS TABLE(
{cols}
)
LANGUAGE plpgsql AS $$
DECLARE
    v_id UUID;
BEGIN
    INSERT INTO tb_author (identifier, name, email, bio)
    VALUES (p_identifier, p_name, p_email, p_bio)
    RETURNING tb_author.id INTO v_id;
    RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
        (SELECT data FROM v_author WHERE v_author.id = v_id), v_id, 'Author');
END;
$$;

CREATE OR REPLACE FUNCTION fn_author_delete(p_id UUID)
RETURNS TABLE(
{cols}
)
LANGUAGE plpgsql AS $$
DECLARE
    v_entity JSONB;
BEGIN
    SELECT data INTO v_entity FROM v_author WHERE v_author.id = p_id;
    IF v_entity IS NULL THEN
        RETURN QUERY SELECT * FROM fraiseql.mutation_err('not_found', 'Author not found');
        RETURN;
    END IF;
    DELETE FROM tb_author WHERE tb_author.id = p_id;
    RETURN QUERY SELECT * FROM fraiseql.mutation_ok(v_entity, p_id, 'Author');
END;
$$;
",
        cols = MUTATION_RESPONSE_COLUMNS
    )
}

fn post_functions() -> String {
    format!(
        "\
-- Mutation functions for post (v2.2 mutation_response contract)
-- Requires `fraiseql setup` (installs fraiseql.mutation_ok/mutation_err).

CREATE OR REPLACE FUNCTION fn_post_create(
    p_identifier TEXT,
    p_title TEXT,
    p_body TEXT,
    p_author_id UUID
) RETURNS TABLE(
{cols}
)
LANGUAGE plpgsql AS $$
DECLARE
    v_id UUID;
BEGIN
    INSERT INTO tb_post (identifier, title, body, author_id)
    VALUES (p_identifier, p_title, p_body, p_author_id)
    RETURNING tb_post.id INTO v_id;
    RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
        (SELECT data FROM v_post WHERE v_post.id = v_id), v_id, 'Post');
END;
$$;

CREATE OR REPLACE FUNCTION fn_post_publish(p_id UUID)
RETURNS TABLE(
{cols}
)
LANGUAGE plpgsql AS $$
BEGIN
    UPDATE tb_post SET published = true, updated_at = now() WHERE tb_post.id = p_id;
    IF NOT FOUND THEN
        RETURN QUERY SELECT * FROM fraiseql.mutation_err('not_found', 'Post not found');
        RETURN;
    END IF;
    RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
        (SELECT data FROM v_post WHERE v_post.id = p_id), p_id, 'Post', TRUE,
        ARRAY['published', 'updatedAt']);
END;
$$;

CREATE OR REPLACE FUNCTION fn_post_delete(p_id UUID)
RETURNS TABLE(
{cols}
)
LANGUAGE plpgsql AS $$
DECLARE
    v_entity JSONB;
BEGIN
    SELECT data INTO v_entity FROM v_post WHERE v_post.id = p_id;
    IF v_entity IS NULL THEN
        RETURN QUERY SELECT * FROM fraiseql.mutation_err('not_found', 'Post not found');
        RETURN;
    END IF;
    DELETE FROM tb_post WHERE tb_post.id = p_id;
    RETURN QUERY SELECT * FROM fraiseql.mutation_ok(v_entity, p_id, 'Post');
END;
$$;
",
        cols = MUTATION_RESPONSE_COLUMNS
    )
}

fn comment_functions() -> String {
    format!(
        "\
-- Mutation functions for comment (v2.2 mutation_response contract)
-- Requires `fraiseql setup` (installs fraiseql.mutation_ok/mutation_err).

CREATE OR REPLACE FUNCTION fn_comment_create(
    p_body TEXT,
    p_author_name TEXT,
    p_post_id UUID
) RETURNS TABLE(
{cols}
)
LANGUAGE plpgsql AS $$
DECLARE
    v_id UUID;
BEGIN
    INSERT INTO tb_comment (body, author_name, post_id)
    VALUES (p_body, p_author_name, p_post_id)
    RETURNING tb_comment.id INTO v_id;
    RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
        (SELECT data FROM v_comment WHERE v_comment.id = v_id), v_id, 'Comment');
END;
$$;

CREATE OR REPLACE FUNCTION fn_comment_delete(p_id UUID)
RETURNS TABLE(
{cols}
)
LANGUAGE plpgsql AS $$
DECLARE
    v_entity JSONB;
BEGIN
    SELECT data INTO v_entity FROM v_comment WHERE v_comment.id = p_id;
    IF v_entity IS NULL THEN
        RETURN QUERY SELECT * FROM fraiseql.mutation_err('not_found', 'Comment not found');
        RETURN;
    END IF;
    DELETE FROM tb_comment WHERE tb_comment.id = p_id;
    RETURN QUERY SELECT * FROM fraiseql.mutation_ok(v_entity, p_id, 'Comment');
END;
$$;
",
        cols = MUTATION_RESPONSE_COLUMNS
    )
}

fn tag_functions() -> String {
    format!(
        "\
-- Mutation functions for tag (v2.2 mutation_response contract)
-- Requires `fraiseql setup` (installs fraiseql.mutation_ok/mutation_err).

CREATE OR REPLACE FUNCTION fn_tag_create(
    p_identifier TEXT,
    p_name TEXT
) RETURNS TABLE(
{cols}
)
LANGUAGE plpgsql AS $$
DECLARE
    v_id UUID;
BEGIN
    INSERT INTO tb_tag (identifier, name)
    VALUES (p_identifier, p_name)
    RETURNING tb_tag.id INTO v_id;
    RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
        (SELECT data FROM v_tag WHERE v_tag.id = v_id), v_id, 'Tag');
END;
$$;

CREATE OR REPLACE FUNCTION fn_tag_delete(p_id UUID)
RETURNS TABLE(
{cols}
)
LANGUAGE plpgsql AS $$
DECLARE
    v_entity JSONB;
BEGIN
    SELECT data INTO v_entity FROM v_tag WHERE v_tag.id = p_id;
    IF v_entity IS NULL THEN
        RETURN QUERY SELECT * FROM fraiseql.mutation_err('not_found', 'Tag not found');
        RETURN;
    END IF;
    DELETE FROM tb_tag WHERE tb_tag.id = p_id;
    RETURN QUERY SELECT * FROM fraiseql.mutation_ok(v_entity, p_id, 'Tag');
END;
$$;
",
        cols = MUTATION_RESPONSE_COLUMNS
    )
}
