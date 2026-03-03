#![allow(clippy::unwrap_used)]  // Reason: test/bench code, panics are acceptable
//! Integration tests for `fraiseql extract` command.
//!
//! Round-trip: run `fraiseql init` to generate a skeleton, then `fraiseql extract`
//! to parse it back to schema.json and verify types/queries match.

use std::{fs, str::FromStr};

use fraiseql_cli::commands::{
    extract,
    init::{self, InitConfig, Language, ProjectSize},
};

/// Run init for the given language, then extract from the skeleton file.
fn roundtrip(lang: Language) -> serde_json::Value {
    let tmp = tempfile::tempdir().unwrap();
    // Use absolute path as project name so init doesn't depend on cwd
    let project_dir = tmp.path().join("roundtrip_test");
    let project_name = project_dir.to_string_lossy().to_string();

    let config = InitConfig {
        project_name,
        language: lang,
        database: init::Database::from_str("postgres").unwrap(),
        size: ProjectSize::S,
        no_git: true,
    };
    init::run(&config).unwrap();

    // Find the schema file
    let ext = match lang {
        Language::Python => "py",
        Language::TypeScript => "ts",
        Language::Rust => "rs",
        Language::Java => "java",
        Language::Kotlin => "kt",
        Language::Go => "go",
        Language::CSharp => "cs",
        Language::Swift => "swift",
        Language::Scala => "scala",
        Language::Php => "php",
    };
    let schema_file = project_dir.join("schema").join(format!("schema.{ext}"));
    assert!(schema_file.exists(), "Schema file not found: {}", schema_file.display());

    let output_path = tmp.path().join("extracted.json");
    extract::run(
        &[schema_file.to_string_lossy().to_string()],
        None,
        false,
        output_path.to_str().unwrap(),
    )
    .unwrap();

    let json_str = fs::read_to_string(&output_path).unwrap();
    serde_json::from_str(&json_str).unwrap()
}

/// Verify the extracted schema has the expected types and queries.
fn verify_blog_schema(schema: &serde_json::Value, lang_name: &str) {
    let types = schema["types"].as_array().expect("types should be array");
    let queries = schema["queries"].as_array().expect("queries should be array");

    // Should have 4 types: Author, Post, Comment, Tag
    assert_eq!(types.len(), 4, "{lang_name}: expected 4 types, got {}", types.len());
    let type_names: Vec<&str> = types.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(type_names.contains(&"Author"), "{lang_name}: missing Author type");
    assert!(type_names.contains(&"Post"), "{lang_name}: missing Post type");
    assert!(type_names.contains(&"Comment"), "{lang_name}: missing Comment type");
    assert!(type_names.contains(&"Tag"), "{lang_name}: missing Tag type");

    // Should have 5 queries: posts, post, authors, author, tags
    assert_eq!(queries.len(), 5, "{lang_name}: expected 5 queries, got {}", queries.len());
    let query_names: Vec<&str> = queries.iter().map(|q| q["name"].as_str().unwrap()).collect();
    assert!(query_names.contains(&"posts"), "{lang_name}: missing 'posts' query");
    assert!(query_names.contains(&"post"), "{lang_name}: missing 'post' query");
    assert!(query_names.contains(&"authors"), "{lang_name}: missing 'authors' query");
    assert!(query_names.contains(&"author"), "{lang_name}: missing 'author' query");
    assert!(query_names.contains(&"tags"), "{lang_name}: missing 'tags' query");

    // Verify Author type has expected fields
    let author = types.iter().find(|t| t["name"] == "Author").unwrap();
    let author_fields = author["fields"].as_array().unwrap();
    assert!(
        author_fields.len() >= 4,
        "{lang_name}: Author should have at least 4 fields, got {}",
        author_fields.len()
    );

    // Verify nullable field exists (bio)
    assert!(
        author_fields.iter().any(|f| f["name"].as_str().unwrap() == "bio"),
        "{lang_name}: Author should have 'bio' field"
    );
    let bio = author_fields.iter().find(|f| f["name"] == "bio").unwrap();
    assert_eq!(bio["nullable"], true, "{lang_name}: Author.bio should be nullable");

    // Verify version
    assert_eq!(schema["version"], "2.0.0", "{lang_name}: version mismatch");

    // Verify posts query returns a list
    let posts = queries.iter().find(|q| q["name"] == "posts").unwrap();
    assert_eq!(posts["returns_list"], true, "{lang_name}: posts query should return a list");

    // Verify post query has arguments
    let post = queries.iter().find(|q| q["name"] == "post").unwrap();
    assert!(
        !post["returns_list"].as_bool().unwrap_or(false),
        "{lang_name}: post query should not return a list"
    );

    // Verify GraphQL type inference on Author fields
    let id_field = author_fields.iter().find(|f| f["name"] == "id").unwrap();
    assert_eq!(
        id_field["type"].as_str().unwrap(),
        "ID",
        "{lang_name}: Author.id should be GraphQL ID"
    );

    let name_field = author_fields.iter().find(|f| f["name"] == "name").unwrap();
    assert_eq!(
        name_field["type"].as_str().unwrap(),
        "String",
        "{lang_name}: Author.name should be GraphQL String"
    );

    // Verify _at fields are DateTime (Author has created_at in most languages)
    if let Some(created_at) = author_fields.iter().find(|f| f["name"] == "created_at") {
        assert_eq!(
            created_at["type"].as_str().unwrap(),
            "DateTime",
            "{lang_name}: Author.created_at should be GraphQL DateTime"
        );
    }

    // Verify post query arg type is ID
    let post_args = post["arguments"].as_array();
    if let Some(args) = post_args {
        if !args.is_empty() {
            assert_eq!(
                args[0]["type"].as_str().unwrap(),
                "ID",
                "{lang_name}: post query id arg should be GraphQL ID"
            );
        }
    }

    // Verify post query has sql_source set
    let post_sql = post["sql_source"].as_str();
    assert!(post_sql.is_some(), "{lang_name}: post query should have sql_source set");
}

