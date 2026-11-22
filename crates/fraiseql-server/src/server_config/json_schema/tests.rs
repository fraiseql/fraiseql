#![allow(clippy::unwrap_used)] // Reason: test code

use fraiseql_core::utils::config_diagnostic::schema::{is_refused, unknown_key};
use serde_json::Value;

use super::{FEATURE_ANNOTATION, SERVER_CONFIG_JSON_SCHEMA_PATH, server_config_json_schema};
use crate::server_config::{
    build_features::{declared_features, missing_features},
    methods::{RefusedKey, refused_keys},
};

/// Set by the CI leg that runs the drift test with every feature on. Where it is set, a build
/// that leaves a feature off fails the test instead of skipping it: that leg is the one place
/// the comparison is made.
const REQUIRE_ALL_FEATURES_ENV: &str = "FRAISEQL_TEST_REQUIRE_ALL_FEATURES";

/// What the drift test does in a build.
#[derive(Debug, PartialEq, Eq)]
enum Comparison {
    /// Every feature is on: the generated schema is the committed one's source.
    Compare,
    /// A feature is off, so this build generates a narrower schema; the reason names them.
    Skip(String),
}

/// The drift test's branch, from the features the build leaves off (comma-separated, as the
/// build script records them) and whether the run requires every feature.
fn comparison(missing: &str, required: bool) -> Comparison {
    if missing.is_empty() {
        return Comparison::Compare;
    }
    assert!(
        !required,
        "{REQUIRE_ALL_FEATURES_ENV} is set but this build leaves off {missing}: the run that \
         compares the committed configuration schema must build every feature"
    );
    Comparison::Skip(format!(
        "this build leaves off {missing}, so it generates a narrower schema than the \
         committed one, which every feature generates"
    ))
}

/// The committed schema is the one a build with every feature generates.
/// `make bless-server-config-schema` rewrites it. A narrower build lists it as ignored.
#[test]
#[cfg_attr(
    not(fraiseql_server_all_features),
    ignore = "the committed schema is the one a build with every feature generates"
)]
fn the_committed_schema_is_the_generated_one() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(SERVER_CONFIG_JSON_SCHEMA_PATH);
    let generated = serde_json::to_string_pretty(&server_config_json_schema()).unwrap() + "\n";
    if std::env::var_os("FRAISEQL_BLESS_SERVER_CONFIG_SCHEMA").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &generated).unwrap();
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        committed == generated,
        "{SERVER_CONFIG_JSON_SCHEMA_PATH} is not the schema the server generates; run \
         `make bless-server-config-schema`"
    );
}

/// The run that compares the committed schema builds every feature, so the drift test above
/// is not listed as ignored there. It runs in every build: where the run requires every
/// feature and this build leaves one off, it fails.
#[test]
fn the_run_that_compares_the_schema_builds_every_feature() {
    let required = std::env::var_os(REQUIRE_ALL_FEATURES_ENV).is_some();
    let missing = env!("FRAISEQL_SERVER_MISSING_FEATURES");
    let compares = comparison(missing, required) == Comparison::Compare;
    assert_eq!(
        compares,
        cfg!(fraiseql_server_all_features),
        "the build script's cfg and its list of features left off disagree: {missing:?}"
    );
}

#[test]
fn a_build_with_every_feature_compares() {
    assert_eq!(comparison("", false), Comparison::Compare);
    assert_eq!(comparison("", true), Comparison::Compare);
}

#[test]
fn a_narrower_build_skips_and_names_what_it_leaves_off() {
    let outcome = comparison("grpc,aws-s3", false);
    assert!(
        matches!(&outcome, Comparison::Skip(why) if why.contains("grpc,aws-s3")),
        "a build without grpc and aws-s3 must skip, naming them: {outcome:?}"
    );
}

#[test]
#[should_panic(
    expected = "FRAISEQL_TEST_REQUIRE_ALL_FEATURES is set but this build leaves off grpc"
)]
fn a_narrower_build_fails_where_the_run_requires_every_feature() {
    let _ = comparison("grpc", true);
}

