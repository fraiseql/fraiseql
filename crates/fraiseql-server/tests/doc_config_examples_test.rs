//! #839 gate: every TOML example in the docs parses as the file it says it is.
//!
//! `docs/architecture/overview.md` shipped a production `fraiseql.toml` whose keys sat
//! in `[server]`/`[database]` grouping tables that `ServerConfig` does not have; serde
//! silently discarded every documented key and the example "worked" while configuring
//! nothing. The config structs now deny unknown fields, and this test closes the loop
//! on the docs side.
//!
//! Every toml-fenced block in `README.md` and `docs/` (history excepted) names its file
//! in its first non-empty line, a comment such as `# fraiseql.toml`, and parses as it:
//!
//! * `server.toml` deserializes into the server's [`ServerConfig`];
//! * `fraiseql.toml` deserializes into one of the compiler's two readers of that file:
//!   [`TomlSchema`] (the file as `fraiseql compile`'s input, TOML-first authoring) or
//!   [`TomlProjectConfig`] (the project config beside a `schema.json`). Two structs for one file
//!   name is the dialect split #1641 removes; until then either counts, and both deny unknown keys,
//!   so an invented key still fails;
//! * any other `*.toml` (`Cargo.toml`, `pyproject.toml`, ...) is valid TOML.
//!
//! A block that names no file fails: an unnamed example is one a reader cannot check,
//! and the two config dialects (D-9) make "which file is this?" a real question.
//!
//! Wired in `.dagger/main.go`'s test leg next to `config_coverage_manifest_test`,
//! with the full server feature set so every feature-gated section exists. The
//! `#![cfg]` guard below keeps a bare feature-less `cargo test` from false-failing —
//! the dagger line is the gate of record.

#![cfg(all(feature = "auth", feature = "observers", feature = "rest"))]
#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code
#![allow(missing_docs)] // Reason: test code

use std::path::{Path, PathBuf};

use fraiseql_cli::config::{TomlProjectConfig, TomlSchema};
use fraiseql_server::ServerConfig;

/// Repo root, resolved from this crate's manifest dir.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

/// Extract every toml-fenced code block from a markdown file, with its starting line.
fn toml_blocks(markdown: &str) -> Vec<(usize, String)> {
    let mut blocks = Vec::new();
    let mut current: Option<(usize, Vec<&str>)> = None;
    for (idx, line) in markdown.lines().enumerate() {
        let trimmed = line.trim_start();
        match &mut current {
            None if trimmed.starts_with("```toml") => current = Some((idx + 1, Vec::new())),
            None => {},
            Some(_) if trimmed.starts_with("```") => {
                let (start, lines) = current.take().unwrap();
                blocks.push((start, lines.join("\n")));
            },
            Some((_, lines)) => lines.push(line),
        }
    }
    blocks
}

/// The file a block names in its first non-empty line (`# fraiseql.toml — ...`), if any.
fn named_file(block: &str) -> Option<&str> {
    let first = block.lines().find(|l| !l.trim().is_empty())?.trim_start();
    let rest = first.strip_prefix('#')?.trim_start();
    let end = rest.find(".toml")? + ".toml".len();
    let name = &rest[..end];
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || "._-/".contains(c))
        .then_some(name)
}

/// Markdown files under `dir`, recursively, skipping the directories that record history.
fn markdown_files(dir: &Path, out: &mut Vec<PathBuf>) {
    const HISTORY: [&str; 3] = ["history", "audits", "migration"];
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if !HISTORY.iter().any(|h| path.ends_with(h)) {
                markdown_files(&path, out);
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            out.push(path);
        }
    }
}