// =============================================================================
// Per-language round-trip tests
// =============================================================================

#[test]
fn test_extract_python_roundtrip() {
    let schema = roundtrip(Language::Python);
    verify_blog_schema(&schema, "Python");
}

#[test]
fn test_extract_typescript_roundtrip() {
    let schema = roundtrip(Language::TypeScript);
    verify_blog_schema(&schema, "TypeScript");
}

#[test]
fn test_extract_rust_roundtrip() {
    let schema = roundtrip(Language::Rust);
    verify_blog_schema(&schema, "Rust");
}

#[test]
fn test_extract_java_roundtrip() {
    let schema = roundtrip(Language::Java);
    verify_blog_schema(&schema, "Java");
}

#[test]
fn test_extract_kotlin_roundtrip() {
    let schema = roundtrip(Language::Kotlin);
    verify_blog_schema(&schema, "Kotlin");
}

#[test]
fn test_extract_go_roundtrip() {
    let schema = roundtrip(Language::Go);
    verify_blog_schema(&schema, "Go");
}

#[test]
fn test_extract_csharp_roundtrip() {
    let schema = roundtrip(Language::CSharp);
    verify_blog_schema(&schema, "CSharp");
}

#[test]
fn test_extract_swift_roundtrip() {
    let schema = roundtrip(Language::Swift);
    verify_blog_schema(&schema, "Swift");
}

#[test]
fn test_extract_scala_roundtrip() {
    let schema = roundtrip(Language::Scala);
    verify_blog_schema(&schema, "Scala");
}

/// Run all 9 languages in a loop to verify consistency.
#[test]
fn test_extract_all_languages_roundtrip() {
    let languages = [
        Language::Python,
        Language::TypeScript,
        Language::Rust,
        Language::Java,
        Language::Kotlin,
        Language::Go,
        Language::CSharp,
        Language::Swift,
        Language::Scala,
    ];

    for lang in &languages {
        let schema = roundtrip(*lang);
        verify_blog_schema(&schema, &format!("{lang}"));
    }
}

/// Test that extract with directory input works.
#[test]
fn test_extract_from_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("dir_test");

    let config = InitConfig {
        project_name: project_dir.to_string_lossy().to_string(),
        language:     Language::Python,
        database:     init::Database::from_str("postgres").unwrap(),
        size:         ProjectSize::S,
        no_git:       true,
    };
    init::run(&config).unwrap();

    let schema_dir = project_dir.join("schema");
    let output_path = tmp.path().join("extracted.json");

    extract::run(
        &[schema_dir.to_string_lossy().to_string()],
        None,
        false,
        output_path.to_str().unwrap(),
    )
    .unwrap();

    let json_str = fs::read_to_string(&output_path).unwrap();
    let schema: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    verify_blog_schema(&schema, "directory");
}

/// Test that extract with --language override works.
#[test]
fn test_extract_with_language_override() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("lang_override_test");

    let config = InitConfig {
        project_name: project_dir.to_string_lossy().to_string(),
        language:     Language::Python,
        database:     init::Database::from_str("postgres").unwrap(),
        size:         ProjectSize::S,
        no_git:       true,
    };
    init::run(&config).unwrap();

    let schema_file = project_dir.join("schema").join("schema.py");
    let output_path = tmp.path().join("extracted.json");

    // Explicitly pass language=python
    extract::run(
        &[schema_file.to_string_lossy().to_string()],
        Some("python"),
        false,
        output_path.to_str().unwrap(),
    )
    .unwrap();

    let json_str = fs::read_to_string(&output_path).unwrap();
    let schema: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    verify_blog_schema(&schema, "language_override");
}

/// Test that extract on an empty file produces empty schema.
#[test]
fn test_extract_empty_file() {
    let tmp = tempfile::tempdir().unwrap();
    let source_file = tmp.path().join("empty.py");
    fs::write(&source_file, "# No FraiseQL annotations here\n").unwrap();

    let output_path = tmp.path().join("extracted.json");
    extract::run(
        &[source_file.to_string_lossy().to_string()],
        None,
        false,
        output_path.to_str().unwrap(),
    )
    .unwrap();

    let json_str = fs::read_to_string(&output_path).unwrap();
    let schema: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    assert_eq!(schema["types"].as_array().unwrap().len(), 0);
    assert_eq!(schema["queries"].as_array().unwrap().len(), 0);
    assert_eq!(schema["version"], "2.0.0");
}

/// Test that extract with nonexistent path fails gracefully.
#[test]
fn test_extract_nonexistent_path() {
    let result =
        extract::run(&["/nonexistent/path/schema.py".to_string()], None, false, "/tmp/out.json");
    assert!(result.is_err());
}