/// The schema names itself by this release's URL.
#[test]
fn the_schema_is_identified_by_this_releases_url() {
    assert_eq!(
        server_config_json_schema()["$id"],
        format!(
            "https://raw.githubusercontent.com/fraiseql/fraiseql/v{}/docs/schemas/fraiseql-server.schema.json",
            env!("CARGO_PKG_VERSION")
        )
    );
}

// ── the features a build leaves off ───────────────────────────────────────────────

const MANIFEST: &str = r#"[package]
name = "demo"

[features]
# a comment = ["x"]
default = ["auth"]
auth = []
grpc = [
  "dep:tonic",
  "auth",
]
aws-s3 = ["dep:aws"]

[package.metadata.docs.rs]
all-features = true
"#;

#[test]
fn the_declared_features_are_the_features_tables_keys() {
    assert_eq!(declared_features(MANIFEST), ["auth", "grpc", "aws-s3"]);
}

#[test]
fn a_build_leaves_off_the_features_it_does_not_enable() {
    assert_eq!(missing_features(MANIFEST, |f| f == "auth"), ["grpc", "aws-s3"]);
    assert!(missing_features(MANIFEST, |_| true).is_empty());
}

/// Read from this crate's real manifest: its features, and none of the keys of the tables
/// around `[features]`.
#[test]
fn this_crates_manifest_declares_its_features() {
    let manifest = include_str!("../../../Cargo.toml");
    let declared = declared_features(manifest);
    for feature in ["auth", "grpc", "functions-runtime-deno", "cdc-kafka"] {
        assert!(declared.iter().any(|f| f == feature), "{feature} missing from {declared:?}");
    }
    for key in ["default", "name", "categories", "exclude"] {
        assert!(!declared.iter().any(|f| f == key), "{key} read as a feature: {declared:?}");
    }
}

// ── the keys the loader refuses by name ───────────────────────────────────────────

/// A file writing `refused` and nothing else.
fn file_writing(refused: &RefusedKey) -> String {
    if refused.key.is_empty() {
        format!("[{}]\nx = 1\n", refused.section)
    } else {
        format!("[{}]\n{} = \"x\"\n", refused.section, refused.key)
    }
}

/// The schema node for `refused`, followed through `$ref`s and an `Option`'s value arm.
fn schema_node<'a>(schema: &'a Value, refused: &RefusedKey) -> Option<&'a Value> {
    let follow = |mut node: &'a Value| {
        for _ in 0..64 {
            if let Some(name) = node
                .get("$ref")
                .and_then(Value::as_str)
                .and_then(|r| r.strip_prefix("#/$defs/"))
            {
                node = &schema["$defs"][name];
            } else if let Some(arm) = node.get("anyOf").and_then(Value::as_array).and_then(|arms| {
                arms.iter().find(|a| a.get("type").and_then(Value::as_str) != Some("null"))
            }) {
                node = arm;
            } else {
                break;
            }
        }
        node
    };
    let section = schema["properties"].get(refused.section)?;
    if refused.key.is_empty() {
        return Some(section);
    }
    follow(section)["properties"].get(refused.key)
}