#[test]
fn every_documented_toml_example_parses_as_the_file_it_names() {
    let root = repo_root();
    let mut files = vec![root.join("README.md")];
    markdown_files(&root.join("docs"), &mut files);
    files.sort();

    let (mut server, mut compiler, mut other) = (0usize, 0usize, 0usize);
    let mut failures = Vec::new();

    for path in &files {
        let content = std::fs::read_to_string(path).unwrap();
        let shown = path.strip_prefix(&root).unwrap().display();
        for (line, block) in toml_blocks(&content) {
            let result = match named_file(&block) {
                None => Err("names no file: start the block with a comment such as \
                             `# fraiseql.toml` or `# server.toml`"
                    .to_string()),
                Some(name) if name.ends_with("server.toml") => {
                    server += 1;
                    toml::from_str::<ServerConfig>(&block).map(drop).map_err(|e| e.to_string())
                },
                Some(name) if name.ends_with("fraiseql.toml") => {
                    compiler += 1;
                    toml::from_str::<TomlSchema>(&block).map(drop).or_else(|schema| {
                        toml::from_str::<TomlProjectConfig>(&block).map(drop).map_err(|project| {
                            format!("as TomlSchema: {schema}; as TomlProjectConfig: {project}")
                        })
                    })
                },
                Some(_) => {
                    other += 1;
                    toml::from_str::<toml::Value>(&block).map(drop).map_err(|e| e.to_string())
                },
            };
            if let Err(e) = result {
                failures.push(format!("{shown}:{line}: {e}"));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "documented TOML examples that do not parse as the file they name \
         (fix the doc or the struct — never let them drift):\n{}",
        failures.join("\n")
    );

    // A silently shrinking corpus is how the gate stops gating: if a floor trips because
    // examples were removed, lower it consciously.
    assert!(server >= 6, "expected at least 6 server.toml examples, found {server}");
    assert!(compiler >= 6, "expected at least 6 fraiseql.toml examples, found {compiler}");
    assert!(other >= 1, "expected at least 1 example of another TOML file, found {other}");
}

/// Top-level keys that author the schema: a block holding one is a TOML schema, the compile
/// input, and is not expected to be a project config.
const AUTHORING_KEYS: [&str; 8] = [
    "schema",
    "types",
    "queries",
    "mutations",
    "includes",
    "domain_discovery",
    "crud",
    "analytics",
];

/// `fraiseql.toml` examples that hold only settings and still parse as a TOML schema alone,
/// by file and by the top-level key the project config does not take. A reader who authors
/// with an SDK keeps a project config beside a `schema.json`, where that key is refused by
/// name with its project spelling (#1719). Shrink-only: a new example must work in both
/// files, or say which one it is by declaring a schema.
const TOML_SCHEMA_ONLY: &[(&str, &str)] = &[
    // Spelled `[fraiseql.tenancy]` and `[fraiseql.session_variables]` in a project config.
    ("docs/auth/auth0.md", "tenancy"),
    ("docs/features/session-variables.md", "session_variables"),
    ("docs/modules/fact-table.md", "session_variables"),
];

/// A settings-only `fraiseql.toml` example works beside a `schema.json` too.
#[test]
fn a_settings_only_fraiseql_toml_example_is_also_a_project_config() {
    let root = repo_root();
    let mut files = vec![root.join("README.md")];
    markdown_files(&root.join("docs"), &mut files);
    files.sort();

    let mut schema_only: Vec<(String, String)> = Vec::new();
    for path in &files {
        let content = std::fs::read_to_string(path).unwrap();
        let shown = path.strip_prefix(&root).unwrap().display().to_string();
        for (_, block) in toml_blocks(&content) {
            if !named_file(&block).is_some_and(|name| name.ends_with("fraiseql.toml")) {
                continue;
            }
            let Ok(table) = block.parse::<toml::Table>() else {
                continue;
            };
            let settings_only = !table.keys().any(|key| AUTHORING_KEYS.contains(&key.as_str()));
            if !settings_only || toml::from_str::<TomlSchema>(&block).is_err() {
                continue;
            }
            if toml::from_str::<TomlProjectConfig>(&block).is_ok() {
                continue;
            }
            // The keys the project config does not take, each once per file.
            let taken = toml::Table::try_from(TomlProjectConfig::default()).unwrap();
            for key in table.keys() {
                let probe = toml::Table::from_iter([(key.clone(), table[key].clone())]);
                let refused = toml::from_str::<TomlProjectConfig>(&probe.to_string()).is_err();
                if refused && !taken.contains_key(key) {
                    let entry = (shown.clone(), key.clone());
                    if !schema_only.contains(&entry) {
                        schema_only.push(entry);
                    }
                }
            }
        }
    }

    let expected: Vec<(String, String)> = TOML_SCHEMA_ONLY
        .iter()
        .map(|(file, key)| ((*file).to_string(), (*key).to_string()))
        .collect();
    let new: Vec<_> = schema_only.iter().filter(|entry| !expected.contains(entry)).collect();
    let gone: Vec<_> = expected.iter().filter(|entry| !schema_only.contains(entry)).collect();
    assert!(
        new.is_empty(),
        "settings-only fraiseql.toml examples a project config refuses; make each work beside \
         a schema.json, or list it in TOML_SCHEMA_ONLY:\n{new:#?}"
    );
    assert!(
        gone.is_empty(),
        "TOML_SCHEMA_ONLY entries that no longer apply; remove them:\n{gone:#?}"
    );
}

/// `fraiseql.toml` examples a project config parses that it does not load: by file and the
/// line the block starts on. Shrink-only: these predate the check below and are tracked to be
/// fixed, never extended to admit a new example.
const PROJECT_CONFIG_UNLOADED: &[(&str, usize)] = &[
    // A service account with neither roles nor scopes.
    ("docs/architecture/federation.md", 277),
    // Names mail templates that exist only in a real project.
    ("docs/auth/mail-templates.md", 8),
    // An empty `[auth]`, and `[auth.local]` password sign-in without `email_from`.
    ("docs/auth/sessions.md", 7),
    ("docs/auth/sessions.md", 97),
    // A circuit-breaker entry naming an entity the file does not declare.
    ("docs/runbooks/14-federation-circuit-breaker.md", 135),
];

/// A `fraiseql.toml` example a project config parses is also one it loads.
///
/// Parsing is the struct alone. Loading a project config also expands `${VAR}`, refuses a
/// misplaced or never-applied key by name, and validates every section, the TOML schema's
/// checks of the sections both files take included: what `fraiseql compile schema.json` does
/// with the file. An example that parses and then fails there is one a reader copies into a
/// refused compile.
#[test]
fn a_project_config_example_loads_and_validates() {
    let root = repo_root();
    let mut files = vec![root.join("README.md")];
    markdown_files(&root.join("docs"), &mut files);
    files.sort();

    let scratch = tempfile::TempDir::new().unwrap();
    let mut failures: Vec<(String, usize, String)> = Vec::new();
    let mut loaded = 0usize;
    for path in &files {
        let content = std::fs::read_to_string(path).unwrap();
        let shown = path.strip_prefix(&root).unwrap().display().to_string();
        for (line, block) in toml_blocks(&content) {
            if !named_file(&block).is_some_and(|name| name.ends_with("fraiseql.toml")) {
                continue;
            }
            if toml::from_str::<TomlProjectConfig>(&block).is_err() {
                continue;
            }
            let file = scratch.path().join("fraiseql.toml");
            std::fs::write(&file, &block).unwrap();
            let result = TomlProjectConfig::from_file(file.to_str().unwrap())
                .and_then(|config| config.validate());
            match result {
                Ok(()) => loaded += 1,
                Err(e) => failures.push((shown.clone(), line, format!("{e:#}"))),
            }
        }
    }

    let new: Vec<_> = failures
        .iter()
        .filter(|(file, line, _)| !PROJECT_CONFIG_UNLOADED.contains(&(file.as_str(), *line)))
        .collect();
    let gone: Vec<_> = PROJECT_CONFIG_UNLOADED
        .iter()
        .filter(|(file, line)| !failures.iter().any(|(f, l, _)| f == file && l == line))
        .collect();
    assert!(
        new.is_empty(),
        "fraiseql.toml examples a project config parses and then refuses to load:\n{new:#?}"
    );
    assert!(
        gone.is_empty(),
        "PROJECT_CONFIG_UNLOADED entries that now load; remove them:\n{gone:#?}"
    );
    assert!(
        loaded >= 6,
        "expected at least 6 project-config examples to load, found {loaded}"
    );
}
