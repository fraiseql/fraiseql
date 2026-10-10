#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
//! #1530: a field whose type resolves to nothing fails the compile.
//!
//! It used to be a warning (#724), on the ground that a `--schema-dir` author could declare
//! the type in a file the validator could not see. The directory is merged before the
//! validator runs (`compile.rs`, `load_intermediate_schema` then `SchemaValidator`), so it
//! sees every file: the case #724 protected compiles here, and a name declared nowhere is a
//! typo or a missing declaration. Compiled anyway, it became an object reference the server
//! answered with no value.
//!
//! **Execution engine:** in-memory (no database required): the refusal is the compiler's,
//! before any database is involved. **Infrastructure:** none.

use fraiseql_cli::commands::compile::{CompileOptions, compile_to_schema};
use fraiseql_core::schema::FieldType;
use serde_json::{Value, json};
use tempfile::TempDir;

/// `Host` with one field of type `ty`, as an SDK writes it.
fn host_schema(ty: &str) -> Value {
    json!({
        "types": [{
            "name": "Host",
            "fields": [
                { "name": "id", "type": "Int", "nullable": false },
                { "name": "born_on", "type": ty, "nullable": false }
            ],
            "sql_source": "v_host"
        }],
        "queries": [{
            "name": "hosts", "return_type": "Host", "returns_list": true,
            "sql_source": "v_host", "nullable": false, "arguments": []
        }],
        "version": "2.0.0"
    })
}

async fn compile_json(schema: &Value) -> Result<fraiseql_core::schema::CompiledSchema, String> {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("schema.json");
    std::fs::write(&path, schema.to_string()).unwrap();
    compile_to_schema(CompileOptions {
        skip_hash: true,
        ..CompileOptions::new(path.to_str().unwrap())
    })
    .await
    .map(|(compiled, _)| compiled.schema)
    .map_err(|e| format!("{e:#}"))
}

#[tokio::test]
async fn an_undeclared_field_type_fails_the_compile_naming_the_field_and_the_type() {
    for ty in ["date", "Strng"] {
        let err = match compile_json(&host_schema(ty)).await {
            Ok(schema) => panic!(
                "`{ty}` compiled, as {:?}",
                schema.find_type("Host").unwrap().fields[1].field_type
            ),
            Err(e) => e,
        };
        assert!(
            err.contains("Host.born_on") && err.contains(&format!("'{ty}'")),
            "names the field and the type: {err}"
        );
    }
}

/// The built-in spellings still compile as their scalars: the refusal is of an unknown name,
/// not of a lowercase one in general.
#[tokio::test]
async fn a_known_scalar_name_compiles() {
    for (ty, expected) in [
        ("Date", FieldType::Date),
        ("Hostname", FieldType::Scalar("Hostname".to_string())),
    ] {
        let schema = compile_json(&host_schema(ty)).await.unwrap();
        assert_eq!(schema.find_type("Host").unwrap().fields[1].field_type, expected, "{ty}");
    }
}

/// #724's case: the type, and a custom scalar, declared in a sibling file of the schema
/// directory. Both resolve.
#[tokio::test]
async fn a_type_declared_in_a_sibling_file_of_the_schema_dir_compiles() {
    let dir = TempDir::new().unwrap();
    let schema_dir = dir.path().join("schema");
    std::fs::create_dir(&schema_dir).unwrap();
    std::fs::write(
        schema_dir.join("host.json"),
        json!({ "types": [{
            "name": "Host",
            "fields": [
                { "name": "id", "type": "Int", "nullable": false },
                { "name": "owner", "type": "Owner", "nullable": false },
                { "name": "shade", "type": "Shade", "nullable": false }
            ],
            "sql_source": "v_host"
        }],
        "queries": [{
            "name": "hosts", "return_type": "Host", "returns_list": true,
            "sql_source": "v_host", "nullable": false, "arguments": []
        }] })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        schema_dir.join("owner.json"),
        json!({
            "types": [{
                "name": "Owner",
                "fields": [{ "name": "id", "type": "Int", "nullable": false }],
                "sql_source": "v_owner"
            }],
            "custom_scalars": [{ "name": "Shade" }]
        })
        .to_string(),
    )
    .unwrap();
    let toml = dir.path().join("fraiseql.toml");
    std::fs::write(
        &toml,
        "[schema]\nname = \"p1530\"\nversion = \"1.0.0\"\ndatabase_target = \"postgresql\"\n",
    )
    .unwrap();

    let (compiled, _) = compile_to_schema(CompileOptions {
        skip_hash: true,
        schema_dir: Some(schema_dir.to_str().unwrap()),
        ..CompileOptions::new(toml.to_str().unwrap())
    })
    .await
    .unwrap_or_else(|e| panic!("#724's case must compile: {e:#}"));
    let host = compiled.schema.find_type("Host").unwrap();
    assert_eq!(host.fields[1].field_type, FieldType::Object("Owner".to_string()));
    assert_eq!(host.fields[2].field_type, FieldType::Scalar("Shade".to_string()));
}
