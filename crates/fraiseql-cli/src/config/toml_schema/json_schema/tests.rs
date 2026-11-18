#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use super::*;

/// The schema of the table at `path` (`["project"]`, `["security", "rate_limiting"]`), with
/// every `$ref` followed and an `Option`'s `null` arm set aside.
fn table<'a>(schema: &'a serde_json::Value, path: &[&str]) -> &'a serde_json::Value {
    let mut at = follow(schema, schema);
    for key in path {
        at = follow(schema, &at["properties"][key]);
    }
    at
}

fn follow<'a>(root: &'a serde_json::Value, node: &'a serde_json::Value) -> &'a serde_json::Value {
    if let Some(reference) = node["$ref"].as_str() {
        let name = reference.rsplit('/').next().unwrap();
        return follow(root, &root["$defs"][name]);
    }
    for combinator in ["anyOf", "oneOf"] {
        if let Some(arms) = node[combinator].as_array() {
            if let Some(arm) = arms.iter().find(|arm| arm["type"] != "null") {
                return follow(root, arm);
            }
        }
    }
    node
}

/// `[project]` is a closed table: the schema names `name` and refuses a key it does not name,
/// as `deny_unknown_fields` does.
#[test]
fn project_is_a_closed_table() {
    let schema = toml_json_schema();
    let project = table(&schema, &["project"]);
    assert!(project["properties"]["name"].is_object(), "{project}");
    assert_eq!(project["additionalProperties"], false, "{project}");
}

