use super::Database;

/// Generate a single-file schema for XS size (blog project)
pub(super) fn generate_single_schema_sql(database: Database) -> String {
    match database {
        Database::Postgres => BLOG_SCHEMA_POSTGRES.to_string(),
        Database::Mysql => BLOG_SCHEMA_MYSQL.to_string(),
        Database::Sqlite => BLOG_SCHEMA_SQLITE.to_string(),
        Database::SqlServer => BLOG_SCHEMA_SQLSERVER.to_string(),
    }
}

const BLOG_SCHEMA_POSTGRES: &str = "\
-- FraiseQL Blog Schema
-- Trinity pattern: pk (internal), id (public UUID), identifier (URL slug)

-- Authors
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

CREATE OR REPLACE VIEW v_author AS
SELECT pk_author, id, identifier, name, email, bio, created_at, updated_at
FROM tb_author;

-- Posts
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

CREATE OR REPLACE VIEW v_post AS
SELECT pk_post, id, identifier, title, body, published, author_id, created_at, updated_at
FROM tb_post;

-- Comments
CREATE TABLE IF NOT EXISTS tb_comment (
    pk_comment  SERIAL PRIMARY KEY,
    id          UUID NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    body        TEXT NOT NULL,
    author_name TEXT NOT NULL,
    post_id     UUID NOT NULL REFERENCES tb_post(id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_tb_comment_post ON tb_comment (post_id);

CREATE OR REPLACE VIEW v_comment AS
SELECT pk_comment, id, body, author_name, post_id, created_at
FROM tb_comment;

-- Tags
CREATE TABLE IF NOT EXISTS tb_tag (
    pk_tag      SERIAL PRIMARY KEY,
    id          UUID NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    identifier  TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL UNIQUE
);

CREATE OR REPLACE VIEW v_tag AS
SELECT pk_tag, id, identifier, name
FROM tb_tag;

-- Post-Tag junction
CREATE TABLE IF NOT EXISTS tb_post_tag (
    post_id UUID NOT NULL REFERENCES tb_post(id) ON DELETE CASCADE,
    tag_id  UUID NOT NULL REFERENCES tb_tag(id) ON DELETE CASCADE,
    PRIMARY KEY (post_id, tag_id)
);
";

const BLOG_SCHEMA_MYSQL: &str = "\
-- FraiseQL Blog Schema
-- Trinity pattern: pk (internal), id (public UUID), identifier (URL slug)

CREATE TABLE IF NOT EXISTS tb_author (
    pk_author   INT AUTO_INCREMENT PRIMARY KEY,
    id          CHAR(36) NOT NULL DEFAULT (UUID()) UNIQUE,
    identifier  VARCHAR(255) NOT NULL UNIQUE,
    name        VARCHAR(255) NOT NULL,
    email       VARCHAR(255) NOT NULL UNIQUE,
    bio         TEXT,
    created_at  TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at  TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    INDEX idx_tb_author_email (email)
);

CREATE OR REPLACE VIEW v_author AS
SELECT pk_author, id, identifier, name, email, bio, created_at, updated_at
FROM tb_author;

CREATE TABLE IF NOT EXISTS tb_post (
    pk_post     INT AUTO_INCREMENT PRIMARY KEY,
    id          CHAR(36) NOT NULL DEFAULT (UUID()) UNIQUE,
    identifier  VARCHAR(255) NOT NULL UNIQUE,
    title       VARCHAR(500) NOT NULL,
    body        LONGTEXT NOT NULL,
    published   BOOLEAN NOT NULL DEFAULT false,
    author_id   CHAR(36) NOT NULL,
    created_at  TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at  TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    INDEX idx_tb_post_author (author_id),
    INDEX idx_tb_post_published (published)
);

CREATE OR REPLACE VIEW v_post AS
SELECT pk_post, id, identifier, title, body, published, author_id, created_at, updated_at
FROM tb_post;

CREATE TABLE IF NOT EXISTS tb_comment (
    pk_comment  INT AUTO_INCREMENT PRIMARY KEY,
    id          CHAR(36) NOT NULL DEFAULT (UUID()) UNIQUE,
    body        TEXT NOT NULL,
    author_name VARCHAR(255) NOT NULL,
    post_id     CHAR(36) NOT NULL,
    created_at  TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    INDEX idx_tb_comment_post (post_id)
);

CREATE OR REPLACE VIEW v_comment AS
SELECT pk_comment, id, body, author_name, post_id, created_at
FROM tb_comment;

CREATE TABLE IF NOT EXISTS tb_tag (
    pk_tag      INT AUTO_INCREMENT PRIMARY KEY,
    id          CHAR(36) NOT NULL DEFAULT (UUID()) UNIQUE,
    identifier  VARCHAR(255) NOT NULL UNIQUE,
    name        VARCHAR(255) NOT NULL UNIQUE
);

CREATE OR REPLACE VIEW v_tag AS
SELECT pk_tag, id, identifier, name
FROM tb_tag;
";

const BLOG_SCHEMA_SQLITE: &str = "\
-- FraiseQL Blog Schema
-- Trinity pattern: pk (internal), id (public UUID), identifier (URL slug)

CREATE TABLE IF NOT EXISTS tb_author (
    pk_author   INTEGER PRIMARY KEY AUTOINCREMENT,
    id          TEXT NOT NULL UNIQUE,
    identifier  TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    email       TEXT NOT NULL UNIQUE,
    bio         TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE VIEW IF NOT EXISTS v_author AS
SELECT pk_author, id, identifier, name, email, bio, created_at, updated_at
FROM tb_author;

CREATE TABLE IF NOT EXISTS tb_post (
    pk_post     INTEGER PRIMARY KEY AUTOINCREMENT,
    id          TEXT NOT NULL UNIQUE,
    identifier  TEXT NOT NULL UNIQUE,
    title       TEXT NOT NULL,
    body        TEXT NOT NULL,
    published   INTEGER NOT NULL DEFAULT 0,
    author_id   TEXT NOT NULL REFERENCES tb_author(id),
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_tb_post_author ON tb_post (author_id);

CREATE VIEW IF NOT EXISTS v_post AS
SELECT pk_post, id, identifier, title, body, published, author_id, created_at, updated_at
FROM tb_post;

CREATE TABLE IF NOT EXISTS tb_comment (
    pk_comment  INTEGER PRIMARY KEY AUTOINCREMENT,
    id          TEXT NOT NULL UNIQUE,
    body        TEXT NOT NULL,
    author_name TEXT NOT NULL,
    post_id     TEXT NOT NULL REFERENCES tb_post(id) ON DELETE CASCADE,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_tb_comment_post ON tb_comment (post_id);

CREATE VIEW IF NOT EXISTS v_comment AS
SELECT pk_comment, id, body, author_name, post_id, created_at
FROM tb_comment;

CREATE TABLE IF NOT EXISTS tb_tag (
    pk_tag      INTEGER PRIMARY KEY AUTOINCREMENT,
    id          TEXT NOT NULL UNIQUE,
    identifier  TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL UNIQUE
);

CREATE VIEW IF NOT EXISTS v_tag AS
SELECT pk_tag, id, identifier, name
FROM tb_tag;
";

const BLOG_SCHEMA_SQLSERVER: &str = "\
-- FraiseQL Blog Schema
-- Trinity pattern: pk (internal), id (public UUID), identifier (URL slug)

IF NOT EXISTS (SELECT * FROM sysobjects WHERE name='tb_author' AND xtype='U')
CREATE TABLE tb_author (
    pk_author   INT IDENTITY(1,1) PRIMARY KEY,
    id          UNIQUEIDENTIFIER NOT NULL DEFAULT NEWID() UNIQUE,
    identifier  NVARCHAR(255) NOT NULL UNIQUE,
    name        NVARCHAR(255) NOT NULL,
    email       NVARCHAR(255) NOT NULL UNIQUE,
    bio         NVARCHAR(MAX),
    created_at  DATETIME2 NOT NULL DEFAULT GETUTCDATE(),
    updated_at  DATETIME2 NOT NULL DEFAULT GETUTCDATE()
);
GO

CREATE OR ALTER VIEW v_author AS
SELECT pk_author, id, identifier, name, email, bio, created_at, updated_at
FROM tb_author;
GO

IF NOT EXISTS (SELECT * FROM sysobjects WHERE name='tb_post' AND xtype='U')
CREATE TABLE tb_post (
    pk_post     INT IDENTITY(1,1) PRIMARY KEY,
    id          UNIQUEIDENTIFIER NOT NULL DEFAULT NEWID() UNIQUE,
    identifier  NVARCHAR(255) NOT NULL UNIQUE,
    title       NVARCHAR(500) NOT NULL,
    body        NVARCHAR(MAX) NOT NULL,
    published   BIT NOT NULL DEFAULT 0,
    author_id   UNIQUEIDENTIFIER NOT NULL,
    created_at  DATETIME2 NOT NULL DEFAULT GETUTCDATE(),
    updated_at  DATETIME2 NOT NULL DEFAULT GETUTCDATE()
);
GO

CREATE OR ALTER VIEW v_post AS
SELECT pk_post, id, identifier, title, body, published, author_id, created_at, updated_at
FROM tb_post;
GO

IF NOT EXISTS (SELECT * FROM sysobjects WHERE name='tb_comment' AND xtype='U')
CREATE TABLE tb_comment (
    pk_comment  INT IDENTITY(1,1) PRIMARY KEY,
    id          UNIQUEIDENTIFIER NOT NULL DEFAULT NEWID() UNIQUE,
    body        NVARCHAR(MAX) NOT NULL,
    author_name NVARCHAR(255) NOT NULL,
    post_id     UNIQUEIDENTIFIER NOT NULL,
    created_at  DATETIME2 NOT NULL DEFAULT GETUTCDATE()
);
GO

CREATE OR ALTER VIEW v_comment AS
SELECT pk_comment, id, body, author_name, post_id, created_at
FROM tb_comment;
GO

IF NOT EXISTS (SELECT * FROM sysobjects WHERE name='tb_tag' AND xtype='U')
CREATE TABLE tb_tag (
    pk_tag      INT IDENTITY(1,1) PRIMARY KEY,
    id          UNIQUEIDENTIFIER NOT NULL DEFAULT NEWID() UNIQUE,
    identifier  NVARCHAR(255) NOT NULL UNIQUE,
    name        NVARCHAR(255) NOT NULL UNIQUE
);
GO

CREATE OR ALTER VIEW v_tag AS
SELECT pk_tag, id, identifier, name
FROM tb_tag;
GO
";

/// Generate per-entity SQL split into (table, view, functions) for S/M layouts
pub(super) fn generate_blog_entity_sql(database: Database, entity: &str) -> (String, String, String) {
    if database != Database::Postgres {
        // Non-Postgres databases get the full schema in the first entity file only,
        // and empty strings for subsequent entities
        if entity == "author" {
            let single = generate_single_schema_sql(database);
            return (single, String::new(), String::new());
        }
        return (
            format!("-- See tb_author.sql for full {database} schema\n"),
            String::new(),
            String::new(),
        );
    }

    match entity {
        "author" => (
            ENTITY_AUTHOR_TABLE.to_string(),
            ENTITY_AUTHOR_VIEW.to_string(),
            ENTITY_AUTHOR_FUNCTIONS.to_string(),
        ),
        "post" => (
            ENTITY_POST_TABLE.to_string(),
            ENTITY_POST_VIEW.to_string(),
            ENTITY_POST_FUNCTIONS.to_string(),
        ),
        "comment" => (
            ENTITY_COMMENT_TABLE.to_string(),
            ENTITY_COMMENT_VIEW.to_string(),
            ENTITY_COMMENT_FUNCTIONS.to_string(),
        ),
        "tag" => (
            ENTITY_TAG_TABLE.to_string(),
            ENTITY_TAG_VIEW.to_string(),
            ENTITY_TAG_FUNCTIONS.to_string(),
        ),
        _ => (format!("-- Unknown entity: {entity}\n"), String::new(), String::new()),
    }
}

// --- Per-entity Postgres SQL templates ---

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

CREATE OR REPLACE VIEW v_author AS
SELECT pk_author, id, identifier, name, email, bio, created_at, updated_at
FROM tb_author;
";

const ENTITY_AUTHOR_FUNCTIONS: &str = "\
-- CRUD functions for author

CREATE OR REPLACE FUNCTION fn_author_create(
    p_identifier TEXT,
    p_name TEXT,
    p_email TEXT,
    p_bio TEXT DEFAULT NULL
) RETURNS UUID
LANGUAGE plpgsql AS $$
DECLARE
    v_id UUID;
BEGIN
    INSERT INTO tb_author (identifier, name, email, bio)
    VALUES (p_identifier, p_name, p_email, p_bio)
    RETURNING id INTO v_id;
    RETURN v_id;
END;
$$;

CREATE OR REPLACE FUNCTION fn_author_delete(p_id UUID)
RETURNS BOOLEAN
LANGUAGE plpgsql AS $$
BEGIN
    DELETE FROM tb_author WHERE id = p_id;
    RETURN FOUND;
END;
$$;
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

CREATE OR REPLACE VIEW v_post AS
SELECT pk_post, id, identifier, title, body, published, author_id, created_at, updated_at
FROM tb_post;
";

const ENTITY_POST_FUNCTIONS: &str = "\
-- CRUD functions for post

CREATE OR REPLACE FUNCTION fn_post_create(
    p_identifier TEXT,
    p_title TEXT,
    p_body TEXT,
    p_author_id UUID
) RETURNS UUID
LANGUAGE plpgsql AS $$
DECLARE
    v_id UUID;
BEGIN
    INSERT INTO tb_post (identifier, title, body, author_id)
    VALUES (p_identifier, p_title, p_body, p_author_id)
    RETURNING id INTO v_id;
    RETURN v_id;
END;
$$;

CREATE OR REPLACE FUNCTION fn_post_publish(p_id UUID)
RETURNS BOOLEAN
LANGUAGE plpgsql AS $$
BEGIN
    UPDATE tb_post SET published = true, updated_at = now() WHERE id = p_id;
    RETURN FOUND;
END;
$$;

CREATE OR REPLACE FUNCTION fn_post_delete(p_id UUID)
RETURNS BOOLEAN
LANGUAGE plpgsql AS $$
BEGIN
    DELETE FROM tb_post WHERE id = p_id;
    RETURN FOUND;
END;
$$;
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

CREATE OR REPLACE VIEW v_comment AS
SELECT pk_comment, id, body, author_name, post_id, created_at
FROM tb_comment;
";

const ENTITY_COMMENT_FUNCTIONS: &str = "\
-- CRUD functions for comment

CREATE OR REPLACE FUNCTION fn_comment_create(
    p_body TEXT,
    p_author_name TEXT,
    p_post_id UUID
) RETURNS UUID
LANGUAGE plpgsql AS $$
DECLARE
    v_id UUID;
BEGIN
    INSERT INTO tb_comment (body, author_name, post_id)
    VALUES (p_body, p_author_name, p_post_id)
    RETURNING id INTO v_id;
    RETURN v_id;
END;
$$;

CREATE OR REPLACE FUNCTION fn_comment_delete(p_id UUID)
RETURNS BOOLEAN
LANGUAGE plpgsql AS $$
BEGIN
    DELETE FROM tb_comment WHERE id = p_id;
    RETURN FOUND;
END;
$$;
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

CREATE OR REPLACE VIEW v_tag AS
SELECT pk_tag, id, identifier, name
FROM tb_tag;
";

const ENTITY_TAG_FUNCTIONS: &str = "\
-- CRUD functions for tag

CREATE OR REPLACE FUNCTION fn_tag_create(
    p_identifier TEXT,
    p_name TEXT
) RETURNS UUID
LANGUAGE plpgsql AS $$
DECLARE
    v_id UUID;
BEGIN
    INSERT INTO tb_tag (identifier, name)
    VALUES (p_identifier, p_name)
    RETURNING id INTO v_id;
    RETURN v_id;
END;
$$;

CREATE OR REPLACE FUNCTION fn_tag_delete(p_id UUID)
RETURNS BOOLEAN
LANGUAGE plpgsql AS $$
BEGIN
    DELETE FROM tb_tag WHERE id = p_id;
    RETURN FOUND;
END;
$$;
";