/// Every key the loader refuses by name is refused by the loader with its row's diagnostic
/// (the code, the table, the key and the real spelling, not a guess at the nearest one), and
/// by the schema, deprecated, described in the loader's words.
#[test]
fn every_key_the_loader_refuses_is_refused_by_the_schema_with_its_words() {
    let schema = server_config_json_schema();
    let refused = refused_keys();
    assert!(refused.len() >= 3, "{} refused keys", refused.len());
    let dir = tempfile::tempdir().unwrap();
    for key in &refused {
        let path = dir.path().join("server.toml");
        std::fs::write(&path, file_writing(key)).unwrap();
        let error = crate::server_config::ServerConfig::load_file(&path).err();
        let mut loader = error.as_ref().and_then(|e| e.diagnostic()).cloned();
        if let Some(diagnostic) = loader.as_mut() {
            diagnostic.file = None;
        }
        assert_eq!(
            loader.as_ref(),
            Some(&key.diagnostic),
            "[{}] {}: the loader's refusal of {:?}",
            key.section,
            key.key,
            error.map(|e| e.to_string())
        );
        assert_eq!(loader.unwrap().message, key.note, "[{}] {}", key.section, key.key);
        let node = schema_node(&schema, key);
        assert!(
            node.is_some_and(is_refused),
            "[{}] {} is not refused by the schema: {node:?}",
            key.section,
            key.key
        );
        let node = node.unwrap();
        assert_eq!(node["deprecated"], true, "[{}] {}", key.section, key.key);
        assert_eq!(node["description"], key.note.as_str(), "[{}] {}", key.section, key.key);
    }
}

/// A key the schema refuses is one the loader refuses: no refusal in the schema the loader
/// does not give.
#[test]
fn the_schema_refuses_only_keys_the_loader_refuses() {
    let schema = server_config_json_schema();
    let refused = refused_keys();
    let mut found = Vec::new();
    collect_refused(&schema, &schema, "", &mut Vec::new(), &mut found);
    for (section, key) in &found {
        assert!(
            refused.iter().any(|r| r.section == section && r.key == key),
            "the schema refuses [{section}] {key}, which the loader does not refuse by name"
        );
    }
    assert_eq!(found.len(), refused.len(), "{found:?}");
}

/// Every refused property reachable from `node`, as (section, key): a top-level refused
/// table is (table, ""), a refused key one table down is (table, key).
fn collect_refused(
    root: &Value,
    node: &Value,
    section: &str,
    seen: &mut Vec<String>,
    found: &mut Vec<(String, String)>,
) {
    let Some(properties) = node.get("properties").and_then(Value::as_object) else {
        return;
    };
    for (name, child) in properties {
        if is_refused(child) {
            found.push(if section.is_empty() {
                (name.clone(), String::new())
            } else {
                (section.to_owned(), name.clone())
            });
            continue;
        }
        if !section.is_empty() {
            continue;
        }
        let mut target = child;
        for _ in 0..64 {
            if let Some(name) = target
                .get("$ref")
                .and_then(Value::as_str)
                .and_then(|r| r.strip_prefix("#/$defs/"))
            {
                if seen.iter().any(|s| s == name) {
                    break;
                }
                target = &root["$defs"][name];
            } else if let Some(arm) = target.get("anyOf").and_then(Value::as_array).and_then(|a| {
                a.iter().find(|a| a.get("type").and_then(Value::as_str) != Some("null"))
            }) {
                target = arm;
            } else {
                break;
            }
        }
        collect_refused(root, target, name, seen, found);
    }
}

// ── the keys a build compiles out ─────────────────────────────────────────────────

/// The committed schema, which every feature generates.
fn committed() -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(SERVER_CONFIG_JSON_SCHEMA_PATH);
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// Every key of every object the schema describes, as `(definition, key)`: the top level is
/// the definition `""`.
fn keys(schema: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut add = |definition: &str, node: &Value| {
        if let Some(properties) = node.get("properties").and_then(Value::as_object) {
            out.extend(properties.keys().map(|key| (definition.to_owned(), key.clone())));
        }
    };
    add("", schema);
    if let Some(definitions) = schema.get("$defs").and_then(Value::as_object) {
        for (name, node) in definitions {
            add(name, node);
        }
    }
    out
}