/// A session variable is read through `FlatSessionVariable` (`#[serde(from)]`), one closed
/// table per source, and schemars follows `from`: the schema is that closed table per
/// source, each naming `name`, `source` and the one key of that source, and requiring all
/// three. The struct's own shape (an open `name` beside a flattened source) would let an
/// editor accept a key the parser refuses.
#[test]
fn a_session_variable_is_one_closed_table_per_source() {
    let schema = toml_json_schema();
    let item = &table(&schema, &["session_variables"])["properties"]["variables"]["items"];
    let reference = item["$ref"].as_str().unwrap_or_else(|| panic!("not a reference: {item}"));
    let mapping = &schema["$defs"][reference.rsplit('/').next().unwrap()];
    let arms = mapping["oneOf"]
        .as_array()
        .unwrap_or_else(|| panic!("not one arm per source: {mapping}"));
    assert!(mapping.get("properties").is_none(), "keys outside the arms: {mapping}");

    let mut sources = Vec::new();
    for arm in arms {
        let source = arm["properties"]["source"]["const"]
            .as_str()
            .unwrap_or_else(|| panic!("an arm without its source: {arm}"));
        let own = match source {
            "jwt" => "claim",
            "header" => "header",
            "literal" => "value",
            "enrichment" => "field",
            other => panic!("a source the parser does not read: {other}"),
        };
        let mut takes = vec!["name", "source", own];
        takes.sort_unstable();
        let mut named: Vec<&str> =
            arm["properties"].as_object().unwrap().keys().map(String::as_str).collect();
        named.sort_unstable();
        let mut required: Vec<&str> = arm["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|key| key.as_str())
            .collect();
        required.sort_unstable();

        assert_eq!(arm["additionalProperties"], false, "{source} is open: {arm}");
        assert_eq!(named, takes, "{source} names other keys: {arm}");
        assert_eq!(required, takes, "{source} requires other keys: {arm}");
        sources.push(source);
    }
    sources.sort_unstable();
    assert_eq!(sources, ["enrichment", "header", "jwt", "literal"]);
}

/// An arm of an internally tagged enum is read by its tag: under `source = "header"` the
/// walker refuses the key that source does not take, naming that source's keys, never
/// `header`, which the `jwt` arm alone would refuse first.
#[test]
fn a_tagged_arm_is_read_by_its_tag() {
    for (source, own, foreign) in [
        ("jwt", "claim", "header"),
        ("header", "header", "claim"),
        ("literal", "value", "field"),
        ("enrichment", "field", "value"),
    ] {
        let file: toml::Table = toml::from_str(&format!(
            "[[session_variables.variables]]\nname = \"a\"\nsource = \"{source}\"\n\
             {own} = \"x\"\n{foreign} = \"y\"\n"
        ))
        .unwrap();
        let found = unknown_key(&file).unwrap_or_else(|| panic!("{source} took `{foreign}`"));
        assert_eq!(found.key, foreign, "{source}: {found:?}");
        assert_eq!(found.table, "session_variables.variables", "{source}: {found:?}");
        let mut accepted: Vec<&str> = found.accepted.iter().map(String::as_str).collect();
        accepted.sort_unstable();
        let mut takes = vec!["name", "source", own];
        takes.sort_unstable();
        assert_eq!(accepted, takes, "{source}: {found:?}");
    }
}

/// A tag no arm writes is left to serde, which names the variants it reads, even beside keys
/// no single arm takes together.
#[test]
fn a_tag_no_arm_writes_is_left_to_serde() {
    let file: toml::Table = toml::from_str(
        "[[session_variables.variables]]\nname = \"a\"\nsource = \"cookie\"\nclaim = \"x\"\n\
         header = \"y\"\n",
    )
    .unwrap();
    let found = unknown_key(&file);
    assert!(found.is_none(), "{found:?}");
}

// ── the walker and serde read one grammar ────────────────────────────────────────

/// A step from a table to the value under it.
#[derive(Debug, Clone)]
enum Step {
    Key(String),
    Item,
    Entry,
}

/// Every closed table the schema reaches, once, with the steps from the file's top to it and
/// whether an arm of an `anyOf` / `oneOf` with several table arms is on the way.
fn closed_tables(root: &serde_json::Value) -> Vec<(Vec<Step>, &serde_json::Value, bool)> {
    let mut found = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::from([(Vec::new(), root, false)]);
    while let Some((path, node, among_arms)) = queue.pop_front() {
        if let Some(name) = node.get("$ref").and_then(serde_json::Value::as_str) {
            if seen.insert(name.to_owned()) {
                let name = name.rsplit('/').next().unwrap();
                queue.push_back((path, &root["$defs"][name], among_arms));
            }
            continue;
        }
        for combinator in ["anyOf", "oneOf"] {
            if let Some(arms) = node.get(combinator).and_then(serde_json::Value::as_array) {
                let tables = arms.iter().filter(|arm| arm["type"] != "null").count();
                for arm in arms.iter().filter(|arm| arm["type"] != "null") {
                    queue.push_back((path.clone(), arm, among_arms || tables > 1));
                }
            }
        }
        if node["additionalProperties"] == false {
            found.push((path.clone(), node, among_arms));
        }
        if let Some(properties) = node.get("properties").and_then(serde_json::Value::as_object) {
            for (key, child) in properties {
                let mut next = path.clone();
                next.push(Step::Key(key.clone()));
                queue.push_back((next, child, among_arms));
            }
        }
        if let Some(child @ serde_json::Value::Object(_)) = node.get("additionalProperties") {
            let mut next = path.clone();
            next.push(Step::Entry);
            queue.push_back((next, child, among_arms));
        }
        if let Some(child) = node.get("items") {
            let mut next = path.clone();
            next.push(Step::Item);
            queue.push_back((next, child, among_arms));
        }
    }
    found
}

/// A file holding `leaf` at `path`.
fn file_with(path: &[Step], leaf: toml::Table) -> toml::Table {
    let mut value = toml::Value::Table(leaf);
    for step in path.iter().rev() {
        value = match step {
            Step::Key(key) => toml::Value::Table(toml::Table::from_iter([(key.clone(), value)])),
            Step::Entry => toml::Value::Table(toml::Table::from_iter([("x".to_owned(), value)])),
            Step::Item => toml::Value::Array(vec![value]),
        };
    }
    match value {
        toml::Value::Table(table) => table,
        _ => panic!("a file is a table"),
    }
}

/// The dotted table the walker names for `path`: the table header the file writes, with a
/// map's entry by its name and an array of tables by the array's.
fn dotted(path: &[Step]) -> String {
    path.iter()
        .filter_map(|step| match step {
            Step::Key(key) => Some(key.as_str()),
            Step::Entry => Some("x"),
            Step::Item => None,
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// The keys `node` takes: its properties, less the keys the compiler refuses by name.
fn accepted_by(node: &serde_json::Value) -> impl Iterator<Item = &String> {
    node["properties"]
        .as_object()
        .into_iter()
        .flat_map(|properties| properties.iter())
        .filter(|(_, schema)| !is_refused(schema))
        .map(|(key, _)| key)
}

/// What serde says of `file`, read as `fraiseql.toml` is, without the walker.
fn serde_reads(file: &toml::Table) -> Result<(), String> {
    let text = toml::to_string(file).unwrap();
    toml::from_str::<super::super::TomlSchema>(&text)
        .map(drop)
        .map_err(|e| e.to_string())
}

/// The keys serde names as expected when it refuses `key`.
fn serde_expects(error: &str, key: &str) -> Option<Vec<String>> {
    let after = error.split(&format!("unknown field `{key}`, ")).nth(1)?;
    let line = after.lines().next()?;
    if line.strip_prefix("there are no fields").is_some() {
        return Some(Vec::new());
    }
    let listed = line
        .strip_prefix("expected one of ")
        .or_else(|| line.strip_prefix("expected "))?;
    Some(
        listed
            .split(", ")
            .flat_map(|k| k.split(" or "))
            .map(|k| k.trim().trim_matches('`').to_owned())
            .collect(),
    )
}

/// For every closed table the schema reaches: an unknown key there is refused by the walker,
/// naming that table and key, and by serde reading without the walker; and the keys serde
/// expects there are the keys the schema lists. So the walker refuses nothing serde takes and
/// takes nothing serde refuses, table by table.
#[test]
fn every_closed_table_is_closed_alike_for_the_walker_and_serde() {
    const UNKNOWN: &str = "zz_not_a_key";
    let root = schema();
    let tables = closed_tables(root);
    assert!(tables.len() > 60, "the sweep reaches the grammar: {} tables", tables.len());

    let mut disagreements = Vec::new();
    for (path, node, among_arms) in &tables {
        let at = dotted(path);
        // An arm of an internally tagged enum is written with its tag (`source = "jwt"`), so
        // serde reads that arm. serde does not name the tag among the keys it expects.
        let tags: toml::Table = node["properties"]
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(key, schema)| Some((key.clone(), schema["const"].as_str()?.into())))
            .collect();
        let mut leaf = tags.clone();
        leaf.insert(UNKNOWN.to_owned(), 0.into());
        let file = file_with(path, leaf);

        match unknown_key(&file) {
            Some(found) if found.table == at && found.key == UNKNOWN => {},
            other => disagreements.push(format!("[{at}] walker: {other:?}")),
        }
        // Every key the schema lists there passes the walker, an arm's key included.
        let listed_keys = accepted_by(node);
        for key in listed_keys {
            let file = file_with(path, toml::Table::from_iter([(key.clone(), 0.into())]));
            if let Some(found) = unknown_key(&file) {
                disagreements.push(format!("[{at}] walker refuses its own `{key}`: {found:?}"));
            }
        }
        let serde = serde_reads(&file);
        let Err(error) = serde else {
            disagreements.push(format!("[{at}] serde takes `{UNKNOWN}`"));
            continue;
        };
        if *among_arms && serde_expects(&error, UNKNOWN).is_none() {
            // An arm of an untagged enum: serde refuses the value, naming no field. An
            // internally tagged enum's arm names its fields, and is compared below.
            continue;
        }
        let listed: std::collections::BTreeSet<String> =
            accepted_by(node).filter(|key| !tags.contains_key(*key)).cloned().collect();
        match serde_expects(&error, UNKNOWN) {
            Some(expected) => {
                let expected: std::collections::BTreeSet<String> = expected.into_iter().collect();
                if expected != listed {
                    disagreements.push(format!(
                        "[{at}] serde expects {:?} that the schema does not list, and the schema \
                         lists {:?} that serde does not expect",
                        expected.difference(&listed).collect::<Vec<_>>(),
                        listed.difference(&expected).collect::<Vec<_>>(),
                    ));
                }
            },
            None => disagreements.push(format!("[{at}] serde refuses otherwise: {error}")),
        }
    }
    assert!(disagreements.is_empty(), "{}", disagreements.join("\n"));
}

// ── the keys the compiler refuses by name ────────────────────────────────────────

/// A file writing `refused`: its section as tables, and its key (or the section itself).
fn file_writing(refused: &crate::config::RefusedKey) -> String {
    match (refused.section, refused.key) {
        ("", key) => format!("{key} = 0\n"),
        (section, "") => format!("[{section}]\n"),
        (section, key) => format!("[{section}]\n{key} = 0\n"),
    }
}

/// Where the schema describes `refused`.
fn described_at<'a>(
    schema: &'a serde_json::Value,
    refused: &crate::config::RefusedKey,
) -> &'a serde_json::Value {
    let mut path: Vec<&str> = refused.section.split('.').filter(|p| !p.is_empty()).collect();
    if !refused.key.is_empty() {
        path.push(refused.key);
    }
    let (last, tables) = path.split_last().unwrap();
    &table(schema, tables)["properties"][last]
}

/// Every key the compiler refuses by name is in the schema where it is written: any value
/// invalid, deprecated, and described by the very refusal the compiler gives a file writing it.
#[test]
fn every_refused_key_is_refused_by_the_schema_with_the_compilers_words() {
    let schema = toml_json_schema();
    let refused_keys = crate::config::refused_keys();
    assert_eq!(
        refused_keys.len(),
        crate::config::RETIRED_SPELLINGS.len()
            + crate::config::SERVER_FILE_SPELLINGS.len()
            + crate::config::UNAPPLIED_KEYS.len(),
        "every row of the three refusal tables"
    );
    for refused in &refused_keys {
        let file = file_writing(refused);
        let error = super::super::TomlSchema::parse_toml(&file).unwrap_err();
        let diagnostic = error
            .downcast_ref::<fraiseql_core::utils::config_diagnostic::ConfigDiagnostic>()
            .unwrap_or_else(|| panic!("{file:?} is refused with a diagnostic: {error:#}"));

        let node = described_at(&schema, refused);
        assert_eq!(node["not"], serde_json::json!({}), "{file:?}: {node}");
        assert_eq!(node["deprecated"], true, "{file:?}: {node}");
        assert_eq!(node["description"], diagnostic.message, "{file:?}");
    }
}

/// And nothing else in the schema refuses every value: each `not` is one refusal's, once.
#[test]
fn only_the_refused_keys_refuse_every_value() {
    fn collect(node: &serde_json::Value, found: &mut Vec<String>) {
        match node {
            serde_json::Value::Object(map) => {
                if map.contains_key("not") {
                    found.push(map["description"].as_str().unwrap_or_default().to_owned());
                }
                map.values().for_each(|child| collect(child, found));
            },
            serde_json::Value::Array(items) => items.iter().for_each(|child| collect(child, found)),
            _ => {},
        }
    }
    let mut found = Vec::new();
    collect(&toml_json_schema(), &mut found);
    let mut expected: Vec<String> = crate::config::refused_keys()
        .into_iter()
        .map(|refused| refused.message)
        .collect();
    found.sort();
    expected.sort();
    assert_eq!(found, expected);
}

/// A key near only a refused spelling is not told to write the refused one.
#[test]
fn a_refused_key_is_never_suggested() {
    let file: toml::Table = "[project]\nname = \"app\"\ndescriptio = \"x\"\n".parse().unwrap();
    let unknown = unknown_key(&file).unwrap();
    assert_eq!(unknown.key, "descriptio");
    assert!(!unknown.accepted.iter().any(|key| key == "description"), "{unknown:?}");
}

// ── the committed file ───────────────────────────────────────────────────────────

/// The committed schema is the generated one. `make bless-toml-schema` rewrites it.
#[test]
fn the_committed_schema_is_the_generated_one() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(TOML_JSON_SCHEMA_PATH);
    let generated = serde_json::to_string_pretty(&toml_json_schema()).unwrap() + "\n";
    if std::env::var_os("FRAISEQL_BLESS_TOML_SCHEMA").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &generated).unwrap();
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        committed == generated,
        "{} is not the schema the compiler generates; run `make bless-toml-schema`",
        TOML_JSON_SCHEMA_PATH
    );
}

/// The schema names itself by this release's URL.
#[test]
fn the_schema_is_identified_by_this_releases_url() {
    assert_eq!(
        toml_json_schema()["$id"],
        format!(
            "https://raw.githubusercontent.com/fraiseql/fraiseql/v{}/docs/schemas/fraiseql.toml.schema.json",
            env!("CARGO_PKG_VERSION")
        )
    );
}
