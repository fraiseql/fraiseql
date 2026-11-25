#![allow(clippy::unwrap_used)] // Reason: test module
#![allow(clippy::panic)] // Reason: test module, panics are the failure mechanism
#![allow(clippy::format_collect)] // Reason: test module, building fixture sources

use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
};

use tempfile::TempDir;

use super::{GraphError, MAX_GRAPH_BYTES, MAX_MODULE_BYTES, MAX_MODULES, ModuleGraph, collect};
use crate::modules::resolve::Refusal;

const ENTRY: &str = "function `notify`";

/// A shared directory holding `files`, and its canonical path.
fn shared(files: &[(&str, &str)]) -> (TempDir, PathBuf) {
    let root = TempDir::new().unwrap();
    let shared = root.path().join("functions/shared");
    std::fs::create_dir_all(&shared).unwrap();
    std::fs::write(root.path().join("functions/notify.ts"), "export default () => 1;\n").unwrap();
    for (path, source) in files {
        let path = shared.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }
    let canonical = shared.canonicalize().unwrap();
    (root, canonical)
}

fn keys(graph: &ModuleGraph) -> Vec<&str> {
    graph.modules.keys().map(super::ModuleKey::as_str).collect()
}

fn refusal(entry: &str, shared_root: Option<&Path>) -> (String, String, Refusal) {
    match collect(ENTRY, entry, shared_root) {
        Err(GraphError::Refused {
            importer,
            specifier,
            refusal,
        }) => (importer, specifier, refusal),
        other => panic!("expected a refused import, got {other:?}"),
    }
}

// ── What is followed ─────────────────────────────────────────────────────────

#[test]
fn a_function_with_no_imports_has_an_empty_graph() {
    let (_root, root) = shared(&[]);
    let entry = "export default async (event) => event;\n";
    assert_eq!(collect(ENTRY, entry, Some(&root)).unwrap(), ModuleGraph::default());
    assert_eq!(collect(ENTRY, entry, None).unwrap(), ModuleGraph::default());
}

#[test]
fn every_import_form_is_followed() {
    let (_root, root) = shared(&[
        ("a.ts", "export const a = 1;\n"),
        ("b.ts", "export const b = 1;\n"),
        ("c.ts", "export const c = 1;\n"),
        ("d.ts", "export const d = 1;\n"),
        ("e.ts", "export const e = 1;\n"),
        ("f.ts", "export type F = number;\n"),
        ("g.ts", "export const g = 1;\n"),
    ]);
    let entry = r#"
        import { a } from "@shared/a.ts";
        export { b } from "@shared/b.ts";
        export * from "@shared/c.ts";
        import type { F } from "@shared/f.ts";
        import "@shared/g.ts";
        export default async () => {
            const d = await import("@shared/d.ts");
            const e = await import(`@shared/e.ts`);
            return a + d.d + e.e;
        };
    "#;
    let graph = collect(ENTRY, entry, Some(&root)).unwrap();
    assert_eq!(keys(&graph), ["a.ts", "b.ts", "c.ts", "d.ts", "e.ts", "f.ts", "g.ts"]);
    assert_eq!(
        graph.modules[&crate::modules::resolve::resolve_specifier("@shared/a.ts").unwrap()],
        "export const a = 1;\n"
    );
}

#[test]
fn a_transitive_import_is_followed() {
    let (_root, root) = shared(&[
        ("a.ts", "import { b } from \"@shared/lib/b.ts\";\nexport const a = b;\n"),
        ("lib/b.ts", "import { c } from \"@shared/lib/c.mjs\";\nexport const b = c;\n"),
        ("lib/c.mjs", "export const c = 1;\n"),
    ]);
    let entry = "import { a } from \"@shared/a.ts\";\nexport default () => a;\n";
    assert_eq!(
        keys(&collect(ENTRY, entry, Some(&root)).unwrap()),
        ["a.ts", "lib/b.ts", "lib/c.mjs"]
    );
}

