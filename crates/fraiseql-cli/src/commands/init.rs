//! `fraiseql init` - Interactive project scaffolder
//!
//! Creates a new FraiseQL project with the correct directory structure,
//! configuration files, and authoring skeleton in the chosen language.

use std::{
    fmt, fs,
    path::{Path, PathBuf},
    process::Command,
    str::FromStr,
};

use anyhow::{Context, Result};
use tracing::info;

/// Supported authoring languages
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    /// Python authoring (default)
    Python,
    /// TypeScript authoring
    TypeScript,
    /// Rust authoring
    Rust,
    /// Java authoring
    Java,
    /// Kotlin authoring
    Kotlin,
    /// Go authoring
    Go,
    /// C# authoring
    CSharp,
    /// Swift authoring
    Swift,
    /// Scala authoring
    Scala,
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Python => write!(f, "python"),
            Self::TypeScript => write!(f, "typescript"),
            Self::Rust => write!(f, "rust"),
            Self::Java => write!(f, "java"),
            Self::Kotlin => write!(f, "kotlin"),
            Self::Go => write!(f, "go"),
            Self::CSharp => write!(f, "csharp"),
            Self::Swift => write!(f, "swift"),
            Self::Scala => write!(f, "scala"),
        }
    }
}

impl Language {
    /// Map file extension to language (for `fraiseql extract` auto-detection).
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "py" => Some(Self::Python),
            "ts" | "tsx" => Some(Self::TypeScript),
            "rs" => Some(Self::Rust),
            "java" => Some(Self::Java),
            "kt" | "kts" => Some(Self::Kotlin),
            "go" => Some(Self::Go),
            "cs" => Some(Self::CSharp),
            "swift" => Some(Self::Swift),
            "scala" | "sc" => Some(Self::Scala),
            _ => None,
        }
    }
}

impl FromStr for Language {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "python" | "py" => Ok(Self::Python),
            "typescript" | "ts" => Ok(Self::TypeScript),
            "rust" | "rs" => Ok(Self::Rust),
            "java" | "jav" => Ok(Self::Java),
            "kotlin" | "kt" => Ok(Self::Kotlin),
            "go" | "golang" => Ok(Self::Go),
            "csharp" | "c#" | "cs" => Ok(Self::CSharp),
            "swift" => Ok(Self::Swift),
            "scala" | "sc" => Ok(Self::Scala),
            other => Err(format!(
                "Unknown language: {other}. Choose: python, typescript, rust, java, kotlin, go, csharp, swift, scala"
            )),
        }
    }
}

/// Supported databases
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Database {
    /// PostgreSQL (primary)
    Postgres,
    /// MySQL
    Mysql,
    /// SQLite
    Sqlite,
    /// SQL Server
    SqlServer,
}

impl fmt::Display for Database {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Postgres => write!(f, "postgres"),
            Self::Mysql => write!(f, "mysql"),
            Self::Sqlite => write!(f, "sqlite"),
            Self::SqlServer => write!(f, "sqlserver"),
        }
    }
}

impl FromStr for Database {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "postgres" | "postgresql" | "pg" => Ok(Self::Postgres),
            "mysql" => Ok(Self::Mysql),
            "sqlite" => Ok(Self::Sqlite),
            "sqlserver" | "mssql" => Ok(Self::SqlServer),
            other => Err(format!(
                "Unknown database: {other}. Choose: postgres, mysql, sqlite, sqlserver"
            )),
        }
    }
}

/// Database target string for fraiseql.toml
impl Database {
    const fn toml_target(self) -> &'static str {
        match self {
            Self::Postgres => "postgresql",
            Self::Mysql => "mysql",
            Self::Sqlite => "sqlite",
            Self::SqlServer => "sqlserver",
        }
    }

    fn default_url(self, project_name: &str) -> String {
        match self {
            Self::Postgres => format!("postgresql://localhost/{project_name}"),
            Self::Mysql => format!("mysql://localhost/{project_name}"),
            Self::Sqlite => format!("{project_name}.db"),
            Self::SqlServer => format!("mssql://localhost/{project_name}"),
        }
    }
}

/// Project size determines directory granularity
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectSize {
    /// Single `schema.sql` file
    Xs,
    /// Flat numbered directories (01_write, 02_read, 03_functions)
    S,
    /// Per-entity subdirectories under each numbered directory
    M,
}

impl FromStr for ProjectSize {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "xs" => Ok(Self::Xs),
            "s" => Ok(Self::S),
            "m" => Ok(Self::M),
            other => Err(format!("Unknown size: {other}. Choose: xs, s, m")),
        }
    }
}

/// Configuration for the init command
pub struct InitConfig {
    /// Name of the project (used as directory name)
    pub project_name: String,
    /// Authoring language for the schema skeleton
    pub language:     Language,
    /// Target database engine
    pub database:     Database,
    /// Project size (directory granularity)
    pub size:         ProjectSize,
    /// Skip git initialization
    pub no_git:       bool,
}

/// Run the init command
pub fn run(config: &InitConfig) -> Result<()> {
    let project_dir = PathBuf::from(&config.project_name);

    if project_dir.exists() {
        anyhow::bail!(
            "Directory '{}' already exists. Choose a different name or remove it first.",
            config.project_name
        );
    }

    info!("Creating project: {}", config.project_name);
    println!("Creating FraiseQL project: {}", config.project_name);

    // Create root directory
    fs::create_dir_all(&project_dir)
        .context(format!("Failed to create directory: {}", config.project_name))?;

    // Create .gitignore
    create_gitignore(&project_dir)?;

    // Create fraiseql.toml
    create_toml_config(&project_dir, config)?;

    // Create schema.json
    create_schema_json(&project_dir)?;

    // Create database directory structure
    create_db_structure(&project_dir, config)?;

    // Create language-specific authoring skeleton
    create_authoring_skeleton(&project_dir, config)?;

    // Initialize git repository
    if !config.no_git {
        init_git(&project_dir)?;
    }

    println!();
    println!("Project created at ./{}", config.project_name);
    println!();
    println!("Next steps:");
    println!("  cd {}", config.project_name);
    println!("  fraiseql compile fraiseql.toml");
    if !config.no_git {
        println!("  git add -A && git commit -m \"Initial FraiseQL project\"");
    }
    println!();

    Ok(())
}

