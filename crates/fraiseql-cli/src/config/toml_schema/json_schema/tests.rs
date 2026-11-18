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
        let file = file_with(path, toml::Table::from_iter([(UNKNOWN.to_owned(), 0.into())]));

        match unknown_key(&file) {
            Some(found) if found.table == at && found.key == UNKNOWN => {},
            other => disagreements.push(format!("[{at}] walker: {other:?}")),
        }
        // Every key the schema lists there passes the walker, an arm's key included.
        let listed_keys =
            node["properties"].as_object().into_iter().flat_map(serde_json::Map::keys);
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
        if *among_arms {
            // An untagged enum: serde refuses the value, naming no field.
            continue;
        }
        let listed: std::collections::BTreeSet<String> = node["properties"]
            .as_object()
            .map(|p| p.keys().cloned().collect())
            .unwrap_or_default();
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