#[test]
fn an_import_cycle_ends_with_each_module_once() {
    let (_root, root) = shared(&[
        ("a.ts", "import { b } from \"@shared/b.ts\";\nexport const a = () => b;\n"),
        ("b.ts", "import { a } from \"@shared/a.ts\";\nexport const b = () => a;\n"),
    ]);
    let entry = "import { a } from \"@shared/a.ts\";\nexport default () => a;\n";
    assert_eq!(keys(&collect(ENTRY, entry, Some(&root)).unwrap()), ["a.ts", "b.ts"]);
}

#[test]
fn a_module_importing_itself_ends() {
    let (_root, root) =
        shared(&[("a.ts", "import * as me from \"@shared/a.ts\";\nexport const a = me;\n")]);
    let entry = "import { a } from \"@shared/a.ts\";\nexport default () => a;\n";
    assert_eq!(keys(&collect(ENTRY, entry, Some(&root)).unwrap()), ["a.ts"]);
}

/// A computed `import()` cannot be followed at load; the isolate resolves it through
/// the same rule and finds only what the graph holds.
#[test]
fn a_computed_dynamic_import_is_neither_followed_nor_refused() {
    let (_root, root) = shared(&[("a.ts", "export const a = 1;\n")]);
    let entry = "export default async (e) => (await import(e.path)).a;\n";
    assert_eq!(collect(ENTRY, entry, Some(&root)).unwrap(), ModuleGraph::default());
    let entry = "export default async (e) => (await import(`@shared/${e.name}.ts`)).a;\n";
    assert_eq!(collect(ENTRY, entry, Some(&root)).unwrap(), ModuleGraph::default());
}

/// The words `import "x"` in a comment or a string are not an import.
#[test]
fn an_import_in_a_comment_or_string_is_not_followed() {
    let (_root, root) = shared(&[]);
    let entry = "// import \"https://evil.example/x.ts\";\n\
                 const s = 'import \"./x.ts\"';\nexport default () => s;\n";
    assert_eq!(collect(ENTRY, entry, Some(&root)).unwrap(), ModuleGraph::default());
}

// ── What is refused, naming who wrote the import ─────────────────────────────

#[test]
fn without_a_shared_dir_any_import_is_refused_naming_the_key() {
    for specifier in [
        "@shared/a.ts",
        "./a.ts",
        "https://example.com/a.ts",
        "npm:zod",
    ] {
        let entry = format!("import {{ a }} from {specifier:?};\nexport default () => a;\n");
        let (importer, written, refusal) = refusal(&entry, None);
        assert_eq!((importer.as_str(), written.as_str()), (ENTRY, specifier));
        assert_eq!(refusal, Refusal::NoSharedDir, "{specifier}");
        assert!(refusal.to_string().contains("`[functions] shared_dir`"));
    }
}

#[test]
fn a_dynamic_import_without_a_shared_dir_is_refused() {
    let entry = "export default async () => (await import(\"@shared/a.ts\")).a;\n";
    assert_eq!(refusal(entry, None).2, Refusal::NoSharedDir);
}

#[test]
fn each_escape_is_refused_with_its_own_rule() {
    let (_root, root) = shared(&[("a.ts", "export const a = 1;\n")]);
    for (specifier, expected) in [
        (
            "https://example.com/a.ts",
            Refusal::Scheme {
                scheme: "https".into(),
            },
        ),
        (
            "npm:zod",
            Refusal::Scheme {
                scheme: "npm".into(),
            },
        ),
        (
            "jsr:@std/path",
            Refusal::Scheme {
                scheme: "jsr".into(),
            },
        ),
        (
            "node:fs",
            Refusal::Scheme {
                scheme: "node".into(),
            },
        ),
        (
            "data:text/javascript,export const a = 1",
            Refusal::Scheme {
                scheme: "data".into(),
            },
        ),
        ("/etc/passwd.ts", Refusal::Absolute),
        ("./a.ts", Refusal::Relative),
        ("../shared/a.ts", Refusal::Relative),
        ("lodash", Refusal::Bare),
        ("@shared/../notify.ts", Refusal::DotSegment),
        ("@shared//a.ts", Refusal::EmptySegment),
        ("@shared/..\\notify.ts", Refusal::Backslash),
        ("@shared/%2e%2e/notify.ts", Refusal::PercentEncoded),
        ("@shared/a", Refusal::NotAModuleExtension),
        ("@shared/missing.ts", Refusal::Missing),
    ] {
        let entry = format!("import {{ a }} from {specifier:?};\nexport default () => a;\n");
        assert_eq!(refusal(&entry, Some(&root)).2, expected, "{specifier}");
    }
}