fn create_gitignore(project_dir: &Path) -> Result<()> {
    let content = "\
# FraiseQL compiled output
schema.compiled.json

# Rust
target/

# Python
__pycache__/
*.pyc
.venv/

# TypeScript / Node
node_modules/
dist/

# IDE
.idea/
.vscode/
*.swp
*.swo

# OS
.DS_Store
Thumbs.db

# Environment
.env
.env.local
";
    fs::write(project_dir.join(".gitignore"), content).context("Failed to create .gitignore")?;
    info!("Created .gitignore");
    Ok(())
}

fn create_toml_config(project_dir: &Path, config: &InitConfig) -> Result<()> {
    let db_url = config.database.default_url(&config.project_name);
    let db_target = config.database.toml_target();

    let content = format!(
        r#"[project]
name = "{name}"
version = "0.1.0"
description = "A FraiseQL project"
database_target = "{db_target}"

[fraiseql]
schema_file = "schema.json"
output_file = "schema.compiled.json"

[fraiseql.security.rate_limiting]
enabled = true
auth_start_max_requests = 100
auth_start_window_secs = 60

[fraiseql.security.audit_logging]
enabled = true
log_level = "info"

# Database connection URL — set via DATABASE_URL environment variable at runtime
# {db_url}
"#,
        name = config.project_name,
    );

    fs::write(project_dir.join("fraiseql.toml"), content)
        .context("Failed to create fraiseql.toml")?;
    info!("Created fraiseql.toml");
    Ok(())
}

fn create_schema_json(project_dir: &Path) -> Result<()> {
    // IntermediateSchema format: arrays of typed objects
    // Blog project: Author, Post, Comment, Tag
    let schema = serde_json::json!({
        "version": "2.0.0",
        "types": [
            {
                "name": "Author",
                "description": "Blog author",
                "fields": [
                    { "name": "pk", "type": "Int", "nullable": false, "description": "Internal primary key" },
                    { "name": "id", "type": "ID", "nullable": false, "description": "Public UUID" },
                    { "name": "identifier", "type": "String", "nullable": false, "description": "URL slug" },
                    { "name": "name", "type": "String", "nullable": false },
                    { "name": "email", "type": "String", "nullable": false },
                    { "name": "bio", "type": "String", "nullable": true },
                    { "name": "created_at", "type": "DateTime", "nullable": false },
                    { "name": "updated_at", "type": "DateTime", "nullable": false }
                ]
            },
            {
                "name": "Post",
                "description": "Blog post",
                "fields": [
                    { "name": "pk", "type": "Int", "nullable": false },
                    { "name": "id", "type": "ID", "nullable": false },
                    { "name": "identifier", "type": "String", "nullable": false, "description": "URL slug" },
                    { "name": "title", "type": "String", "nullable": false },
                    { "name": "body", "type": "String", "nullable": false },
                    { "name": "published", "type": "Boolean", "nullable": false },
                    { "name": "author_id", "type": "ID", "nullable": false },
                    { "name": "created_at", "type": "DateTime", "nullable": false },
                    { "name": "updated_at", "type": "DateTime", "nullable": false }
                ]
            },
            {
                "name": "Comment",
                "description": "Comment on a blog post",
                "fields": [
                    { "name": "pk", "type": "Int", "nullable": false },
                    { "name": "id", "type": "ID", "nullable": false },
                    { "name": "body", "type": "String", "nullable": false },
                    { "name": "author_name", "type": "String", "nullable": false },
                    { "name": "post_id", "type": "ID", "nullable": false },
                    { "name": "created_at", "type": "DateTime", "nullable": false }
                ]
            },
            {
                "name": "Tag",
                "description": "Categorization tag for posts",
                "fields": [
                    { "name": "pk", "type": "Int", "nullable": false },
                    { "name": "id", "type": "ID", "nullable": false },
                    { "name": "identifier", "type": "String", "nullable": false, "description": "URL slug" },
                    { "name": "name", "type": "String", "nullable": false }
                ]
            }
        ],
        "queries": [
            {
                "name": "posts",
                "return_type": "Post",
                "return_array": true,
                "sql_source": "v_post",
                "description": "List all published posts"
            },
            {
                "name": "post",
                "return_type": "Post",
                "return_array": false,
                "sql_source": "v_post",
                "args": [{ "name": "id", "type": "ID", "required": true }]
            },
            {
                "name": "authors",
                "return_type": "Author",
                "return_array": true,
                "sql_source": "v_author"
            },
            {
                "name": "author",
                "return_type": "Author",
                "return_array": false,
                "sql_source": "v_author",
                "args": [{ "name": "id", "type": "ID", "required": true }]
            },
            {
                "name": "tags",
                "return_type": "Tag",
                "return_array": true,
                "sql_source": "v_tag"
            }
        ],
        "mutations": [],
        "enums": [],
        "input_types": [],
        "interfaces": [],
        "unions": [],
        "subscriptions": []
    });

    let content = serde_json::to_string_pretty(&schema).context("Failed to serialize schema")?;
    fs::write(project_dir.join("schema.json"), content).context("Failed to create schema.json")?;
    info!("Created schema.json");
    Ok(())
}

fn create_db_structure(project_dir: &Path, config: &InitConfig) -> Result<()> {
    match config.size {
        ProjectSize::Xs => create_db_xs(project_dir, config),
        ProjectSize::S => create_db_s(project_dir, config),
        ProjectSize::M => create_db_m(project_dir, config),
    }
}

