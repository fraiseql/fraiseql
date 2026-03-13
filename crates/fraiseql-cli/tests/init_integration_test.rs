#![allow(clippy::unwrap_used)]  // Reason: test/bench code, panics are acceptable
//! Integration tests for `fraiseql init`
//!
//! Verifies that scaffolded projects are valid and compilable:
//! 1. Directory structure matches expected layout
//! 2. Generated schema.json is valid `IntermediateSchema`
//! 3. Generated fraiseql.toml is parseable
//! 4. `fraiseql compile` succeeds on the scaffolded project
//! 5. All language/database/size combinations produce valid projects

use std::{fs, process::Command};

use tempfile::TempDir;

/// Helper: get the CLI binary path
fn cli_bin() -> String {
    env!("CARGO_BIN_EXE_fraiseql-cli").to_string()
}

/// Helper: run `fraiseql init` with given args, return (success, stdout, stderr)
fn run_init(temp_dir: &TempDir, project_name: &str, args: &[&str]) -> (bool, String, String) {
    let mut cmd = Command::new(cli_bin());
    cmd.current_dir(temp_dir.path()).arg("init").arg(project_name);
    for arg in args {
        cmd.arg(arg);
    }

    let output = cmd.output().expect("Failed to run fraiseql-cli init");

    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

/// Helper: run `fraiseql compile` on a scaffolded project, return (success, stdout, stderr)
fn run_compile(project_dir: &std::path::Path) -> (bool, String, String) {
    let schema_path = project_dir.join("schema.json");
    let output_path = project_dir.join("schema.compiled.json");

    let output = Command::new(cli_bin())
        .current_dir(project_dir)
        .args([
            "compile",
            schema_path.to_str().unwrap(),
            "--output",
            output_path.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to run fraiseql-cli compile");

    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

// ============================================================================
// Structure tests: verify scaffolded directories and files exist
// ============================================================================

#[test]
fn test_init_default_creates_expected_structure() {
    let tmp = TempDir::new().unwrap();
    let (ok, _stdout, stderr) = run_init(&tmp, "myapp", &["--no-git"]);
    assert!(ok, "init failed: {stderr}");

    let root = tmp.path().join("myapp");

    // Core files
    assert!(root.join(".gitignore").exists(), "Missing .gitignore");
    assert!(root.join("fraiseql.toml").exists(), "Missing fraiseql.toml");
    assert!(root.join("schema.json").exists(), "Missing schema.json");

    // Default size=s directory structure
    assert!(root.join("db/0_schema/01_write").is_dir(), "Missing 01_write/");
    assert!(root.join("db/0_schema/02_read").is_dir(), "Missing 02_read/");
    assert!(root.join("db/0_schema/03_functions").is_dir(), "Missing 03_functions/");

    // SQL files (blog entities)
    assert!(root.join("db/0_schema/01_write/011_tb_author.sql").exists());
    assert!(root.join("db/0_schema/01_write/012_tb_post.sql").exists());
    assert!(root.join("db/0_schema/01_write/013_tb_comment.sql").exists());
    assert!(root.join("db/0_schema/01_write/014_tb_tag.sql").exists());
    assert!(root.join("db/0_schema/02_read/021_v_author.sql").exists());
    assert!(root.join("db/0_schema/03_functions/031_fn_author_crud.sql").exists());

    // Default language=python skeleton
    assert!(root.join("schema/schema.py").exists(), "Missing schema.py");
}

#[test]
fn test_init_xs_creates_single_file() {
    let tmp = TempDir::new().unwrap();
    let (ok, _, stderr) = run_init(&tmp, "tiny", &["--size", "xs", "--no-git"]);
    assert!(ok, "init failed: {stderr}");

    let root = tmp.path().join("tiny");
    assert!(root.join("db/0_schema/schema.sql").exists(), "Missing schema.sql");

    // Should NOT have the numbered directories
    assert!(!root.join("db/0_schema/01_write").exists());
    assert!(!root.join("db/0_schema/02_read").exists());
}

#[test]
fn test_init_m_creates_per_entity_dirs() {
    let tmp = TempDir::new().unwrap();
    let (ok, _, stderr) = run_init(&tmp, "large", &["--size", "m", "--no-git"]);
    assert!(ok, "init failed: {stderr}");

    let root = tmp.path().join("large");
    assert!(root.join("db/0_schema/01_write/author/tb_author.sql").exists());
    assert!(root.join("db/0_schema/01_write/post/tb_post.sql").exists());
    assert!(root.join("db/0_schema/01_write/comment/tb_comment.sql").exists());
    assert!(root.join("db/0_schema/01_write/tag/tb_tag.sql").exists());
    assert!(root.join("db/0_schema/02_read/author/v_author.sql").exists());
    assert!(root.join("db/0_schema/03_functions/post/fn_post_crud.sql").exists());
}

#[test]
fn test_init_default_generates_python_skeleton() {
    let tmp = TempDir::new().unwrap();
    let (ok, _, stderr) = run_init(&tmp, "pyapp", &["--no-git"]);
    assert!(ok, "init failed: {stderr}");

    let root = tmp.path().join("pyapp");
    assert!(root.join("schema/schema.py").exists(), "Missing schema.py");
    assert!(!root.join("schema/schema.ts").exists(), "Should not generate schema.ts");
    assert!(!root.join("schema/schema.rs").exists(), "Should not generate schema.rs");

    let py = fs::read_to_string(root.join("schema/schema.py")).unwrap();
    assert!(py.contains("import fraiseql"), "Python skeleton should have imports");
    assert!(py.contains("class Author"), "Python skeleton should define Author");
    assert!(py.contains("class Post"), "Python skeleton should define Post");
    assert!(py.contains("class Comment"), "Python skeleton should define Comment");
    assert!(py.contains("class Tag"), "Python skeleton should define Tag");
}

#[test]
fn test_init_typescript_skeleton() {
    let tmp = TempDir::new().unwrap();
    let (ok, _, stderr) = run_init(&tmp, "tsapp", &["--language", "typescript", "--no-git"]);
    assert!(ok, "init failed: {stderr}");

    let root = tmp.path().join("tsapp");
    assert!(root.join("schema/schema.ts").exists(), "Missing schema.ts");
    assert!(!root.join("schema/schema.py").exists(), "Should not generate schema.py");

    let ts = fs::read_to_string(root.join("schema/schema.ts")).unwrap();
    assert!(ts.contains("import"), "TypeScript skeleton should have imports");
    assert!(ts.contains("Author"), "TypeScript skeleton should define Author");
    assert!(ts.contains("Post"), "TypeScript skeleton should define Post");
}

#[test]
fn test_init_rust_skeleton() {
    let tmp = TempDir::new().unwrap();
    let (ok, _, stderr) = run_init(&tmp, "rsapp", &["--language", "rust", "--no-git"]);
    assert!(ok, "init failed: {stderr}");

    let root = tmp.path().join("rsapp");
    assert!(root.join("schema/schema.rs").exists(), "Missing schema.rs");
    assert!(!root.join("schema/schema.py").exists(), "Should not generate schema.py");

    let rs = fs::read_to_string(root.join("schema/schema.rs")).unwrap();
    assert!(rs.contains("pub struct Author"), "Rust skeleton should define Author");
    assert!(rs.contains("pub struct Post"), "Rust skeleton should define Post");
    assert!(rs.contains("pub struct Tag"), "Rust skeleton should define Tag");
    assert!(!rs.contains("unimplemented!"), "Rust skeleton must not use unimplemented! (use todo! instead)");
}

#[test]
fn test_init_java_skeleton() {
    let tmp = TempDir::new().unwrap();
    let (ok, _, stderr) = run_init(&tmp, "javaapp", &["--language", "java", "--no-git"]);
    assert!(ok, "init failed: {stderr}");

    let root = tmp.path().join("javaapp");
    assert!(root.join("schema/schema.java").exists(), "Missing schema.java");
    assert!(!root.join("schema/schema.py").exists(), "Should not generate schema.py");

    let content = fs::read_to_string(root.join("schema/schema.java")).unwrap();
    assert!(content.contains("Author"), "Java skeleton should define Author");
    assert!(content.contains("Post"), "Java skeleton should define Post");
    assert!(content.contains("Comment"), "Java skeleton should define Comment");
    assert!(content.contains("Tag"), "Java skeleton should define Tag");
}

#[test]
fn test_init_kotlin_skeleton() {
    let tmp = TempDir::new().unwrap();
    let (ok, _, stderr) = run_init(&tmp, "ktapp", &["--language", "kotlin", "--no-git"]);
    assert!(ok, "init failed: {stderr}");

    let root = tmp.path().join("ktapp");
    assert!(root.join("schema/schema.kt").exists(), "Missing schema.kt");
    assert!(!root.join("schema/schema.py").exists(), "Should not generate schema.py");

    let content = fs::read_to_string(root.join("schema/schema.kt")).unwrap();
    assert!(content.contains("Author"), "Kotlin skeleton should define Author");
    assert!(content.contains("Post"), "Kotlin skeleton should define Post");
    assert!(content.contains("Comment"), "Kotlin skeleton should define Comment");
    assert!(content.contains("Tag"), "Kotlin skeleton should define Tag");
}

#[test]
fn test_init_go_skeleton() {
    let tmp = TempDir::new().unwrap();
    let (ok, _, stderr) = run_init(&tmp, "goapp", &["--language", "go", "--no-git"]);
    assert!(ok, "init failed: {stderr}");

    let root = tmp.path().join("goapp");
    assert!(root.join("schema/schema.go").exists(), "Missing schema.go");
    assert!(!root.join("schema/schema.py").exists(), "Should not generate schema.py");

    let content = fs::read_to_string(root.join("schema/schema.go")).unwrap();
    assert!(content.contains("Author"), "Go skeleton should define Author");
    assert!(content.contains("Post"), "Go skeleton should define Post");
    assert!(content.contains("Comment"), "Go skeleton should define Comment");
    assert!(content.contains("Tag"), "Go skeleton should define Tag");
}

#[test]
fn test_init_csharp_skeleton() {
    let tmp = TempDir::new().unwrap();
    let (ok, _, stderr) = run_init(&tmp, "csapp", &["--language", "csharp", "--no-git"]);
    assert!(ok, "init failed: {stderr}");

    let root = tmp.path().join("csapp");
    assert!(root.join("schema/schema.cs").exists(), "Missing schema.cs");
    assert!(!root.join("schema/schema.py").exists(), "Should not generate schema.py");

    let content = fs::read_to_string(root.join("schema/schema.cs")).unwrap();
    assert!(content.contains("Author"), "C# skeleton should define Author");
    assert!(content.contains("Post"), "C# skeleton should define Post");
    assert!(content.contains("Comment"), "C# skeleton should define Comment");
    assert!(content.contains("Tag"), "C# skeleton should define Tag");
}

#[test]
fn test_init_swift_skeleton() {
    let tmp = TempDir::new().unwrap();
    let (ok, _, stderr) = run_init(&tmp, "swiftapp", &["--language", "swift", "--no-git"]);
    assert!(ok, "init failed: {stderr}");

    let root = tmp.path().join("swiftapp");
    assert!(root.join("schema/schema.swift").exists(), "Missing schema.swift");
    assert!(!root.join("schema/schema.py").exists(), "Should not generate schema.py");

    let content = fs::read_to_string(root.join("schema/schema.swift")).unwrap();
    assert!(content.contains("Author"), "Swift skeleton should define Author");
    assert!(content.contains("Post"), "Swift skeleton should define Post");
    assert!(content.contains("Comment"), "Swift skeleton should define Comment");
    assert!(content.contains("Tag"), "Swift skeleton should define Tag");
}

#[test]
fn test_init_scala_skeleton() {
    let tmp = TempDir::new().unwrap();
    let (ok, _, stderr) = run_init(&tmp, "scalaapp", &["--language", "scala", "--no-git"]);
    assert!(ok, "init failed: {stderr}");

    let root = tmp.path().join("scalaapp");
    assert!(root.join("schema/schema.scala").exists(), "Missing schema.scala");
    assert!(!root.join("schema/schema.py").exists(), "Should not generate schema.py");

    let content = fs::read_to_string(root.join("schema/schema.scala")).unwrap();
    assert!(content.contains("Author"), "Scala skeleton should define Author");
    assert!(content.contains("Post"), "Scala skeleton should define Post");
    assert!(content.contains("Comment"), "Scala skeleton should define Comment");
    assert!(content.contains("Tag"), "Scala skeleton should define Tag");
}

// ============================================================================
// Configuration tests: verify generated files are parseable and correct
// ============================================================================

#[test]
fn test_init_fraiseql_toml_is_valid() {
    let tmp = TempDir::new().unwrap();
    let (ok, _, stderr) = run_init(&tmp, "cfgtest", &["--no-git"]);
    assert!(ok, "init failed: {stderr}");

    let toml_content = fs::read_to_string(tmp.path().join("cfgtest/fraiseql.toml")).unwrap();
    let parsed: toml::Value =
        toml::from_str(&toml_content).expect("fraiseql.toml should be valid TOML");

    assert_eq!(
        parsed["project"]["name"].as_str().unwrap(),
        "cfgtest",
        "Project name should match"
    );
    assert!(parsed["project"]["version"].as_str().is_some(), "Should have version");
    assert!(
        parsed["project"]["database_target"].as_str().is_some(),
        "Should have database_target"
    );
}

#[test]
fn test_init_mysql_sets_correct_database_target() {
    let tmp = TempDir::new().unwrap();
    let (ok, _, stderr) = run_init(&tmp, "myapp", &["--database", "mysql", "--no-git"]);
    assert!(ok, "init failed: {stderr}");

    let toml_content = fs::read_to_string(tmp.path().join("myapp/fraiseql.toml")).unwrap();
    let parsed: toml::Value = toml::from_str(&toml_content).unwrap();

    assert_eq!(parsed["project"]["database_target"].as_str().unwrap(), "mysql");
    // Database URL is in a comment line (not a TOML field) — check the raw content instead
    let toml_raw = fs::read_to_string(tmp.path().join("myapp/fraiseql.toml")).unwrap();
    assert!(toml_raw.contains("mysql"), "Database URL comment should mention mysql");
}

#[test]
fn test_init_schema_json_is_intermediate_format() {
    let tmp = TempDir::new().unwrap();
    let (ok, _, stderr) = run_init(&tmp, "schematest", &["--no-git"]);
    assert!(ok, "init failed: {stderr}");

    let json_content = fs::read_to_string(tmp.path().join("schematest/schema.json")).unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(&json_content).expect("schema.json should be valid JSON");

    // Must be IntermediateSchema format (arrays, not maps)
    assert!(parsed["types"].is_array(), "types must be an array");
    assert!(parsed["queries"].is_array(), "queries must be an array");
    assert!(parsed["mutations"].is_array(), "mutations must be an array");
    assert_eq!(parsed["version"].as_str().unwrap(), "2.0.0");

    // Verify blog type structure
    let type_names: Vec<&str> = parsed["types"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(type_names.contains(&"Author"), "Missing Author type");
    assert!(type_names.contains(&"Post"), "Missing Post type");
    assert!(type_names.contains(&"Comment"), "Missing Comment type");
    assert!(type_names.contains(&"Tag"), "Missing Tag type");

    // Verify Author has trinity pattern fields
    let author = &parsed["types"][0];
    assert_eq!(author["name"], "Author");
    let field_names: Vec<&str> = author["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert!(field_names.contains(&"pk"), "Missing trinity pk field");
    assert!(field_names.contains(&"id"), "Missing trinity id field");
    assert!(field_names.contains(&"identifier"), "Missing trinity identifier field");

    // Verify queries
    let query_names: Vec<&str> = parsed["queries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|q| q["name"].as_str().unwrap())
        .collect();
    assert!(query_names.contains(&"posts"), "Missing posts query");
    assert!(query_names.contains(&"post"), "Missing post query");
    assert!(query_names.contains(&"authors"), "Missing authors query");
    assert!(query_names.contains(&"tags"), "Missing tags query");
}

// ============================================================================
// Compilation tests: verify scaffolded project actually compiles
// ============================================================================

#[test]
fn test_init_project_compiles_successfully() {
    let tmp = TempDir::new().unwrap();
    let (ok, _, stderr) = run_init(&tmp, "compiletest", &["--no-git"]);
    assert!(ok, "init failed: {stderr}");

    let project_dir = tmp.path().join("compiletest");
    let (compile_ok, stdout, stderr) = run_compile(&project_dir);

    assert!(
        compile_ok,
        "fraiseql compile failed on scaffolded project.\nstdout: {stdout}\nstderr: {stderr}"
    );

    // Verify compiled output was created
    let compiled_path = project_dir.join("schema.compiled.json");
    assert!(compiled_path.exists(), "schema.compiled.json should exist after compilation");

    // Verify compiled schema is valid JSON with expected structure
    let compiled_content = fs::read_to_string(&compiled_path).unwrap();
    let compiled: serde_json::Value =
        serde_json::from_str(&compiled_content).expect("schema.compiled.json should be valid JSON");

    assert!(compiled["types"].is_array(), "Compiled schema should have types array");
    assert!(compiled["queries"].is_array(), "Compiled schema should have queries array");
}

#[test]
fn test_init_postgres_project_compiles() {
    let tmp = TempDir::new().unwrap();
    let (ok, _, stderr) = run_init(&tmp, "pgapp", &["--database", "postgres", "--no-git"]);
    assert!(ok, "init failed: {stderr}");

    let (compile_ok, stdout, stderr) = run_compile(&tmp.path().join("pgapp"));
    assert!(compile_ok, "compile failed: stdout={stdout}\nstderr={stderr}");
}

#[test]
fn test_init_mysql_project_compiles() {
    let tmp = TempDir::new().unwrap();
    let (ok, _, stderr) = run_init(&tmp, "myapp", &["--database", "mysql", "--no-git"]);
    assert!(ok, "init failed: {stderr}");

    let (compile_ok, stdout, stderr) = run_compile(&tmp.path().join("myapp"));
    assert!(compile_ok, "compile failed: stdout={stdout}\nstderr={stderr}");
}

#[test]
fn test_init_sqlite_project_compiles() {
    let tmp = TempDir::new().unwrap();
    let (ok, _, stderr) = run_init(&tmp, "sqliteapp", &["--database", "sqlite", "--no-git"]);
    assert!(ok, "init failed: {stderr}");

    let (compile_ok, stdout, stderr) = run_compile(&tmp.path().join("sqliteapp"));
    assert!(compile_ok, "compile failed: stdout={stdout}\nstderr={stderr}");
}

#[test]
fn test_init_sqlserver_project_compiles() {
    let tmp = TempDir::new().unwrap();
    let (ok, _, stderr) = run_init(&tmp, "mssqlapp", &["--database", "sqlserver", "--no-git"]);
    assert!(ok, "init failed: {stderr}");

    let (compile_ok, stdout, stderr) = run_compile(&tmp.path().join("mssqlapp"));
    assert!(compile_ok, "compile failed: stdout={stdout}\nstderr={stderr}");
}

#[test]
fn test_init_all_sizes_compile() {
    for size in &["xs", "s", "m"] {
        let tmp = TempDir::new().unwrap();
        let name = format!("size_{size}");
        let (ok, _, stderr) = run_init(&tmp, &name, &["--size", size, "--no-git"]);
        assert!(ok, "init --size {size} failed: {stderr}");

        let (compile_ok, stdout, stderr) = run_compile(&tmp.path().join(&name));
        assert!(compile_ok, "compile failed for size={size}: stdout={stdout}\nstderr={stderr}");
    }
}

#[test]
fn test_init_all_languages_compile() {
    for lang in &[
        "python",
        "typescript",
        "rust",
        "java",
        "kotlin",
        "go",
        "csharp",
        "swift",
        "scala",
    ] {
        let tmp = TempDir::new().unwrap();
        let name = format!("lang_{lang}");
        let (ok, _, stderr) = run_init(&tmp, &name, &["--language", lang, "--no-git"]);
        assert!(ok, "init --language {lang} failed: {stderr}");

        let (compile_ok, stdout, stderr) = run_compile(&tmp.path().join(&name));
        assert!(
            compile_ok,
            "compile failed for language={lang}: stdout={stdout}\nstderr={stderr}"
        );
    }
}

// ============================================================================
// Compiled schema content tests: verify the compiled output is meaningful
// ============================================================================

#[test]
fn test_compiled_schema_contains_blog_types() {
    let tmp = TempDir::new().unwrap();
    let (ok, _, stderr) = run_init(&tmp, "contenttest", &["--no-git"]);
    assert!(ok, "init failed: {stderr}");

    let project_dir = tmp.path().join("contenttest");
    let (compile_ok, stdout, stderr) = run_compile(&project_dir);
    assert!(compile_ok, "compile failed: stdout={stdout}\nstderr={stderr}");

    let compiled_content = fs::read_to_string(project_dir.join("schema.compiled.json")).unwrap();
    let compiled: serde_json::Value = serde_json::from_str(&compiled_content).unwrap();

    let types = compiled["types"].as_array().expect("types should be array");
    let type_names: Vec<&str> = types.iter().filter_map(|t| t["name"].as_str()).collect();

    assert!(type_names.contains(&"Author"), "Compiled schema should contain Author type");
    assert!(type_names.contains(&"Post"), "Compiled schema should contain Post type");
    assert!(type_names.contains(&"Comment"), "Compiled schema should contain Comment type");
    assert!(type_names.contains(&"Tag"), "Compiled schema should contain Tag type");

    // Verify Author has expected fields
    let author_type = types.iter().find(|t| t["name"] == "Author").unwrap();
    let field_names: Vec<&str> = author_type["fields"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|f| f["name"].as_str())
        .collect();
    assert!(field_names.contains(&"id"), "Author should have id field");
    assert!(field_names.contains(&"email"), "Author should have email field");
    assert!(field_names.contains(&"name"), "Author should have name field");
}

#[test]
fn test_compiled_schema_contains_queries() {
    let tmp = TempDir::new().unwrap();
    let (ok, _, stderr) = run_init(&tmp, "querytest", &["--no-git"]);
    assert!(ok, "init failed: {stderr}");

    let project_dir = tmp.path().join("querytest");
    let (compile_ok, stdout, stderr) = run_compile(&project_dir);
    assert!(compile_ok, "compile failed: stdout={stdout}\nstderr={stderr}");

    let compiled_content = fs::read_to_string(project_dir.join("schema.compiled.json")).unwrap();
    let compiled: serde_json::Value = serde_json::from_str(&compiled_content).unwrap();

    let queries = compiled["queries"].as_array().expect("queries should be array");
    let query_names: Vec<&str> = queries.iter().filter_map(|q| q["name"].as_str()).collect();

    assert!(query_names.contains(&"posts"), "Should have 'posts' query");
    assert!(query_names.contains(&"post"), "Should have 'post' query");
    assert!(query_names.contains(&"authors"), "Should have 'authors' query");
    assert!(query_names.contains(&"tags"), "Should have 'tags' query");
}

#[test]
fn test_compiled_schema_has_security_section() {
    let tmp = TempDir::new().unwrap();
    let (ok, _, stderr) = run_init(&tmp, "sectest", &["--no-git"]);
    assert!(ok, "init failed: {stderr}");

    let project_dir = tmp.path().join("sectest");
    let (compile_ok, stdout, stderr) = run_compile(&project_dir);
    assert!(compile_ok, "compile failed: stdout={stdout}\nstderr={stderr}");

    let compiled_content = fs::read_to_string(project_dir.join("schema.compiled.json")).unwrap();
    let compiled: serde_json::Value = serde_json::from_str(&compiled_content).unwrap();

    // Security config from fraiseql.toml should be embedded
    assert!(
        compiled.get("security").is_some(),
        "Compiled schema should have security section from fraiseql.toml"
    );
}

// ============================================================================
// Error handling tests
// ============================================================================

#[test]
fn test_init_refuses_existing_directory() {
    let tmp = TempDir::new().unwrap();

    // Create the directory first
    fs::create_dir(tmp.path().join("existing")).unwrap();

    let (ok, _, stderr) = run_init(&tmp, "existing", &["--no-git"]);
    assert!(!ok, "init should fail when directory exists");
    assert!(
        stderr.contains("already exists"),
        "Error message should mention existing directory: {stderr}"
    );
}

#[test]
fn test_init_rejects_invalid_language() {
    let tmp = TempDir::new().unwrap();
    let (ok, _, stderr) = run_init(&tmp, "badlang", &["--language", "haskell", "--no-git"]);
    assert!(!ok, "init should fail with invalid language");
    assert!(
        stderr.contains("Unknown language") || stderr.contains("haskell"),
        "Error should mention invalid language: {stderr}"
    );
}

#[test]
fn test_init_rejects_invalid_database() {
    let tmp = TempDir::new().unwrap();
    let (ok, _, stderr) = run_init(&tmp, "baddb", &["--database", "oracle", "--no-git"]);
    assert!(!ok, "init should fail with invalid database");
    assert!(
        stderr.contains("Unknown database") || stderr.contains("oracle"),
        "Error should mention invalid database: {stderr}"
    );
}

#[test]
fn test_init_rejects_invalid_size() {
    let tmp = TempDir::new().unwrap();
    let (ok, _, stderr) = run_init(&tmp, "badsz", &["--size", "xl", "--no-git"]);
    assert!(!ok, "init should fail with invalid size");
    assert!(
        stderr.contains("Unknown size") || stderr.contains("xl"),
        "Error should mention invalid size: {stderr}"
    );
}

// ============================================================================
// SQL content tests: verify generated DDL is database-appropriate
// ============================================================================

#[test]
fn test_init_postgres_sql_uses_serial_and_uuid() {
    let tmp = TempDir::new().unwrap();
    let (ok, _, stderr) = run_init(&tmp, "pgsql", &["--database", "postgres", "--no-git"]);
    assert!(ok, "init failed: {stderr}");

    let sql = fs::read_to_string(tmp.path().join("pgsql/db/0_schema/01_write/011_tb_author.sql"))
        .unwrap();

    assert!(sql.contains("SERIAL"), "Postgres SQL should use SERIAL");
    assert!(sql.contains("UUID"), "Postgres SQL should use UUID");
    assert!(sql.contains("gen_random_uuid()"), "Postgres SQL should use gen_random_uuid()");
    assert!(sql.contains("TIMESTAMPTZ"), "Postgres SQL should use TIMESTAMPTZ");
    assert!(sql.contains("CREATE INDEX"), "Indexes should live with tables (locality-first)");
}

#[test]
fn test_init_mysql_sql_uses_auto_increment() {
    let tmp = TempDir::new().unwrap();
    let (ok, _, stderr) =
        run_init(&tmp, "mysqltest", &["--database", "mysql", "--size", "xs", "--no-git"]);
    assert!(ok, "init failed: {stderr}");

    let sql = fs::read_to_string(tmp.path().join("mysqltest/db/0_schema/schema.sql")).unwrap();

    assert!(sql.contains("AUTO_INCREMENT"), "MySQL SQL should use AUTO_INCREMENT");
    assert!(sql.contains("CHAR(36)"), "MySQL SQL should use CHAR(36) for UUID");
}

#[test]
fn test_init_sqlite_sql_uses_autoincrement() {
    let tmp = TempDir::new().unwrap();
    let (ok, _, stderr) =
        run_init(&tmp, "sqlitetest", &["--database", "sqlite", "--size", "xs", "--no-git"]);
    assert!(ok, "init failed: {stderr}");

    let sql = fs::read_to_string(tmp.path().join("sqlitetest/db/0_schema/schema.sql")).unwrap();

    assert!(sql.contains("AUTOINCREMENT"), "SQLite SQL should use AUTOINCREMENT");
    assert!(sql.contains("datetime('now')"), "SQLite SQL should use datetime()");
}

#[test]
fn test_init_sqlserver_sql_uses_identity() {
    let tmp = TempDir::new().unwrap();
    let (ok, _, stderr) =
        run_init(&tmp, "mssqltest", &["--database", "sqlserver", "--size", "xs", "--no-git"]);
    assert!(ok, "init failed: {stderr}");

    let sql = fs::read_to_string(tmp.path().join("mssqltest/db/0_schema/schema.sql")).unwrap();

    assert!(sql.contains("IDENTITY"), "SQL Server SQL should use IDENTITY");
    assert!(sql.contains("UNIQUEIDENTIFIER"), "SQL Server SQL should use UNIQUEIDENTIFIER");
    assert!(sql.contains("NVARCHAR"), "SQL Server SQL should use NVARCHAR");
}

// ============================================================================
// Gitignore tests
// ============================================================================

#[test]
fn test_gitignore_excludes_compiled_output() {
    let tmp = TempDir::new().unwrap();
    let (ok, _, stderr) = run_init(&tmp, "gitest", &["--no-git"]);
    assert!(ok, "init failed: {stderr}");

    let gitignore = fs::read_to_string(tmp.path().join("gitest/.gitignore")).unwrap();

    assert!(
        gitignore.contains("schema.compiled.json"),
        ".gitignore should exclude schema.compiled.json"
    );
    assert!(gitignore.contains("target/"), ".gitignore should exclude target/");
    assert!(gitignore.contains("__pycache__/"), ".gitignore should exclude __pycache__/");
    assert!(gitignore.contains("node_modules/"), ".gitignore should exclude node_modules/");
    assert!(gitignore.contains(".env"), ".gitignore should exclude .env");
}