#[test]
fn a_shared_module_writing_a_relative_import_is_refused_naming_it() {
    let (_root, root) = shared(&[
        ("a.ts", "import { b } from \"./b.ts\";\nexport const a = b;\n"),
        ("b.ts", "export const b = 1;\n"),
    ]);
    let entry = "import { a } from \"@shared/a.ts\";\nexport default () => a;\n";
    let (importer, specifier, refusal) = refusal(entry, Some(&root));
    assert_eq!((importer.as_str(), specifier.as_str()), ("@shared/a.ts", "./b.ts"));
    assert_eq!(refusal, Refusal::Relative);
}

#[test]
fn import_attributes_are_refused_in_every_form() {
    let (_root, root) = shared(&[("a.ts", "export const a = 1;\n")]);
    for entry in [
        "import a from \"@shared/a.ts\" with { type: \"json\" };\nexport default () => a;\n",
        "export { a } from \"@shared/a.ts\" with { type: \"json\" };\nexport default () => 1;\n",
        "export * from \"@shared/a.ts\" with { type: \"json\" };\nexport default () => 1;\n",
        "export default async () => await import(\"@shared/a.ts\", { with: { type: \"json\" } });\n",
    ] {
        assert_eq!(refusal(entry, Some(&root)).2, Refusal::ImportAttributes, "{entry}");
    }
}

#[cfg(unix)]
#[test]
fn a_symlink_escaping_the_shared_directory_is_refused() {
    let (root_dir, root) = shared(&[]);
    std::os::unix::fs::symlink(root_dir.path().join("functions/notify.ts"), root.join("fn.ts"))
        .unwrap();
    let entry = "import f from \"@shared/fn.ts\";\nexport default () => f;\n";
    assert!(matches!(refusal(entry, Some(&root)).2, Refusal::Escapes { .. }));
}

#[test]
fn a_shared_module_that_is_not_utf8_is_refused() {
    let (_root, root) = shared(&[]);
    std::fs::write(root.join("a.ts"), [0xff, 0xfe, 0x00]).unwrap();
    let entry = "import { a } from \"@shared/a.ts\";\nexport default () => a;\n";
    assert_eq!(refusal(entry, Some(&root)).2, Refusal::NotUtf8);
}

#[test]
fn a_shared_module_that_does_not_parse_is_refused_naming_it() {
    let (_root, root) = shared(&[("a.ts", "export const = ;\n")]);
    let entry = "import { a } from \"@shared/a.ts\";\nexport default () => a;\n";
    match collect(ENTRY, entry, Some(&root)) {
        Err(GraphError::Parse { module, .. }) => assert_eq!(module, "@shared/a.ts"),
        other => panic!("expected a parse error, got {other:?}"),
    }
}

// ── Limits ───────────────────────────────────────────────────────────────────

fn sized(bytes: u64) -> String {
    let mut source = String::from("export const a = 1;\n");
    source.push_str(&"/".repeat(usize::try_from(bytes).unwrap() - source.len()));
    source
}