fn create_db_xs(project_dir: &Path, config: &InitConfig) -> Result<()> {
    let db_dir = project_dir.join("db").join("0_schema");
    fs::create_dir_all(&db_dir).context("Failed to create db/0_schema")?;

    let content = generate_single_schema_sql(config.database);
    fs::write(db_dir.join("schema.sql"), content).context("Failed to create schema.sql")?;
    info!("Created db/0_schema/schema.sql (xs layout)");
    Ok(())
}

fn create_db_s(project_dir: &Path, config: &InitConfig) -> Result<()> {
    let schema_dir = project_dir.join("db").join("0_schema");
    let write_dir = schema_dir.join("01_write");
    let read_dir = schema_dir.join("02_read");
    let functions_dir = schema_dir.join("03_functions");

    fs::create_dir_all(&write_dir).context("Failed to create 01_write")?;
    fs::create_dir_all(&read_dir).context("Failed to create 02_read")?;
    fs::create_dir_all(&functions_dir).context("Failed to create 03_functions")?;

    // Blog entities: author, post, comment, tag (ordered by dependency)
    let entities = ["author", "post", "comment", "tag"];
    for (i, entity) in entities.iter().enumerate() {
        let n = i + 1;
        let (table_sql, view_sql, fn_sql) = generate_blog_entity_sql(config.database, entity);
        fs::write(write_dir.join(format!("01{n}_tb_{entity}.sql")), table_sql)
            .context(format!("Failed to create tb_{entity}.sql"))?;
        if !view_sql.is_empty() {
            fs::write(read_dir.join(format!("02{n}_v_{entity}.sql")), view_sql)
                .context(format!("Failed to create v_{entity}.sql"))?;
        }
        if !fn_sql.is_empty() {
            fs::write(functions_dir.join(format!("03{n}_fn_{entity}_crud.sql")), fn_sql)
                .context(format!("Failed to create fn_{entity}_crud.sql"))?;
        }
    }

    info!("Created db/0_schema/ (s layout)");
    Ok(())
}

fn create_db_m(project_dir: &Path, config: &InitConfig) -> Result<()> {
    let schema_dir = project_dir.join("db").join("0_schema");

    let entities = ["author", "post", "comment", "tag"];
    for entity in &entities {
        let write_dir = schema_dir.join("01_write").join(entity);
        let read_dir = schema_dir.join("02_read").join(entity);
        let functions_dir = schema_dir.join("03_functions").join(entity);

        fs::create_dir_all(&write_dir).context(format!("Failed to create 01_write/{entity}"))?;
        fs::create_dir_all(&read_dir).context(format!("Failed to create 02_read/{entity}"))?;
        fs::create_dir_all(&functions_dir)
            .context(format!("Failed to create 03_functions/{entity}"))?;

        let (table_sql, view_sql, fn_sql) = generate_blog_entity_sql(config.database, entity);
        fs::write(write_dir.join(format!("tb_{entity}.sql")), table_sql)
            .context(format!("Failed to create tb_{entity}.sql"))?;
        if !view_sql.is_empty() {
            fs::write(read_dir.join(format!("v_{entity}.sql")), view_sql)
                .context(format!("Failed to create v_{entity}.sql"))?;
        }
        if !fn_sql.is_empty() {
            fs::write(functions_dir.join(format!("fn_{entity}_crud.sql")), fn_sql)
                .context(format!("Failed to create fn_{entity}_crud.sql"))?;
        }
    }

    info!("Created db/0_schema/ (m layout)");
    Ok(())
}

