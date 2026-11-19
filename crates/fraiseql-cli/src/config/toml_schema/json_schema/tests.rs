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

/// The keys `node` refuses with a refusal of [`crate::config::SECTION_REFUSALS`]: serde reads
/// them, for the section's check to refuse.
fn read_to_be_refused(node: &serde_json::Value) -> impl Iterator<Item = &String> {
    let messages: Vec<String> = crate::config::SECTION_REFUSALS
        .iter()
        .map(|(_, _, message)| message())
        .collect();
    node["properties"]
        .as_object()
        .into_iter()
        .flat_map(|properties| properties.iter())
        .filter(move |(_, schema)| {
            is_refused(schema)
                && schema["description"].as_str().is_some_and(|d| messages.iter().any(|m| m == d))
        })
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
        // A key a section's check refuses is refused by the schema, yet serde reads it: the
        // check needs its value to refuse it.
        let listed: std::collections::BTreeSet<String> = accepted_by(node)
            .chain(read_to_be_refused(node))
            .filter(|key| !tags.contains_key(*key))
            .cloned()
            .collect();
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

/// A file writing `refused`: its section as tables, and its key (or the section itself). The
/// value is a string, which every refused key's own type takes (a mail body is one), so the
/// refusal met is the key's and not a type error.
fn file_writing(refused: &crate::config::RefusedKey) -> String {
    match (refused.section, refused.key) {
        ("", key) => format!("{key} = \"x\"\n"),
        (section, "") => format!("[{section}]\n"),
        (section, key) => format!("[{section}]\n{key} = \"x\"\n"),
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
            + crate::config::UNAPPLIED_KEYS.len()
            + crate::config::SECTION_REFUSALS.len(),
        "every row of the four refusal tables"
    );
    for refused in &refused_keys {
        let file = file_writing(refused);
        let error = super::super::TomlSchema::parse_toml(&file)
            .and_then(|schema| schema.validate())
            .unwrap_err();
        let diagnostic = error
            .downcast_ref::<fraiseql_core::utils::config_diagnostic::ConfigDiagnostic>()
            .unwrap_or_else(|| panic!("{file:?} is refused with a diagnostic: {error:#}"));

        let node = described_at(&schema, refused);
        assert_eq!(node["not"], serde_json::json!({}), "{file:?}: {node}");
        assert_eq!(node["deprecated"], true, "{file:?}: {node}");
        assert_eq!(node["description"], diagnostic.message, "{file:?}");
    }
}

/// A key a section's own check refuses whenever it is written is refused by the schema too,
/// with that check's words. Each file goes through the compiler's path, parsed then
/// validated, and the node is looked up where the refusal says the key is. The three kinds
/// of mail template share one node, so a mail template's refusal names no kind.
#[test]
fn every_key_a_section_check_refuses_is_refused_by_the_schema_with_its_words() {
    let schema = toml_json_schema();
    let mut files = vec![
        "[rest]\nmax_embedded_reads = 5\n".to_owned(),
        "[caching]\nredis_url = \"redis://cache:6379\"\n".to_owned(),
        "[observers]\nredis_url = \"redis://cache:6379\"\n".to_owned(),
    ];
    for kind in ["otp", "reset", "verification"] {
        for key in ["text", "html"] {
            files.push(format!("[auth.local.email.{kind}]\n{key} = \"Your code\"\n"));
        }
    }
    for file in &files {
        let error = super::super::TomlSchema::parse_toml(file)
            .and_then(|schema| schema.validate())
            .unwrap_err();
        let diagnostic = error
            .downcast_ref::<fraiseql_core::utils::config_diagnostic::ConfigDiagnostic>()
            .unwrap_or_else(|| panic!("{file:?} is refused with a diagnostic: {error:#}"));
        let tables: Vec<&str> = diagnostic.table.split('.').collect();
        let key = diagnostic.key.as_deref().unwrap_or_else(|| panic!("{file:?} names a key"));

        let node = &table(&schema, &tables)["properties"][key];
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

// ── every fraiseql.toml in the repository ────────────────────────────────────────

/// A `fraiseql.toml` the repository keeps invalid on purpose, by its path from the repository
/// root, with the reason. An entry whose file validates, or is gone, is reported, so the list
/// cannot outlive the files it excuses.
const INVALID_ON_PURPOSE: &[(&str, &str)] = &[];

/// Directories the walk does not enter: version control, build output and installed packages,
/// none of which the repository authors.
const NOT_AUTHORED: &[&str] = &[".git", "target", "node_modules"];

/// Every file named `fraiseql.toml` under `root`, by its path from it, sorted. The walk reads
/// the tree rather than git's index, which CI's source does not carry.
fn toml_files_under(root: &std::path::Path) -> Vec<String> {
    let mut found: Vec<String> = walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() == 0
                || !entry.file_type().is_dir()
                || !NOT_AUTHORED.iter().any(|name| entry.file_name() == *name)
        })
        .map(Result::unwrap)
        .filter(|entry| entry.file_type().is_file() && entry.file_name() == "fraiseql.toml")
        .map(|entry| entry.path().strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"))
        .collect();
    found.sort();
    found
}

/// A TOML value as an editor validates it: a date or time is its text. A float JSON cannot
/// hold (`nan`, `inf`) is `null`.
fn as_json(value: &toml::Value) -> serde_json::Value {
    match value {
        toml::Value::String(text) => text.clone().into(),
        toml::Value::Integer(integer) => (*integer).into(),
        toml::Value::Float(float) => serde_json::Number::from_f64(*float)
            .map_or(serde_json::Value::Null, serde_json::Value::Number),
        toml::Value::Boolean(boolean) => (*boolean).into(),
        toml::Value::Datetime(datetime) => datetime.to_string().into(),
        toml::Value::Array(items) => items.iter().map(as_json).collect(),
        toml::Value::Table(table) => {
            table.iter().map(|(key, value)| (key.clone(), as_json(value))).collect()
        },
    }
}

/// What is wrong with the `fraiseql.toml` files under `root`, judged by `schema`: a file it
/// refuses (where in the file, and why), a file that is not TOML, an entry of
/// `invalid_on_purpose` that is stale or gives no reason, or no file at all. Empty when every
/// file is as listed.
///
/// A file is judged as written: a `${VAR}` placeholder is not expanded, as the compiler leaves
/// one it cannot expand, so the verdict never depends on the environment it runs in.
fn findings(
    root: &std::path::Path,
    schema: &serde_json::Value,
    invalid_on_purpose: &[(&str, &str)],
) -> Vec<String> {
    let validator = jsonschema::draft202012::new(schema).unwrap();
    let files = toml_files_under(root);
    let mut findings = Vec::new();
    if files.is_empty() {
        findings.push(format!("no fraiseql.toml found under {}", root.display()));
    }
    for file in &files {
        let text = std::fs::read_to_string(root.join(file)).unwrap();
        let refusals: Vec<String> = match toml::from_str::<toml::Table>(&text) {
            Err(error) => vec![format!("{file}: not TOML: {error}")],
            Ok(table) => {
                let instance = as_json(&toml::Value::Table(table));
                validator
                    .iter_errors(&instance)
                    .map(|error| {
                        let at = error.instance_path().as_str();
                        format!("{file} at `{}`: {error}", if at.is_empty() { "/" } else { at })
                    })
                    .collect()
            },
        };
        let listed = invalid_on_purpose.iter().any(|(path, _)| path == file);
        match (listed, refusals.is_empty()) {
            (false, false) => findings.extend(refusals),
            (true, true) => findings.push(format!(
                "{file} is listed as invalid on purpose but validates: remove its entry"
            )),
            _ => {},
        }
    }
    for (path, reason) in invalid_on_purpose {
        if !files.iter().any(|file| file == path) {
            findings.push(format!(
                "{path} is listed as invalid on purpose but is not a fraiseql.toml under the \
                 repository: remove its entry"
            ));
        }
        if reason.trim().is_empty() {
            findings.push(format!("{path} is listed as invalid on purpose with no reason"));
        }
    }
    findings
}

/// Every `fraiseql.toml` the repository holds validates against the committed schema, so an
/// editor never flags a file the compiler takes. The schema is checked against the parser key
/// by key; this checks it value by value, on the files the repository ships as examples.
#[test]
fn every_fraiseql_toml_in_the_repository_validates_against_the_committed_schema() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let committed: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join(TOML_JSON_SCHEMA_PATH)).unwrap())
            .unwrap();
    let findings = findings(&root.canonicalize().unwrap(), &committed, INVALID_ON_PURPOSE);
    assert!(
        findings.is_empty(),
        "fraiseql.toml files the committed schema refuses. Fix the schema's type (a \
         `Deserialize` taking a shape its `JsonSchema` does not describe), or list a file kept \
         invalid on purpose in INVALID_ON_PURPOSE with its reason. A file git does not track \
         is judged too: delete it.\n{}",
        findings.join("\n")
    );
}

