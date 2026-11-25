#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code — fail-loud.
//! Two functions share one helper, authored, compiled and run by `functions invoke`.
//!
//! The fixture `tests/fixtures/functions_shared/functions/` holds `greet.ts` and
//! `shout.ts`, both importing `@shared/text.ts`, and the `deno.json` an editor reads. The
//! server's half of this acceptance, over the same fixture, is
//! `fraiseql-server`'s `functions_setup_shared_tests`: both assert the hash computed
//! here, from the files, so the harness and the server report the same one.
//!
//! **Execution engine:** V8 (in the spawned CLI child)
//! **Infrastructure:** none
//! **Parallelism:** safe (each test forks its own binary + temp dir)

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
};

use fraiseql_functions::modules::{hash::function_hash, resolve::resolve_specifier};
use tempfile::TempDir;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/functions_shared/functions")
}

/// The fixture copied to `dir/functions`, so a test can edit it.
fn copy_fixture(dir: &Path) -> PathBuf {
    let module_dir = dir.join("functions");
    for relative in [
        "greet.ts",
        "shout.ts",
        "deno.json",
        "shared/text.ts",
        "shared/types.ts",
    ] {
        let target = module_dir.join(relative);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::copy(fixture().join(relative), target).unwrap();
    }
    module_dir
}

/// The hash of `name` as its files now stand: the function and `text.ts`. `types.ts`
/// is imported only as a type, so it is not part of the function that runs.
fn expected_hash(module_dir: &Path, name: &str) -> String {
    let entry = std::fs::read(module_dir.join(format!("{name}.ts"))).unwrap();
    let text = std::fs::read(module_dir.join("shared/text.ts")).unwrap();
    function_hash(&entry, &BTreeMap::from([(resolve_specifier("@shared/text.ts").unwrap(), text)]))
}

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
}

fn compile(dir: &Path, module_dir: &Path) -> PathBuf {
    let types = serde_json::json!({
        "version": "2.0.0",
        "types": [{
            "name": "Order", "sql_source": "v_order", "is_input": false,
            "fields": [{"name": "id", "type": "ID", "nullable": false}]
        }],
        "mutations": [{
            "name": "updateOrder", "return_type": "Order", "sql_source": "fn_update_order",
            "operation": "update", "invalidates_views": ["v_order"], "arguments": []
        }],
        "functions": [
            { "name": "greet", "trigger": "after:mutation:Order:update", "runtime": "Deno" },
            { "name": "shout", "trigger": "after:mutation:Order:update", "runtime": "Deno" }
        ]
    });
    let types_path = dir.join("types.json");
    std::fs::write(&types_path, types.to_string()).unwrap();
    let toml_path = dir.join("fraiseql.toml");
    std::fs::write(
        &toml_path,
        format!(
            "[project]\nname = \"shared\"\nversion = \"1.0.0\"\ndatabase_target = \"postgresql\"\n\n\
             [functions]\nmodule_dir = {:?}\nshared_dir = {:?}\n",
            module_dir.display().to_string(),
            module_dir.join("shared").display().to_string(),
        ),
    )
    .unwrap();
    let schema = dir.join("schema.compiled.json");
    let out = cli()
        .arg("compile")
        .arg(&toml_path)
        .arg("--types")
        .arg(&types_path)
        .arg("--output")
        .arg(&schema)
        .output()
        .unwrap();
    assert!(out.status.success(), "compile:\n{}", String::from_utf8_lossy(&out.stderr));
    schema
}

/// `invoke <name> --json`: the function's result and hash.
fn invoke(dir: &Path, schema: &Path, name: &str) -> (serde_json::Value, String) {
    let payload = dir.join("payload.json");
    std::fs::write(&payload, r#"{"event_kind":"update","old":{"id":"1"},"new":{"id":"1"}}"#)
        .unwrap();
    let out = cli()
        .args(["functions", "invoke", name, "--json", "--payload"])
        .arg(&payload)
        .arg("--schema")
        .arg(schema)
        .env_remove("FRAISEQL_FUNCTIONS_MODULE_DIR")
        .env_remove("FRAISEQL_FUNCTIONS_SHARED_DIR")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{name}:\n{stdout}");
    let report: serde_json::Value = serde_json::from_str(stdout.lines().last().unwrap()).unwrap();
    (report["result"].clone(), report["hash"].as_str().unwrap().to_owned())
}

#[test]
fn two_functions_share_a_helper_and_an_edit_to_it_changes_both() {
    let dir = TempDir::new().unwrap();
    let module_dir = copy_fixture(dir.path());
    let schema = compile(dir.path(), &module_dir);

    let (greet, greet_hash) = invoke(dir.path(), &schema, "greet");
    let (shout, shout_hash) = invoke(dir.path(), &schema, "shout");
    assert_eq!(greet, "hello, fraiseql");
    assert_eq!(shout, "HELLO, FRAISEQL!");
    assert_eq!(greet_hash, expected_hash(&module_dir, "greet"));
    assert_eq!(shout_hash, expected_hash(&module_dir, "shout"));

    // Edit the helper alone: both functions run the new code, and both hashes move.
    let text = module_dir.join("shared/text.ts");
    let edited = std::fs::read_to_string(&text).unwrap().replace("hello, ", "hi, ");
    std::fs::write(&text, edited).unwrap();

    let (greet_after, greet_hash_after) = invoke(dir.path(), &schema, "greet");
    let (shout_after, shout_hash_after) = invoke(dir.path(), &schema, "shout");
    assert_eq!(greet_after, "hi, fraiseql");
    assert_eq!(shout_after, "HI, FRAISEQL!");
    assert_ne!(greet_hash_after, greet_hash);
    assert_ne!(shout_hash_after, shout_hash);
    assert_eq!(greet_hash_after, expected_hash(&module_dir, "greet"));
    assert_eq!(shout_hash_after, expected_hash(&module_dir, "shout"));
}