/// Generate a single-file schema for XS size (blog project)
fn generate_single_schema_sql(database: Database) -> String {
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
fn generate_blog_entity_sql(database: Database, entity: &str) -> (String, String, String) {
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

fn create_authoring_skeleton(project_dir: &Path, config: &InitConfig) -> Result<()> {
    match config.language {
        Language::Python => create_python_skeleton(project_dir, config),
        Language::TypeScript => create_typescript_skeleton(project_dir, config),
        Language::Rust => create_rust_skeleton(project_dir, config),
        Language::Java => create_java_skeleton(project_dir, config),
        Language::Kotlin => create_kotlin_skeleton(project_dir, config),
        Language::Go => create_go_skeleton(project_dir, config),
        Language::CSharp => create_csharp_skeleton(project_dir, config),
        Language::Swift => create_swift_skeleton(project_dir, config),
        Language::Scala => create_scala_skeleton(project_dir, config),
    }
}

fn create_python_skeleton(project_dir: &Path, config: &InitConfig) -> Result<()> {
    let dir = project_dir.join("schema");
    fs::create_dir_all(&dir).context("Failed to create schema/ directory")?;

    let content = format!(
        r#"""FraiseQL blog schema definition for {name}."""

import fraiseql


@fraiseql.type(sql_source="v_author")
class Author:
    """Blog author with trinity pattern."""

    pk: int
    id: ID
    identifier: str
    name: str
    email: str
    bio: str | None
    created_at: DateTime
    updated_at: DateTime


@fraiseql.type(sql_source="v_post")
class Post:
    """Blog post with trinity pattern."""

    pk: int
    id: ID
    identifier: str
    title: str
    body: str
    published: bool
    author_id: ID
    created_at: DateTime
    updated_at: DateTime


@fraiseql.type(sql_source="v_comment")
class Comment:
    """Comment on a blog post."""

    pk: int
    id: ID
    body: str
    author_name: str
    post_id: ID
    created_at: DateTime


@fraiseql.type(sql_source="v_tag")
class Tag:
    """Categorization tag for posts."""

    pk: int
    id: ID
    identifier: str
    name: str


@fraiseql.query(return_type=Post, return_array=True, sql_source="v_post")
def posts() -> list[Post]:
    """List all published posts."""
    ...


@fraiseql.query(return_type=Post, sql_source="v_post")
def post(*, id: ID) -> Post:
    """Get post by ID."""
    ...


@fraiseql.query(return_type=Author, return_array=True, sql_source="v_author")
def authors() -> list[Author]:
    """List all authors."""
    ...


@fraiseql.query(return_type=Author, sql_source="v_author")
def author(*, id: ID) -> Author:
    """Get author by ID."""
    ...


@fraiseql.query(return_type=Tag, return_array=True, sql_source="v_tag")
def tags() -> list[Tag]:
    """List all tags."""
    ...
"#,
        name = config.project_name,
    );

    fs::write(dir.join("schema.py"), content).context("Failed to create schema.py")?;
    info!("Created schema/schema.py");
    Ok(())
}

fn create_typescript_skeleton(project_dir: &Path, config: &InitConfig) -> Result<()> {
    let dir = project_dir.join("schema");
    fs::create_dir_all(&dir).context("Failed to create schema/ directory")?;

    let content = format!(
        r#"/**
 * FraiseQL blog schema definition for {name}.
 */

import {{ type_, query }} from "fraiseql";

export const Author = type_("Author", {{
  sqlSource: "v_author",
  fields: {{
    pk: {{ type: "Int", nullable: false }},
    id: {{ type: "ID", nullable: false }},
    identifier: {{ type: "String", nullable: false }},
    name: {{ type: "String", nullable: false }},
    email: {{ type: "String", nullable: false }},
    bio: {{ type: "String", nullable: true }},
    created_at: {{ type: "DateTime", nullable: false }},
    updated_at: {{ type: "DateTime", nullable: false }},
  }},
}});

export const Post = type_("Post", {{
  sqlSource: "v_post",
  fields: {{
    pk: {{ type: "Int", nullable: false }},
    id: {{ type: "ID", nullable: false }},
    identifier: {{ type: "String", nullable: false }},
    title: {{ type: "String", nullable: false }},
    body: {{ type: "String", nullable: false }},
    published: {{ type: "Boolean", nullable: false }},
    author_id: {{ type: "ID", nullable: false }},
    created_at: {{ type: "DateTime", nullable: false }},
    updated_at: {{ type: "DateTime", nullable: false }},
  }},
}});

export const Comment = type_("Comment", {{
  sqlSource: "v_comment",
  fields: {{
    pk: {{ type: "Int", nullable: false }},
    id: {{ type: "ID", nullable: false }},
    body: {{ type: "String", nullable: false }},
    author_name: {{ type: "String", nullable: false }},
    post_id: {{ type: "ID", nullable: false }},
    created_at: {{ type: "DateTime", nullable: false }},
  }},
}});

export const Tag = type_("Tag", {{
  sqlSource: "v_tag",
  fields: {{
    pk: {{ type: "Int", nullable: false }},
    id: {{ type: "ID", nullable: false }},
    identifier: {{ type: "String", nullable: false }},
    name: {{ type: "String", nullable: false }},
  }},
}});

export const posts = query("posts", {{
  returnType: "Post",
  returnArray: true,
  sqlSource: "v_post",
}});

export const post = query("post", {{
  returnType: "Post",
  returnArray: false,
  sqlSource: "v_post",
  args: [{{ name: "id", type: "ID", required: true }}],
}});

export const authors = query("authors", {{
  returnType: "Author",
  returnArray: true,
  sqlSource: "v_author",
}});

export const author = query("author", {{
  returnType: "Author",
  returnArray: false,
  sqlSource: "v_author",
  args: [{{ name: "id", type: "ID", required: true }}],
}});

export const tagsQuery = query("tags", {{
  returnType: "Tag",
  returnArray: true,
  sqlSource: "v_tag",
}});
"#,
        name = config.project_name,
    );

    fs::write(dir.join("schema.ts"), content).context("Failed to create schema.ts")?;
    info!("Created schema/schema.ts");
    Ok(())
}

fn create_rust_skeleton(project_dir: &Path, config: &InitConfig) -> Result<()> {
    let dir = project_dir.join("schema");
    fs::create_dir_all(&dir).context("Failed to create schema/ directory")?;

    let content = format!(
        r#"//! FraiseQL blog schema definition for {name}.

use fraiseql::{{type_, query}};

/// Blog author with trinity pattern.
#[type_(sql_source = "v_author")]
pub struct Author {{
    pub pk: i32,
    pub id: ID,
    pub identifier: String,
    pub name: String,
    pub email: String,
    pub bio: Option<String>,
    pub created_at: DateTime,
    pub updated_at: DateTime,
}}

/// Blog post with trinity pattern.
#[type_(sql_source = "v_post")]
pub struct Post {{
    pub pk: i32,
    pub id: ID,
    pub identifier: String,
    pub title: String,
    pub body: String,
    pub published: bool,
    pub author_id: ID,
    pub created_at: DateTime,
    pub updated_at: DateTime,
}}

/// Comment on a blog post.
#[type_(sql_source = "v_comment")]
pub struct Comment {{
    pub pk: i32,
    pub id: ID,
    pub body: String,
    pub author_name: String,
    pub post_id: ID,
    pub created_at: DateTime,
}}

/// Categorization tag for posts.
#[type_(sql_source = "v_tag")]
pub struct Tag {{
    pub pk: i32,
    pub id: ID,
    pub identifier: String,
    pub name: String,
}}

#[query(return_type = "Post", return_array = true, sql_source = "v_post")]
pub fn posts() -> Vec<Post> {{
    unimplemented!("Schema definition only")
}}

#[query(return_type = "Post", sql_source = "v_post")]
pub fn post(id: ID) -> Post {{
    unimplemented!("Schema definition only")
}}

#[query(return_type = "Author", return_array = true, sql_source = "v_author")]
pub fn authors() -> Vec<Author> {{
    unimplemented!("Schema definition only")
}}

#[query(return_type = "Author", sql_source = "v_author")]
pub fn author(id: ID) -> Author {{
    unimplemented!("Schema definition only")
}}

#[query(return_type = "Tag", return_array = true, sql_source = "v_tag")]
pub fn tags() -> Vec<Tag> {{
    unimplemented!("Schema definition only")
}}
"#,
        name = config.project_name,
    );

    fs::write(dir.join("schema.rs"), content).context("Failed to create schema.rs")?;
    info!("Created schema/schema.rs");
    Ok(())
}

fn create_java_skeleton(project_dir: &Path, config: &InitConfig) -> Result<()> {
    let dir = project_dir.join("schema");
    fs::create_dir_all(&dir).context("Failed to create schema/ directory")?;

    let content = format!(
        r#"// FraiseQL blog schema definition for {name}.

package schema;

import fraiseql.FraiseQL;
import fraiseql.annotations.*;

/// Blog author with trinity pattern.
@Type(sqlSource = "v_author")
public record Author(
    int pk,
    ID id,
    String identifier,
    String name,
    String email,
    @Nullable String bio,
    DateTime createdAt,
    DateTime updatedAt
) {{}}

/// Blog post with trinity pattern.
@Type(sqlSource = "v_post")
public record Post(
    int pk,
    ID id,
    String identifier,
    String title,
    String body,
    boolean published,
    ID authorId,
    DateTime createdAt,
    DateTime updatedAt
) {{}}

/// Comment on a blog post.
@Type(sqlSource = "v_comment")
public record Comment(
    int pk,
    ID id,
    String body,
    String authorName,
    ID postId,
    DateTime createdAt
) {{}}

/// Categorization tag for posts.
@Type(sqlSource = "v_tag")
public record Tag(
    int pk,
    ID id,
    String identifier,
    String name
) {{}}

@Query(returnType = Post.class, returnArray = true, sqlSource = "v_post")
public interface Posts {{}}

@Query(returnType = Post.class, sqlSource = "v_post", args = @Arg(name = "id", type = "ID", required = true))
public interface PostById {{}}

@Query(returnType = Author.class, returnArray = true, sqlSource = "v_author")
public interface Authors {{}}

@Query(returnType = Author.class, sqlSource = "v_author", args = @Arg(name = "id", type = "ID", required = true))
public interface AuthorById {{}}

@Query(returnType = Tag.class, returnArray = true, sqlSource = "v_tag")
public interface Tags {{}}
"#,
        name = config.project_name,
    );

    fs::write(dir.join("schema.java"), content).context("Failed to create schema.java")?;
    info!("Created schema/schema.java");
    Ok(())
}

fn create_kotlin_skeleton(project_dir: &Path, config: &InitConfig) -> Result<()> {
    let dir = project_dir.join("schema");
    fs::create_dir_all(&dir).context("Failed to create schema/ directory")?;

    let content = format!(
        r#"// FraiseQL blog schema definition for {name}.

package schema

import fraiseql.*

/// Blog author with trinity pattern.
@Type(sqlSource = "v_author")
data class Author(
    val pk: Int,
    val id: ID,
    val identifier: String,
    val name: String,
    val email: String,
    val bio: String?,
    val createdAt: DateTime,
    val updatedAt: DateTime,
)

/// Blog post with trinity pattern.
@Type(sqlSource = "v_post")
data class Post(
    val pk: Int,
    val id: ID,
    val identifier: String,
    val title: String,
    val body: String,
    val published: Boolean,
    val authorId: ID,
    val createdAt: DateTime,
    val updatedAt: DateTime,
)

/// Comment on a blog post.
@Type(sqlSource = "v_comment")
data class Comment(
    val pk: Int,
    val id: ID,
    val body: String,
    val authorName: String,
    val postId: ID,
    val createdAt: DateTime,
)

/// Categorization tag for posts.
@Type(sqlSource = "v_tag")
data class Tag(
    val pk: Int,
    val id: ID,
    val identifier: String,
    val name: String,
)

@Query(returnType = Post::class, returnArray = true, sqlSource = "v_post")
fun posts(): List<Post> = TODO("Schema definition only")

@Query(returnType = Post::class, sqlSource = "v_post")
fun post(id: ID): Post = TODO("Schema definition only")

@Query(returnType = Author::class, returnArray = true, sqlSource = "v_author")
fun authors(): List<Author> = TODO("Schema definition only")

@Query(returnType = Author::class, sqlSource = "v_author")
fun author(id: ID): Author = TODO("Schema definition only")

@Query(returnType = Tag::class, returnArray = true, sqlSource = "v_tag")
fun tags(): List<Tag> = TODO("Schema definition only")
"#,
        name = config.project_name,
    );

    fs::write(dir.join("schema.kt"), content).context("Failed to create schema.kt")?;
    info!("Created schema/schema.kt");
    Ok(())
}

fn create_go_skeleton(project_dir: &Path, config: &InitConfig) -> Result<()> {
    let dir = project_dir.join("schema");
    fs::create_dir_all(&dir).context("Failed to create schema/ directory")?;

    let content = format!(
        r#"// FraiseQL blog schema definition for {name}.

package schema

import "fraiseql"

// Author - Blog author with trinity pattern.
// @Type(sqlSource = "v_author")
type Author struct {{
	PK         int      `fraiseql:"pk"`
	ID         ID       `fraiseql:"id"`
	Identifier string   `fraiseql:"identifier"`
	Name       string   `fraiseql:"name"`
	Email      string   `fraiseql:"email"`
	Bio        *string  `fraiseql:"bio"`
	CreatedAt  DateTime `fraiseql:"created_at"`
	UpdatedAt  DateTime `fraiseql:"updated_at"`
}}

// Post - Blog post with trinity pattern.
// @Type(sqlSource = "v_post")
type Post struct {{
	PK         int      `fraiseql:"pk"`
	ID         ID       `fraiseql:"id"`
	Identifier string   `fraiseql:"identifier"`
	Title      string   `fraiseql:"title"`
	Body       string   `fraiseql:"body"`
	Published  bool     `fraiseql:"published"`
	AuthorID   ID       `fraiseql:"author_id"`
	CreatedAt  DateTime `fraiseql:"created_at"`
	UpdatedAt  DateTime `fraiseql:"updated_at"`
}}

// Comment - Comment on a blog post.
// @Type(sqlSource = "v_comment")
type Comment struct {{
	PK         int      `fraiseql:"pk"`
	ID         ID       `fraiseql:"id"`
	Body       string   `fraiseql:"body"`
	AuthorName string   `fraiseql:"author_name"`
	PostID     ID       `fraiseql:"post_id"`
	CreatedAt  DateTime `fraiseql:"created_at"`
}}

// Tag - Categorization tag for posts.
// @Type(sqlSource = "v_tag")
type Tag struct {{
	PK         int    `fraiseql:"pk"`
	ID         ID     `fraiseql:"id"`
	Identifier string `fraiseql:"identifier"`
	Name       string `fraiseql:"name"`
}}

// Queries are registered via fraiseql.RegisterQuery().
func init() {{
	fraiseql.RegisterQuery("posts", fraiseql.QueryDef{{ReturnType: "Post", ReturnArray: true, SQLSource: "v_post"}})
	fraiseql.RegisterQuery("post", fraiseql.QueryDef{{ReturnType: "Post", SQLSource: "v_post", Args: []fraiseql.Arg{{{{Name: "id", Type: "ID", Required: true}}}}}})
	fraiseql.RegisterQuery("authors", fraiseql.QueryDef{{ReturnType: "Author", ReturnArray: true, SQLSource: "v_author"}})
	fraiseql.RegisterQuery("author", fraiseql.QueryDef{{ReturnType: "Author", SQLSource: "v_author", Args: []fraiseql.Arg{{{{Name: "id", Type: "ID", Required: true}}}}}})
	fraiseql.RegisterQuery("tags", fraiseql.QueryDef{{ReturnType: "Tag", ReturnArray: true, SQLSource: "v_tag"}})
}}
"#,
        name = config.project_name,
    );

    fs::write(dir.join("schema.go"), content).context("Failed to create schema.go")?;
    info!("Created schema/schema.go");
    Ok(())
}

fn create_csharp_skeleton(project_dir: &Path, config: &InitConfig) -> Result<()> {
    let dir = project_dir.join("schema");
    fs::create_dir_all(&dir).context("Failed to create schema/ directory")?;

    let content = format!(
        r#"// FraiseQL blog schema definition for {name}.

using FraiseQL;

namespace Schema;

/// Blog author with trinity pattern.
[Type(SqlSource = "v_author")]
public record Author(
    int Pk,
    ID Id,
    string Identifier,
    string Name,
    string Email,
    string? Bio,
    DateTime CreatedAt,
    DateTime UpdatedAt
);

/// Blog post with trinity pattern.
[Type(SqlSource = "v_post")]
public record Post(
    int Pk,
    ID Id,
    string Identifier,
    string Title,
    string Body,
    bool Published,
    ID AuthorId,
    DateTime CreatedAt,
    DateTime UpdatedAt
);

/// Comment on a blog post.
[Type(SqlSource = "v_comment")]
public record Comment(
    int Pk,
    ID Id,
    string Body,
    string AuthorName,
    ID PostId,
    DateTime CreatedAt
);

/// Categorization tag for posts.
[Type(SqlSource = "v_tag")]
public record Tag(
    int Pk,
    ID Id,
    string Identifier,
    string Name
);

[Query(ReturnType = typeof(Post), ReturnArray = true, SqlSource = "v_post")]
public static partial class Posts;

[Query(ReturnType = typeof(Post), SqlSource = "v_post", Arg(Name = "id", Type = "ID", Required = true))]
public static partial class PostById;

[Query(ReturnType = typeof(Author), ReturnArray = true, SqlSource = "v_author")]
public static partial class Authors;

[Query(ReturnType = typeof(Author), SqlSource = "v_author", Arg(Name = "id", Type = "ID", Required = true))]
public static partial class AuthorById;

[Query(ReturnType = typeof(Tag), ReturnArray = true, SqlSource = "v_tag")]
public static partial class Tags;
"#,
        name = config.project_name,
    );

    fs::write(dir.join("schema.cs"), content).context("Failed to create schema.cs")?;
    info!("Created schema/schema.cs");
    Ok(())
}

fn create_swift_skeleton(project_dir: &Path, config: &InitConfig) -> Result<()> {
    let dir = project_dir.join("schema");
    fs::create_dir_all(&dir).context("Failed to create schema/ directory")?;

    let content = format!(
        r#"// FraiseQL blog schema definition for {name}.

import FraiseQL

/// Blog author with trinity pattern.
@Type(sqlSource: "v_author")
struct Author {{
    let pk: Int
    let id: ID
    let identifier: String
    let name: String
    let email: String
    let bio: String?
    let createdAt: DateTime
    let updatedAt: DateTime
}}

/// Blog post with trinity pattern.
@Type(sqlSource: "v_post")
struct Post {{
    let pk: Int
    let id: ID
    let identifier: String
    let title: String
    let body: String
    let published: Bool
    let authorId: ID
    let createdAt: DateTime
    let updatedAt: DateTime
}}

/// Comment on a blog post.
@Type(sqlSource: "v_comment")
struct Comment {{
    let pk: Int
    let id: ID
    let body: String
    let authorName: String
    let postId: ID
    let createdAt: DateTime
}}

/// Categorization tag for posts.
@Type(sqlSource: "v_tag")
struct Tag {{
    let pk: Int
    let id: ID
    let identifier: String
    let name: String
}}

@Query(returnType: Post.self, returnArray: true, sqlSource: "v_post")
func posts() -> [Post] {{ fatalError("Schema definition only") }}

@Query(returnType: Post.self, sqlSource: "v_post")
func post(id: ID) -> Post {{ fatalError("Schema definition only") }}

@Query(returnType: Author.self, returnArray: true, sqlSource: "v_author")
func authors() -> [Author] {{ fatalError("Schema definition only") }}

@Query(returnType: Author.self, sqlSource: "v_author")
func author(id: ID) -> Author {{ fatalError("Schema definition only") }}

@Query(returnType: Tag.self, returnArray: true, sqlSource: "v_tag")
func tags() -> [Tag] {{ fatalError("Schema definition only") }}
"#,
        name = config.project_name,
    );

    fs::write(dir.join("schema.swift"), content).context("Failed to create schema.swift")?;
    info!("Created schema/schema.swift");
    Ok(())
}

fn create_scala_skeleton(project_dir: &Path, config: &InitConfig) -> Result<()> {
    let dir = project_dir.join("schema");
    fs::create_dir_all(&dir).context("Failed to create schema/ directory")?;

    let content = format!(
        r#"// FraiseQL blog schema definition for {name}.

package schema

import fraiseql._

/// Blog author with trinity pattern.
@Type(sqlSource = "v_author")
case class Author(
  pk: Int,
  id: ID,
  identifier: String,
  name: String,
  email: String,
  bio: Option[String],
  createdAt: DateTime,
  updatedAt: DateTime
)

/// Blog post with trinity pattern.
@Type(sqlSource = "v_post")
case class Post(
  pk: Int,
  id: ID,
  identifier: String,
  title: String,
  body: String,
  published: Boolean,
  authorId: ID,
  createdAt: DateTime,
  updatedAt: DateTime
)

/// Comment on a blog post.
@Type(sqlSource = "v_comment")
case class Comment(
  pk: Int,
  id: ID,
  body: String,
  authorName: String,
  postId: ID,
  createdAt: DateTime
)

/// Categorization tag for posts.
@Type(sqlSource = "v_tag")
case class Tag(
  pk: Int,
  id: ID,
  identifier: String,
  name: String
)

@Query(returnType = classOf[Post], returnArray = true, sqlSource = "v_post")
def posts(): List[Post] = ???

@Query(returnType = classOf[Post], sqlSource = "v_post")
def post(id: ID): Post = ???

@Query(returnType = classOf[Author], returnArray = true, sqlSource = "v_author")
def authors(): List[Author] = ???

@Query(returnType = classOf[Author], sqlSource = "v_author")
def author(id: ID): Author = ???

@Query(returnType = classOf[Tag], returnArray = true, sqlSource = "v_tag")
def tags(): List[Tag] = ???
"#,
        name = config.project_name,
    );

    fs::write(dir.join("schema.scala"), content).context("Failed to create schema.scala")?;
    info!("Created schema/schema.scala");
    Ok(())
}

fn init_git(project_dir: &Path) -> Result<()> {
    let status = Command::new("git")
        .args(["init"])
        .current_dir(project_dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    match status {
        Ok(s) if s.success() => {
            info!("Initialized git repository");
            Ok(())
        },
        Ok(_) => {
            // git init failed but non-fatal
            eprintln!("Warning: git init failed. You can initialize git manually.");
            Ok(())
        },
        Err(_) => {
            eprintln!("Warning: git not found. Skipping repository initialization.");
            Ok(())
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_from_str() {
        assert_eq!(Language::from_str("python").unwrap(), Language::Python);
        assert_eq!(Language::from_str("py").unwrap(), Language::Python);
        assert_eq!(Language::from_str("typescript").unwrap(), Language::TypeScript);
        assert_eq!(Language::from_str("ts").unwrap(), Language::TypeScript);
        assert_eq!(Language::from_str("rust").unwrap(), Language::Rust);
        assert_eq!(Language::from_str("rs").unwrap(), Language::Rust);
        assert_eq!(Language::from_str("java").unwrap(), Language::Java);
        assert_eq!(Language::from_str("jav").unwrap(), Language::Java);
        assert_eq!(Language::from_str("kotlin").unwrap(), Language::Kotlin);
        assert_eq!(Language::from_str("kt").unwrap(), Language::Kotlin);
        assert_eq!(Language::from_str("go").unwrap(), Language::Go);
        assert_eq!(Language::from_str("golang").unwrap(), Language::Go);
        assert_eq!(Language::from_str("csharp").unwrap(), Language::CSharp);
        assert_eq!(Language::from_str("c#").unwrap(), Language::CSharp);
        assert_eq!(Language::from_str("cs").unwrap(), Language::CSharp);
        assert_eq!(Language::from_str("swift").unwrap(), Language::Swift);
        assert_eq!(Language::from_str("scala").unwrap(), Language::Scala);
        assert_eq!(Language::from_str("sc").unwrap(), Language::Scala);
        assert!(Language::from_str("haskell").is_err());
    }

    #[test]
    fn test_language_from_extension() {
        assert_eq!(Language::from_extension("py"), Some(Language::Python));
        assert_eq!(Language::from_extension("ts"), Some(Language::TypeScript));
        assert_eq!(Language::from_extension("tsx"), Some(Language::TypeScript));
        assert_eq!(Language::from_extension("rs"), Some(Language::Rust));
        assert_eq!(Language::from_extension("java"), Some(Language::Java));
        assert_eq!(Language::from_extension("kt"), Some(Language::Kotlin));
        assert_eq!(Language::from_extension("kts"), Some(Language::Kotlin));
        assert_eq!(Language::from_extension("go"), Some(Language::Go));
        assert_eq!(Language::from_extension("cs"), Some(Language::CSharp));
        assert_eq!(Language::from_extension("swift"), Some(Language::Swift));
        assert_eq!(Language::from_extension("scala"), Some(Language::Scala));
        assert_eq!(Language::from_extension("sc"), Some(Language::Scala));
        assert_eq!(Language::from_extension("rb"), None);
        assert_eq!(Language::from_extension(""), None);
    }

    #[test]
    fn test_database_from_str() {
        assert_eq!(Database::from_str("postgres").unwrap(), Database::Postgres);
        assert_eq!(Database::from_str("postgresql").unwrap(), Database::Postgres);
        assert_eq!(Database::from_str("pg").unwrap(), Database::Postgres);
        assert_eq!(Database::from_str("mysql").unwrap(), Database::Mysql);
        assert_eq!(Database::from_str("sqlite").unwrap(), Database::Sqlite);
        assert_eq!(Database::from_str("sqlserver").unwrap(), Database::SqlServer);
        assert_eq!(Database::from_str("mssql").unwrap(), Database::SqlServer);
        assert!(Database::from_str("oracle").is_err());
    }

    #[test]
    fn test_size_from_str() {
        assert_eq!(ProjectSize::from_str("xs").unwrap(), ProjectSize::Xs);
        assert_eq!(ProjectSize::from_str("s").unwrap(), ProjectSize::S);
        assert_eq!(ProjectSize::from_str("m").unwrap(), ProjectSize::M);
        assert!(ProjectSize::from_str("l").is_err());
    }

    #[test]
    fn test_database_default_url() {
        assert_eq!(Database::Postgres.default_url("myapp"), "postgresql://localhost/myapp");
        assert_eq!(Database::Sqlite.default_url("myapp"), "myapp.db");
    }

    #[test]
    fn test_init_creates_project() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path().join("test_project");

        let config = InitConfig {
            project_name: project_dir.to_string_lossy().to_string(),
            language:     Language::Python,
            database:     Database::Postgres,
            size:         ProjectSize::S,
            no_git:       true,
        };

        run(&config).unwrap();

        // Verify files exist
        assert!(project_dir.join(".gitignore").exists());
        assert!(project_dir.join("fraiseql.toml").exists());
        assert!(project_dir.join("schema.json").exists());
        assert!(project_dir.join("db/0_schema/01_write/011_tb_author.sql").exists());
        assert!(project_dir.join("db/0_schema/01_write/012_tb_post.sql").exists());
        assert!(project_dir.join("db/0_schema/01_write/013_tb_comment.sql").exists());
        assert!(project_dir.join("db/0_schema/01_write/014_tb_tag.sql").exists());
        assert!(project_dir.join("db/0_schema/02_read/021_v_author.sql").exists());
        assert!(project_dir.join("db/0_schema/03_functions/031_fn_author_crud.sql").exists());
        // Selected language skeleton only
        assert!(project_dir.join("schema/schema.py").exists());
        assert!(!project_dir.join("schema/schema.ts").exists());
        assert!(!project_dir.join("schema/schema.rs").exists());
    }

    #[test]
    fn test_init_xs_layout() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path().join("test_xs");

        let config = InitConfig {
            project_name: project_dir.to_string_lossy().to_string(),
            language:     Language::TypeScript,
            database:     Database::Postgres,
            size:         ProjectSize::Xs,
            no_git:       true,
        };

        run(&config).unwrap();

        assert!(project_dir.join("db/0_schema/schema.sql").exists());
        assert!(project_dir.join("schema/schema.ts").exists());

        // Should NOT have the numbered directories
        assert!(!project_dir.join("db/0_schema/01_write").exists());
    }

    #[test]
    fn test_init_m_layout() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path().join("test_m");

        let config = InitConfig {
            project_name: project_dir.to_string_lossy().to_string(),
            language:     Language::Rust,
            database:     Database::Postgres,
            size:         ProjectSize::M,
            no_git:       true,
        };

        run(&config).unwrap();

        assert!(project_dir.join("db/0_schema/01_write/author/tb_author.sql").exists());
        assert!(project_dir.join("db/0_schema/01_write/post/tb_post.sql").exists());
        assert!(project_dir.join("db/0_schema/02_read/author/v_author.sql").exists());
        assert!(project_dir.join("db/0_schema/03_functions/author/fn_author_crud.sql").exists());
        assert!(project_dir.join("schema/schema.rs").exists());
    }

    #[test]
    fn test_init_refuses_existing_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path().join("existing");

        fs::create_dir(&project_dir).unwrap();

        let config = InitConfig {
            project_name: project_dir.to_string_lossy().to_string(),
            language:     Language::Python,
            database:     Database::Postgres,
            size:         ProjectSize::S,
            no_git:       true,
        };

        let result = run(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn test_toml_config_is_valid() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path().join("toml_test");

        let config = InitConfig {
            project_name: project_dir.to_string_lossy().to_string(),
            language:     Language::Python,
            database:     Database::Postgres,
            size:         ProjectSize::S,
            no_git:       true,
        };

        run(&config).unwrap();

        // Verify the TOML can be parsed
        let toml_content = fs::read_to_string(project_dir.join("fraiseql.toml")).unwrap();
        let parsed: toml::Value = toml::from_str(&toml_content).unwrap();
        // project name in TOML is the full path since we pass absolute paths
        assert!(parsed["project"]["name"].as_str().is_some());
    }

    #[test]
    fn test_schema_json_is_valid() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path().join("json_test");

        let config = InitConfig {
            project_name: project_dir.to_string_lossy().to_string(),
            language:     Language::Python,
            database:     Database::Postgres,
            size:         ProjectSize::Xs,
            no_git:       true,
        };

        run(&config).unwrap();

        let json_content = fs::read_to_string(project_dir.join("schema.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_content).unwrap();

        // IntermediateSchema format: arrays, not maps
        assert!(parsed["types"].is_array(), "types should be an array");
        assert!(parsed["queries"].is_array(), "queries should be an array");
        assert_eq!(parsed["types"][0]["name"], "Author");
        assert_eq!(parsed["types"][1]["name"], "Post");
        assert_eq!(parsed["types"][2]["name"], "Comment");
        assert_eq!(parsed["types"][3]["name"], "Tag");
        assert_eq!(parsed["queries"][0]["name"], "posts");
        assert_eq!(parsed["version"], "2.0.0");
    }
}
