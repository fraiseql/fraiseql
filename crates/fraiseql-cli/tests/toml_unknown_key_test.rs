//! Every table `fraiseql.toml` can contain refuses a key it does not declare (#1837).
//!
//! serde does not carry `deny_unknown_fields` into nested structs, so each section's own
//! type needs it. Without it a misspelt key is discarded and the setting it meant stays at
//! its default: `[grpc] enabeld = true` compiled with gRPC off.
//!
//! The tables are discovered by serializing each root's `Default`, so a section added
//! later is checked without editing this file. `Default` cannot reach an `Option` section,
//! an array-of-tables entry or a map entry, so those are listed; the table counts are
//! asserted so the walk cannot quietly measure less.
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use fraiseql_cli::config::{TomlProjectConfig, toml_schema::TomlSchema};
use serde::{Serialize, de::DeserializeOwned};

const UNKNOWN: &str = "zz_not_a_key";

/// Tables whose keys are names the user chooses, not fields: any key is valid.
const NAME_KEYED: &[&str] = &["fraiseql.cost_weights", "types", "queries", "mutations"];

/// `fraiseql compile`'s root: sections `Default` leaves out, written as TOML headers.
const SCHEMA_UNREACHED: &[&str] = &[
    "[auth]",
    "[auth.me]",
    "[changelog]",
    "[crud]",
    "[locale]",
    "[hierarchies.h]",
    "[types.T]",
    "[queries.Q]",
    "[mutations.M]",
    "[security.error_sanitization]",
    "[security.rate_limiting]",
    "[security.state_encryption]",
    "[security.pkce]",
    "[security.api_keys]",
    "[security.token_revocation]",
    "[security.trusted_documents]",
    "[subscriptions.hooks]",
    "[federation.circuit_breaker]",
    "[[security.rules]]",
    "[[security.policies]]",
    "[[security.field_auth]]",
    "[[caching.rules]]",
    "[[analytics.queries]]",
    "[[federation.entities]]",
    "[[federation.circuit_breaker.per_database]]",
    "[[observers.handlers]]",
];

/// The project file's root: sections `Default` leaves out.
const PROJECT_UNREACHED: &[&str] = &["[locale]", "[observers]", "[[observers.handlers]]"];

/// Tables `Default` reaches on each root, the root itself included.
const SCHEMA_TABLES: usize = 29;
const PROJECT_TABLES: usize = 20;

fn table_paths(prefix: &str, table: &toml::Table, out: &mut Vec<String>) {
    for (key, value) in table {
        if let toml::Value::Table(sub) = value {
            let path = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{prefix}.{key}")
            };
            out.push(path.clone());
            table_paths(&path, sub, out);
        }
    }
}

/// `None` when `doc` is refused for naming `key`; otherwise what happened instead.
fn not_refused<T: DeserializeOwned>(doc: &str, key: &str) -> Option<String> {
    match toml::from_str::<T>(doc) {
        // A TOML error quotes its source line, so only serde's own wording counts.
        Err(e) if e.to_string().contains(&format!("unknown field `{key}`")) => None,
        Err(e) => Some(format!("refused for another reason: {}", e.message())),
        Ok(_) => Some("accepted".to_string()),
    }
}

/// Every table of `T` that accepts [`UNKNOWN`], and how many tables `Default` reached.
fn sweep<T: DeserializeOwned + Serialize + Default>(unreached: &[&str]) -> (usize, Vec<String>) {
    let base = toml::Table::try_from(T::default()).unwrap();
    let mut paths = vec![String::new()];
    table_paths("", &base, &mut paths);

    let mut failures = Vec::new();
    for path in paths.iter().filter(|path| !NAME_KEYED.contains(&path.as_str())) {
        let mut doc = base.clone();
        let mut table = &mut doc;
        for part in path.split('.').filter(|part| !part.is_empty()) {
            table = table.get_mut(part).unwrap().as_table_mut().unwrap();
        }
        table.insert(UNKNOWN.into(), toml::Value::Integer(1));
        if let Some(what) = not_refused::<T>(&toml::to_string(&doc).unwrap(), UNKNOWN) {
            failures.push(format!("[{path}]: {what}"));
        }
    }
    for header in unreached {
        if let Some(what) = not_refused::<T>(&format!("{header}\n{UNKNOWN} = 1\n"), UNKNOWN) {
            failures.push(format!("{header}: {what}"));
        }
    }
    (paths.len(), failures)
}

#[test]
fn every_compile_toml_table_refuses_a_key_it_does_not_declare() {
    let (tables, failures) = sweep::<TomlSchema>(SCHEMA_UNREACHED);
    assert!(failures.is_empty(), "tables that accept a misspelt key: {failures:#?}");
    assert_eq!(
        tables, SCHEMA_TABLES,
        "fewer tables means the walk measures less; more, re-count"
    );
}

#[test]
fn every_project_toml_table_refuses_a_key_it_does_not_declare() {
    let (tables, failures) = sweep::<TomlProjectConfig>(PROJECT_UNREACHED);
    assert!(failures.is_empty(), "tables that accept a misspelt key: {failures:#?}");
    assert_eq!(
        tables, PROJECT_TABLES,
        "fewer tables means the walk measures less; more, re-count"
    );
}

/// The misspellings #1837 was filed for, through the loaders `fraiseql compile` and
/// `fraiseql run` use, each refused naming the key that was written.
#[test]
fn each_misspelling_is_refused_by_name_where_it_was_written() {
    for (doc, key) in [
        ("[grpc]\nenabeld = true\n", "enabeld"),
        ("[session_variables]\ninject_started = false\n", "inject_started"),
        ("[changelog]\nexpose = true\nread_rol = \"auditor\"\n", "read_rol"),
        ("[crud]\nfunction_shema = \"app\"\n", "function_shema"),
        (
            "[hierarchies.h]\ntable = \"t\"\npath_column = \"p\"\npath_colum = \"q\"\n",
            "path_colum",
        ),
    ] {
        let err = TomlSchema::parse_toml(doc).expect_err(doc);
        assert!(format!("{err:#}").contains(key), "{doc}: {err:#}");
    }

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fraiseql.toml");
    std::fs::write(&path, "[fraiseql.session_variables]\ninject_started = false\n").unwrap();
    let err = TomlProjectConfig::from_file(path.to_str().unwrap()).unwrap_err();
    assert!(format!("{err:#}").contains("inject_started"), "{err:#}");
}