/// A tree of `(path, contents)` files in a fresh directory.
fn tree(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, contents) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
    dir
}

const VALID: &str = "[project]\nname = \"shop\"\n";
const MISSPELT: &str = "[project]\nnmae = \"shop\"\n";

fn judged(files: &[(&str, &str)], invalid_on_purpose: &[(&str, &str)]) -> Vec<String> {
    let dir = tree(files);
    findings(dir.path(), &toml_json_schema(), invalid_on_purpose)
}

#[test]
fn a_valid_file_has_no_finding() {
    assert_eq!(judged(&[("a/fraiseql.toml", VALID)], &[]), Vec::<String>::new());
}

#[test]
fn a_refused_file_is_named_with_where_and_why() {
    let findings = judged(&[("a/fraiseql.toml", VALID), ("b/fraiseql.toml", MISSPELT)], &[]);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(
        findings[0].strip_prefix("b/fraiseql.toml at `/project`: ").is_some(),
        "{findings:?}"
    );
    assert!(findings[0].contains("nmae"), "{findings:?}");
}

#[test]
fn a_file_that_is_not_toml_is_a_finding() {
    let findings = judged(&[("fraiseql.toml", "[project\n")], &[]);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(findings[0].strip_prefix("fraiseql.toml: not TOML: ").is_some(), "{findings:?}");
}