/// A key the committed schema has and this build's does not is one a feature this build
/// leaves off compiles in, and the committed schema says which. A key is judged in an object
/// this build still describes. Only a narrower build tests anything here: with every
/// feature on, nothing is missing.
#[test]
fn every_key_this_build_compiles_out_is_marked_with_its_feature() {
    let committed = committed();
    let built = keys(&server_config_json_schema());
    let left_off: Vec<&str> = env!("FRAISEQL_SERVER_MISSING_FEATURES")
        .split(',')
        .filter(|f| !f.is_empty())
        .collect();
    let mut unmarked = Vec::new();
    for (definition, key) in keys(&committed) {
        // A definition this build does not have at all is compiled out with the key that
        // holds it, and judged there.
        let defined = definition.is_empty() || built.iter().any(|(d, _)| d == &definition);
        if !defined || built.contains(&(definition.clone(), key.clone())) {
            continue;
        }
        let node = if definition.is_empty() {
            &committed["properties"][&key]
        } else {
            &committed["$defs"][&definition]["properties"][&key]
        };
        let feature = node.get(FEATURE_ANNOTATION).and_then(Value::as_str);
        if !feature.is_some_and(|feature| left_off.contains(&feature)) {
            unmarked.push(format!("{definition}.{key} ({FEATURE_ANNOTATION}: {feature:?})"));
        }
    }
    assert!(
        unmarked.is_empty(),
        "keys this build compiles out, with no feature it leaves off marked on them (it leaves \
         off {left_off:?}):\n{}",
        unmarked.join("\n")
    );
}

/// An annotation describes a key this build reads; it never adds one. A property holding
/// nothing but annotations would accept any value for a key this build compiled out.
#[test]
fn no_key_is_only_an_annotation() {
    let schema = server_config_json_schema();
    let mut phantoms = Vec::new();
    for (definition, key) in keys(&schema) {
        let node = if definition.is_empty() {
            &schema["properties"][&key]
        } else {
            &schema["$defs"][&definition]["properties"][&key]
        };
        let annotations_only = node
            .as_object()
            .is_some_and(|node| node.keys().all(|name| name.starts_with("x-fraiseql-")));
        if annotations_only {
            phantoms.push(format!("{definition}.{key}: {node}"));
        }
    }
    assert!(phantoms.is_empty(), "{}", phantoms.join("\n"));
}

// ── the walk against serde ────────────────────────────────────────────────────────

/// A key no table of the server's file takes.
const NOT_A_KEY: &str = "zz_not_a_key";

/// `node` with every `$ref` followed and an `Option`'s value arm taken; `None` for a union of
/// several shapes, which the walk judges arm by arm and this sweep does not reach into.
fn object_of<'a>(root: &'a Value, mut node: &'a Value) -> Option<&'a Value> {
    for _ in 0..64 {
        if let Some(name) = node
            .get("$ref")
            .and_then(Value::as_str)
            .and_then(|r| r.strip_prefix("#/$defs/"))
        {
            node = &root["$defs"][name];
            continue;
        }
        let arms: Vec<&Value> = ["anyOf", "oneOf"]
            .iter()
            .filter_map(|c| node.get(*c).and_then(Value::as_array))
            .flatten()
            .filter(|arm| arm.get("type").and_then(Value::as_str) != Some("null"))
            .collect();
        match arms.as_slice() {
            [] => return Some(node),
            [arm] => node = arm,
            _ => return None,
        }
    }
    Some(node)
}

/// How a file reaches a table: through a key, an array's item, or a map's entry.
#[derive(Clone, Debug)]
enum Step {
    Key(String),
    Item,
    Entry,
}

/// Every table the schema describes, by the steps a file takes to reach it, with its schema.
fn tables(root: &Value) -> Vec<(Vec<Step>, &Value)> {
    fn visit<'a>(
        root: &'a Value,
        node: &'a Value,
        steps: &mut Vec<Step>,
        out: &mut Vec<(Vec<Step>, &'a Value)>,
    ) {
        let Some(node) = object_of(root, node) else {
            return;
        };
        if steps.len() > 6 {
            return;
        }
        if let Some(items) = node.get("items") {
            steps.push(Step::Item);
            visit(root, items, steps, out);
            steps.pop();
            return;
        }
        if let Some(entry @ Value::Object(_)) = node.get("additionalProperties") {
            steps.push(Step::Entry);
            visit(root, entry, steps, out);
            steps.pop();
        }
        let Some(properties) = node.get("properties").and_then(Value::as_object) else {
            return;
        };
        out.push((steps.clone(), node));
        for (key, child) in properties {
            if is_refused(child) {
                continue;
            }
            steps.push(Step::Key(key.clone()));
            visit(root, child, steps, out);
            steps.pop();
        }
    }
    let mut out = Vec::new();
    visit(root, root, &mut Vec::new(), &mut out);
    out
}

