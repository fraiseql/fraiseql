#![no_main]
//! A `schema.compiled.json` through the loader the server boots with,
//! `CompiledSchemaLoader::load_extended`: the JSON parse, `CompiledSchema::from_json` and
//! its load-time checks, the `functions` section and its validation, the query-to-function
//! bindings, and the refusals of sections this build cannot serve.
//!
//! Beyond "no panic", what the server accepts is what the compiler's own check accepts:
//! the accepted schema and `functions` section, written back as `fraiseql compile` writes
//! them (content hash first), load with the hash enforced and through the boot loader
//! again, and serialize to the same text both times.

use std::{path::PathBuf, sync::OnceLock};

use fraiseql_core::schema::{CompiledSchema, content_hash_of};
use fraiseql_server::schema::loader::{CompiledSchemaLoader, ExtendedCompiledSchema};
use libfuzzer_sys::fuzz_target;
use serde_json::{Map, Value};

/// The directory inputs are written to, one per process. The working directory is left
/// alone: libFuzzer resolves the corpus and artifact paths it is given against it.
fn workdir() -> &'static PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("fraiseql-server-fuzz-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create the input directory");
        dir
    })
}

fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("a current-thread runtime")
    })
}

/// Write `text` to `name` and load it as the server does at boot.
fn boot_load(name: &str, text: &str) -> Option<ExtendedCompiledSchema> {
    let path = workdir().join(name);
    std::fs::write(&path, text).expect("write the input");
    runtime().block_on(CompiledSchemaLoader::new(&path).load_extended()).ok()
}

/// The file `fraiseql compile` would write for these sections: the hash first, then the
/// schema's fields, then `functions`.
fn as_compiled(loaded: &ExtendedCompiledSchema) -> (Value, String) {
    let mut body = serde_json::to_value(&loaded.schema).expect("an accepted schema serializes");
    let object = body.as_object_mut().expect("a schema serializes as an object");
    if let Some(functions) = &loaded.functions {
        let functions = serde_json::to_value(functions).expect("accepted functions serialize");
        object.insert("functions".to_string(), functions);
    }
    let mut file = Map::new();
    file.insert("_content_hash".to_string(), Value::String(content_hash_of(&body)));
    file.extend(body.as_object().expect("an object").clone());
    let text = serde_json::to_string_pretty(&Value::Object(file)).expect("serialize");
    (body, text)
}

fuzz_target!(|data: &str| {
    let Some(first) = boot_load("input.json", data) else {
        return;
    };
    let (body, written) = as_compiled(&first);

    if let Err(e) = CompiledSchema::from_json(&written, true) {
        panic!(
            "the server accepted a schema that, written as compiled, fails the hash-enforced load: {e}\n{written}"
        );
    }
    let Some(second) = boot_load("written.json", &written) else {
        panic!("the server accepted a schema whose compiled form it then refuses:\n{written}");
    };
    let (again, _) = as_compiled(&second);
    assert_eq!(
        body, again,
        "an accepted schema changed when written as compiled and loaded again"
    );
});