#[test]
fn a_module_at_the_size_limit_loads_and_one_byte_more_is_refused() {
    let entry = "import { a } from \"@shared/a.ts\";\nexport default () => a;\n";
    let (_root, root) = shared(&[("a.ts", &sized(MAX_MODULE_BYTES))]);
    collect(ENTRY, entry, Some(&root)).unwrap();
    let (_root, root) = shared(&[("a.ts", &sized(MAX_MODULE_BYTES + 1))]);
    let error = collect(ENTRY, entry, Some(&root)).unwrap_err();
    assert!(
        matches!(
            error,
            GraphError::ModuleTooLarge {
                limit: MAX_MODULE_BYTES,
                ..
            }
        ),
        "{error:?}"
    );
    assert!(error.to_string().contains(&MAX_MODULE_BYTES.to_string()), "{error}");
}

/// Eight one-MiB modules fill the graph limit exactly; a ninth byte is refused.
#[test]
fn the_graph_size_limit_counts_every_module() {
    let full = sized(MAX_MODULE_BYTES);
    let names: Vec<String> = (0..8).map(|i| format!("m{i}.ts")).collect();
    let entry: String = names
        .iter()
        .enumerate()
        .map(|(i, name)| format!("import * as m{i} from \"@shared/{name}\";\n"))
        .collect::<String>()
        + "export default () => 1;\n";

    let files: Vec<(&str, &str)> =
        names.iter().map(|name| (name.as_str(), full.as_str())).collect();
    let (_root, root) = shared(&files);
    assert_eq!(MAX_GRAPH_BYTES, 8 * MAX_MODULE_BYTES);
    collect(ENTRY, &entry, Some(&root)).unwrap();

    let mut files = files;
    files.push(("extra.ts", "export const z = 1;\n"));
    let (_root, root) = shared(&files);
    let entry = format!("import \"@shared/extra.ts\";\n{entry}");
    let error = collect(ENTRY, &entry, Some(&root)).unwrap_err();
    assert_eq!(
        error,
        GraphError::GraphTooLarge {
            limit: MAX_GRAPH_BYTES,
        }
    );
}

/// One file reached under several keys is read, and counted, once.
#[cfg(unix)]
#[test]
fn one_file_under_several_keys_counts_once() {
    let (_root, root) = shared(&[("big.ts", &sized(MAX_MODULE_BYTES))]);
    let mut entry = String::new();
    for i in 0..9 {
        std::os::unix::fs::symlink(root.join("big.ts"), root.join(format!("alias{i}.ts"))).unwrap();
        writeln!(entry, "import * as a{i} from \"@shared/alias{i}.ts\";").unwrap();
    }
    entry.push_str("export default () => 1;\n");
    assert_eq!(collect(ENTRY, &entry, Some(&root)).unwrap().modules.len(), 9);
}

#[test]
fn the_module_count_limit_is_exact() {
    let names: Vec<String> = (0..=MAX_MODULES).map(|i| format!("m{i}.ts")).collect();
    let files: Vec<(&str, &str)> =
        names.iter().map(|name| (name.as_str(), "export {};\n")).collect();
    let (_root, root) = shared(&files);
    let imports = |count: usize| -> String {
        names[..count]
            .iter()
            .map(|name| format!("import \"@shared/{name}\";\n"))
            .collect::<String>()
            + "export default () => 1;\n"
    };
    assert_eq!(
        collect(ENTRY, &imports(MAX_MODULES), Some(&root)).unwrap().modules.len(),
        MAX_MODULES
    );
    assert_eq!(
        collect(ENTRY, &imports(MAX_MODULES + 1), Some(&root)).unwrap_err(),
        GraphError::TooManyModules { limit: MAX_MODULES }
    );
}

/// A lone surrogate is a JS string but not a path: it is refused, not lossily mapped
/// to a replacement character that could name another file.
#[test]
fn a_specifier_that_is_not_a_valid_string_is_refused() {
    let (_root, root) = shared(&[("a.ts", "export const a = 1;\n")]);
    let entry = "export default async () => (await import(\"@shared/\\uD800a.ts\")).a;\n";
    assert_eq!(refusal(entry, Some(&root)).2, Refusal::NotAString);
}