/// A file holding `leaf` in the table `steps` reach.
fn file_reaching(steps: &[Step], leaf: toml::Table) -> toml::Table {
    let mut value = toml::Value::Table(leaf);
    for step in steps.iter().rev() {
        value = match step {
            Step::Key(key) => toml::Value::Table(toml::Table::from_iter([(key.clone(), value)])),
            Step::Entry => {
                toml::Value::Table(toml::Table::from_iter([("probe".to_owned(), value)]))
            },
            Step::Item => toml::Value::Array(vec![value]),
        };
    }
    match value {
        toml::Value::Table(table) => table,
        _ => toml::Table::new(),
    }
}

/// Whether serde, reading `file` as the server does, refuses `key` as a field it does not
/// know.
fn serde_refuses_as_unknown(file: &toml::Table, key: &str) -> bool {
    toml::Value::Table(file.clone())
        .try_into::<crate::server_config::ServerConfig>()
        .is_err_and(|e| e.to_string().contains(&format!("unknown field `{key}`")))
}

/// The walk refuses a key exactly where serde refuses it as unknown: in each table the
/// schema describes, a key no table takes is refused by both or by neither.
#[test]
fn the_walk_refuses_an_unknown_key_where_serde_does() {
    let schema = server_config_json_schema();
    let tables = tables(&schema);
    assert!(tables.len() >= 20, "{} tables reached", tables.len());
    let mut disagree = Vec::new();
    for (steps, _) in &tables {
        let file = file_reaching(steps, toml::Table::from_iter([(NOT_A_KEY.into(), 1.into())]));
        let walk = unknown_key(&schema, &file).is_some_and(|u| u.key == NOT_A_KEY);
        let serde = serde_refuses_as_unknown(&file, NOT_A_KEY);
        if walk != serde {
            disagree.push(format!("{steps:?}: the walk refuses: {walk}, serde refuses: {serde}"));
        }
    }
    assert!(disagree.is_empty(), "{}", disagree.join("\n"));
}

/// Every key the schema describes is one serde knows, and the walk takes.
#[test]
fn every_key_the_schema_describes_is_one_serde_knows() {
    let schema = server_config_json_schema();
    let mut unknown = Vec::new();
    for (steps, node) in tables(&schema) {
        let properties = node["properties"].as_object().unwrap();
        for (key, child) in properties {
            if is_refused(child) {
                continue;
            }
            let file = file_reaching(&steps, toml::Table::from_iter([(key.clone(), true.into())]));
            if serde_refuses_as_unknown(&file, key) || unknown_key(&schema, &file).is_some() {
                unknown.push(format!("{steps:?} {key}"));
            }
        }
    }
    assert!(unknown.is_empty(), "{}", unknown.join("\n"));
}

/// A table serde reads with no key written requires none in the schema, so an editor does not
/// demand a key the server defaults.
#[test]
fn a_table_serde_reads_empty_requires_no_key() {
    let schema = server_config_json_schema();
    let mut demanding = Vec::new();
    for (steps, node) in tables(&schema) {
        let required = node.get("required").and_then(Value::as_array).filter(|r| !r.is_empty());
        let file = file_reaching(&steps, toml::Table::new());
        let read = toml::Value::Table(file).try_into::<crate::server_config::ServerConfig>();
        if let (Some(required), Ok(_)) = (required, read) {
            demanding.push(format!("{steps:?} requires {required:?}"));
        }
    }
    assert!(demanding.is_empty(), "{}", demanding.join("\n"));
}
