use super::*;

#[test]
fn test_language_from_str() {
    assert_eq!(Language::from_str("python").expect("test"), Language::Python);
    assert_eq!(Language::from_str("py").expect("test"), Language::Python);
    assert_eq!(Language::from_str("typescript").expect("test"), Language::TypeScript);
    assert_eq!(Language::from_str("ts").expect("test"), Language::TypeScript);
    assert_eq!(Language::from_str("rust").expect("test"), Language::Rust);
    assert_eq!(Language::from_str("rs").expect("test"), Language::Rust);
    assert_eq!(Language::from_str("java").expect("test"), Language::Java);
    assert_eq!(Language::from_str("jav").expect("test"), Language::Java);
    assert_eq!(Language::from_str("kotlin").expect("test"), Language::Kotlin);
    assert_eq!(Language::from_str("kt").expect("test"), Language::Kotlin);
    assert_eq!(Language::from_str("go").expect("test"), Language::Go);
    assert_eq!(Language::from_str("golang").expect("test"), Language::Go);
    assert_eq!(Language::from_str("csharp").expect("test"), Language::CSharp);
    assert_eq!(Language::from_str("c#").expect("test"), Language::CSharp);
    assert_eq!(Language::from_str("cs").expect("test"), Language::CSharp);
    assert_eq!(Language::from_str("swift").expect("test"), Language::Swift);
    assert_eq!(Language::from_str("scala").expect("test"), Language::Scala);
    assert_eq!(Language::from_str("sc").expect("test"), Language::Scala);
    assert!(
        Language::from_str("haskell").is_err(),
        "expected Err for unsupported language 'haskell'"
    );
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
    assert_eq!(Database::from_str("postgres").expect("test"), Database::Postgres);
    assert_eq!(Database::from_str("postgresql").expect("test"), Database::Postgres);
    assert_eq!(Database::from_str("pg").expect("test"), Database::Postgres);
    assert_eq!(Database::from_str("mysql").expect("test"), Database::Mysql);
    assert_eq!(Database::from_str("sqlite").expect("test"), Database::Sqlite);
    assert_eq!(Database::from_str("sqlserver").expect("test"), Database::SqlServer);
    assert_eq!(Database::from_str("mssql").expect("test"), Database::SqlServer);
    assert!(
        Database::from_str("oracle").is_err(),
        "expected Err for unsupported database 'oracle'"
    );
}

#[test]
fn test_size_from_str() {
    assert_eq!(ProjectSize::from_str("xs").expect("test"), ProjectSize::Xs);
    assert_eq!(ProjectSize::from_str("s").expect("test"), ProjectSize::S);
    assert_eq!(ProjectSize::from_str("m").expect("test"), ProjectSize::M);
    assert!(ProjectSize::from_str("l").is_err(), "expected Err for unsupported size 'l'");
}

#[test]
fn test_database_default_url() {
    assert_eq!(Database::Postgres.default_url("myapp"), "postgresql://localhost/myapp");
    assert_eq!(Database::Sqlite.default_url("myapp"), "myapp.db");
}

#[test]
fn test_init_creates_project() {
    let tmp = tempfile::tempdir().expect("test");
    let project_dir = tmp.path().join("test_project");

    let config = InitConfig {
        project_name: project_dir.to_string_lossy().to_string(),
        language: Language::Python,
        database: Database::Postgres,
        size: ProjectSize::S,
        no_git: true,
    };

    run(&config).expect("test");

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
    let tmp = tempfile::tempdir().expect("test");
    let project_dir = tmp.path().join("test_xs");

    let config = InitConfig {
        project_name: project_dir.to_string_lossy().to_string(),
        language: Language::TypeScript,
        database: Database::Postgres,
        size: ProjectSize::Xs,
        no_git: true,
    };

    run(&config).expect("test");

    assert!(project_dir.join("db/0_schema/schema.sql").exists());
    assert!(project_dir.join("schema/schema.ts").exists());

    // Should NOT have the numbered directories
    assert!(!project_dir.join("db/0_schema/01_write").exists());
}

#[test]
fn test_init_m_layout() {
    let tmp = tempfile::tempdir().expect("test");
    let project_dir = tmp.path().join("test_m");

    let config = InitConfig {
        project_name: project_dir.to_string_lossy().to_string(),
        language: Language::Rust,
        database: Database::Postgres,
        size: ProjectSize::M,
        no_git: true,
    };

    run(&config).expect("test");

    assert!(project_dir.join("db/0_schema/01_write/author/tb_author.sql").exists());
    assert!(project_dir.join("db/0_schema/01_write/post/tb_post.sql").exists());
    assert!(project_dir.join("db/0_schema/02_read/author/v_author.sql").exists());
    assert!(project_dir.join("db/0_schema/03_functions/author/fn_author_crud.sql").exists());
    assert!(project_dir.join("schema/schema.rs").exists());
}

#[test]
fn test_init_refuses_existing_dir() {
    let tmp = tempfile::tempdir().expect("test");
    let project_dir = tmp.path().join("existing");

    fs::create_dir(&project_dir).expect("test");

    let config = InitConfig {
        project_name: project_dir.to_string_lossy().to_string(),
        language: Language::Python,
        database: Database::Postgres,
        size: ProjectSize::S,
        no_git: true,
    };

    let result = run(&config);
    assert!(result.is_err(), "expected Err when directory already exists");
    assert!(result.expect_err("test").to_string().contains("already exists"));
}

#[test]
fn test_toml_config_is_valid() {
    let tmp = tempfile::tempdir().expect("test");
    let project_dir = tmp.path().join("toml_test");

    let config = InitConfig {
        project_name: project_dir.to_string_lossy().to_string(),
        language: Language::Python,
        database: Database::Postgres,
        size: ProjectSize::S,
        no_git: true,
    };

    run(&config).expect("test");

    // Verify the TOML can be parsed
    let toml_content = fs::read_to_string(project_dir.join("fraiseql.toml")).expect("test");
    let parsed: toml::Value = toml::from_str(&toml_content).expect("test");
    // project name in TOML is the full path since we pass absolute paths
    assert!(parsed["project"]["name"].as_str().is_some());
}

#[test]
fn test_schema_json_is_valid() {
    let tmp = tempfile::tempdir().expect("test");
    let project_dir = tmp.path().join("json_test");

    let config = InitConfig {
        project_name: project_dir.to_string_lossy().to_string(),
        language: Language::Python,
        database: Database::Postgres,
        size: ProjectSize::Xs,
        no_git: true,
    };

    run(&config).expect("test");

    let json_content = fs::read_to_string(project_dir.join("schema.json")).expect("test");
    let parsed: serde_json::Value = serde_json::from_str(&json_content).expect("test");

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