#[test]
fn a_placeholder_is_judged_as_written() {
    let file = "[database]\nurl = \"${FRAISEQL_UNSET_PLACEHOLDER}\"\n";
    assert_eq!(judged(&[("fraiseql.toml", file)], &[]), Vec::<String>::new());
}

#[test]
fn a_file_listed_as_invalid_on_purpose_is_not_reported() {
    let listed = [("b/fraiseql.toml", "a refusal fixture")];
    assert_eq!(judged(&[("b/fraiseql.toml", MISSPELT)], &listed), Vec::<String>::new());
}

/// The list is by path: an entry excuses its own file and no other.
#[test]
fn a_listing_excuses_only_its_own_file() {
    let listed = [("b/fraiseql.toml", "a refusal fixture")];
    let findings = judged(&[("b/fraiseql.toml", MISSPELT), ("c/fraiseql.toml", MISSPELT)], &listed);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(
        findings[0].strip_prefix("c/fraiseql.toml at `/project`: ").is_some(),
        "{findings:?}"
    );
}

#[test]
fn a_listed_file_that_validates_is_stale() {
    let listed = [("a/fraiseql.toml", "a refusal fixture")];
    let findings = judged(&[("a/fraiseql.toml", VALID)], &listed);
    assert_eq!(
        findings,
        ["a/fraiseql.toml is listed as invalid on purpose but validates: remove its entry"]
    );
}

#[test]
fn a_listed_file_that_is_gone_is_stale() {
    let listed = [("gone/fraiseql.toml", "a refusal fixture")];
    let findings = judged(&[("a/fraiseql.toml", VALID)], &listed);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(
        findings[0].strip_prefix("gone/fraiseql.toml is listed").is_some(),
        "{findings:?}"
    );
}

#[test]
fn a_listed_file_needs_a_reason() {
    let findings = judged(&[("b/fraiseql.toml", MISSPELT)], &[("b/fraiseql.toml", " ")]);
    assert_eq!(findings, ["b/fraiseql.toml is listed as invalid on purpose with no reason"]);
}

#[test]
fn no_file_found_is_a_finding() {
    let findings = judged(&[("a/other.toml", MISSPELT)], &[]);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(
        findings[0].strip_prefix("no fraiseql.toml found under ").is_some(),
        "{findings:?}"
    );
}

#[test]
fn the_walk_skips_what_the_repository_does_not_author() {
    let dir = tree(&[
        ("fraiseql.toml", VALID),
        ("examples/shop/fraiseql.toml", VALID),
        (".git/fraiseql.toml", MISSPELT),
        ("crates/x/target/fraiseql.toml", MISSPELT),
        ("sdk/node_modules/pkg/fraiseql.toml", MISSPELT),
        ("targets/fraiseql.toml", VALID),
    ]);
    assert_eq!(
        toml_files_under(dir.path()),
        [
            "examples/shop/fraiseql.toml",
            "fraiseql.toml",
            "targets/fraiseql.toml"
        ]
    );
    assert_eq!(findings(dir.path(), &toml_json_schema(), &[]), Vec::<String>::new());
}
