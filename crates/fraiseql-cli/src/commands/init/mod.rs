//! `fraiseql init` - Interactive project scaffolder
//!
//! Creates a new FraiseQL project with the correct directory structure,
//! configuration files, and authoring skeleton in the chosen language.

mod skeletons;
mod sql_templates;

use std::{
    fmt, fs,
    path::{Path, PathBuf},
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
    /// PHP authoring
    Php,
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
            Self::Php => write!(f, "php"),
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
            "php" => Some(Self::Php),
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
            "php" => Ok(Self::Php),
            other => Err(format!(
                "Unknown language: {other}. Choose: python, typescript, rust, java, kotlin, go, csharp, swift, scala, php"
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
///
/// # Errors
///
/// Returns an error if the operation fails.
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
    skeletons::create_authoring_skeleton(&project_dir, config)?;

    // Initialize git repository
    if !config.no_git {
        skeletons::init_git(&project_dir)?;
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

    let content = sql_templates::generate_single_schema_sql(config.database);
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
        let (table_sql, view_sql, fn_sql) =
            sql_templates::generate_blog_entity_sql(config.database, entity);
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

        let (table_sql, view_sql, fn_sql) =
            sql_templates::generate_blog_entity_sql(config.database, entity);
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

#[cfg(test)]
mod tests;
