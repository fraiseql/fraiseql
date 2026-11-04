#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
//! #1799: `fraiseql compile` refuses a tenant-scoped fact table whose aggregate and window
//! roots nothing scopes.
//!
//! An aggregate or a window root has no `inject_params`, and the server installs no row
//! policy, so in a schema whose reads inject the caller's tenant those roots would read
//! every tenant's rows. The server refuses them; the compiler refuses the schema first,
//! naming the fact table, why it is scoped, and the declarations that scope it: the fact
//! table's own `inject_params` (#1800), or a relation that filters its own rows.
//!
//! **Execution engine:** in-memory (no database required)
//! **Infrastructure:** none
//! **Parallelism:** one test — `compile_to_schema` resolves its input against the process
//! working directory, so cases in separate `#[tokio::test]`s would race on `chdir`.

use fraiseql_cli::commands::compile::{CompileOptions, compile_to_schema};
use fraiseql_core::{
    compiler::fact_table::FactRowScope,
    schema::{CompiledSchema, InjectedParamSource},
};
use serde_json::{Value, json};
use tempfile::TempDir;

/// A list root injecting `tenant_id` from the token, and `tf_sales` with `fact_table_extra`
/// merged in.
fn schema(inject: bool, fact_table_extra: &Value) -> String {
    let mut query = json!({
        "name": "rows",
        "return_type": "Row",
        "returns_list": true,
        "sql_source": "v_row"
    });
    if inject {
        query["inject_params"] = json!({"tenant_id": {"source": "jwt", "claim": "tid"}});
    }
    let mut fact_table = json!({
        "table_name": "tf_sales",
        "measures": [{"name": "revenue", "sql_type": "numeric", "nullable": false}],
        "dimensions": {"name": "data", "paths": []},
        "denormalized_filters": [{"name": "tenant_id", "sql_type": "text", "indexed": true}]
    });
    for (key, value) in fact_table_extra.as_object().unwrap() {
        fact_table[key] = value.clone();
    }
    json!({
        "types": [{
            "name": "Row",
            "sql_source": "v_row",
            "fields": [
                {"name": "id", "type": "Int", "nullable": false},
                {"name": "tenant_id", "type": "String", "nullable": false}
            ]
        }],
        "queries": [query],
        "fact_tables": [fact_table]
    })
    .to_string()
}

/// Compile `schema_json` from a fresh temp dir, restoring the working directory.
async fn compile(schema_json: &str) -> anyhow::Result<CompiledSchema> {
    let dir = TempDir::new().expect("temp dir");
    std::fs::write(dir.path().join("schema.json"), schema_json).expect("write schema.json");
    let original = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(dir.path()).expect("chdir into temp dir");
    let result = compile_to_schema(CompileOptions::new("schema.json")).await;
    std::env::set_current_dir(original).expect("restore cwd");
    result.map(|(artifact, _)| artifact.schema)
}

#[tokio::test]
async fn compile_refuses_a_tenant_scoped_fact_table_nothing_scopes() {
    // Its `tenant_id` column is the one the list root injects.
    let err = compile(&schema(true, &json!({})))
        .await
        .expect_err("an unscoped tenant-scoped fact table must not compile");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("fact table 'tf_sales' (its column 'tenant_id' is injected by query 'rows')"),
        "names the fact table and the matched inject key: {msg}"
    );
    assert!(msg.contains("row_scope = \"relation\""), "names the way out: {msg}");
    assert!(msg.contains("inject_params"), "names the other way out (#1800): {msg}");

    // A relation that filters its own rows says so, and the declaration reaches the artifact.
    let compiled = compile(&schema(true, &json!({"row_scope": "relation"})))
        .await
        .expect("a relation-scoped fact table compiles");
    assert_eq!(compiled.get_fact_table("tf_sales").unwrap().row_scope, FactRowScope::Relation);

    // A schema that injects nothing does not scope the fact table: either declaration compiles.
    for extra in [json!({}), json!({"row_scope": "relation"})] {
        let result = compile(&schema(false, &extra)).await;
        assert!(result.is_ok(), "{extra}: an unscoped schema compiles: {:?}", result.err());
    }

    // #1800: a fact table declaring its own `inject_params` is scoped by them, in either
    // accepted shape, and the declaration reaches the artifact.
    for inject in [
        json!({"tenant_id": {"source": "jwt", "claim": "tid"}}),
        json!({"tenant_id": "jwt:tid"}),
    ] {
        let compiled = compile(&schema(true, &json!({"inject_params": inject})))
            .await
            .map_err(|e| format!("{inject}: a fact table its inject params scope compiles: {e:#}"))
            .unwrap();
        assert_eq!(
            compiled.get_fact_table("tf_sales").unwrap().inject_params.get("tenant_id"),
            Some(&InjectedParamSource::Jwt("tid".to_string())),
            "{inject}"
        );
    }

    // An absent claim cannot filter rows, refused as the converter refuses it on a query;
    // and a filter needs a column of the table.
    for (inject, expected) in [
        (
            json!({"tenant_id": "jwt_optional:tid"}),
            "'jwt_optional' is for mutation arguments",
        ),
        (
            json!({"owner_id": "jwt:tid"}),
            "'owner_id' is not one of its denormalized filter columns",
        ),
    ] {
        let err = compile(&schema(true, &json!({"inject_params": inject})))
            .await
            .expect_err("an inject param the runtime cannot honour must not compile");
        assert!(format!("{err:#}").contains(expected), "{inject}: {err:#}");
    }

    // Only the one mode is accepted.
    let err = compile(&schema(true, &json!({"row_scope": "client"})))
        .await
        .expect_err("an unknown row scope must not compile");
    assert!(format!("{err:#}").contains("client"), "names the value: {err:#}");
}
