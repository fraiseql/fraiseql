#![allow(clippy::unwrap_used)] // Reason: test module
#![allow(clippy::panic)] // Reason: test module, panics are the failure mechanism

use tempfile::TempDir;

use super::{ModuleKey, Refusal, locate, resolve_specifier};

fn refused(specifier: &str) -> Refusal {
    match resolve_specifier(specifier) {
        Ok(key) => panic!("{specifier:?} must be refused, resolved to {key}"),
        Err(refusal) => refusal,
    }
}

fn scheme(scheme: &str) -> Refusal {
    Refusal::Scheme {
        scheme: scheme.to_string(),
    }
}

// ── Lexical: accepted ────────────────────────────────────────────────────────

#[test]
fn a_shared_specifier_names_its_key() {
    for (specifier, key) in [
        ("@shared/llm.ts", "llm.ts"),
        ("@shared/llm/client.ts", "llm/client.ts"),
        ("@shared/a/b/c/budget.mjs", "a/b/c/budget.mjs"),
        ("@shared/v.js", "v.js"),
        ("@shared/v.mts", "v.mts"),
        ("@shared/.hidden.ts", ".hidden.ts"),
    ] {
        assert_eq!(resolve_specifier(specifier).unwrap().as_str(), key, "{specifier}");
    }
}

#[test]
fn a_key_round_trips_through_its_specifier_and_url() {
    let key = resolve_specifier("@shared/llm/client.ts").unwrap();
    assert_eq!(key.specifier(), "@shared/llm/client.ts");
    assert_eq!(key.url(), "file:///fraiseql/shared/llm/client.ts");
    assert_eq!(ModuleKey::from_url(&key.url()), Some(key));
}

#[test]
fn a_url_outside_the_virtual_root_names_no_key() {
    for url in [
        "file:///fraiseql/function.ts",
        "file:///etc/passwd.ts",
        "file:///fraiseql/shared/../function.ts",
        "file:///fraiseql/shared/x",
        "https://example.com/fraiseql/shared/x.ts",
    ] {
        assert_eq!(ModuleKey::from_url(url), None, "{url}");
    }
}

// ── Lexical: refused, one rule each ──────────────────────────────────────────

#[test]
fn every_scheme_is_refused_naming_it() {
    for (specifier, name) in [
        ("https://example.com/x.ts", "https"),
        ("http://example.com/x.ts", "http"),
        ("file:///etc/passwd.ts", "file"),
        ("data:text/javascript,export default 1", "data"),
        ("blob:null/1234", "blob"),
        ("npm:lodash", "npm"),
        ("jsr:@std/path", "jsr"),
        ("node:fs", "node"),
    ] {
        assert_eq!(refused(specifier), scheme(name), "{specifier}");
    }
}

#[test]
fn an_absolute_path_is_refused() {
    assert_eq!(refused("/etc/passwd.ts"), Refusal::Absolute);
    assert_eq!(refused("/srv/functions/shared/x.ts"), Refusal::Absolute);
}

#[test]
fn a_relative_path_is_refused() {
    for specifier in [
        "./x.ts",
        "../shared/x.ts",
        "./shared/x.ts",
        "../../etc/x.ts",
        ".",
        "..",
    ] {
        assert_eq!(refused(specifier), Refusal::Relative, "{specifier}");
    }
}

#[test]
fn a_bare_specifier_is_refused() {
    for specifier in [
        "lodash",
        "x.ts",
        "shared/x.ts",
        "@shared",
        "@other/x.ts",
        "@Shared/x.ts",
        "",
    ] {
        assert_eq!(refused(specifier), Refusal::Bare, "{specifier}");
    }
}

#[test]
fn a_dot_segment_is_refused() {
    for specifier in [
        "@shared/../x.ts",
        "@shared/../../etc/x.ts",
        "@shared/a/../../x.ts",
        "@shared/./x.ts",
        "@shared/a/./x.ts",
        "@shared/a/..",
    ] {
        assert_eq!(refused(specifier), Refusal::DotSegment, "{specifier}");
    }
}

#[test]
fn an_empty_segment_is_refused() {
    for specifier in ["@shared/", "@shared//x.ts", "@shared/a//x.ts", "@shared/a/"] {
        assert_eq!(refused(specifier), Refusal::EmptySegment, "{specifier}");
    }
}

#[test]
fn a_backslash_is_refused() {
    assert_eq!(refused("@shared/..\\..\\x.ts"), Refusal::Backslash);
    assert_eq!(refused("@shared/a\\x.ts"), Refusal::Backslash);
}

#[test]
fn percent_encoding_is_refused() {
    assert_eq!(refused("@shared/%2e%2e/x.ts"), Refusal::PercentEncoded);
    assert_eq!(refused("@shared/x%2Ets"), Refusal::PercentEncoded);
}

#[test]
fn a_control_character_is_refused() {
    assert_eq!(refused("@shared/x\0.ts"), Refusal::ControlCharacter);
    assert_eq!(refused("@shared/x\n.ts"), Refusal::ControlCharacter);
}

#[test]
fn a_file_without_a_module_extension_is_refused() {
    for specifier in [
        "@shared/llm",
        "@shared/llm/client",
        "@shared/data.json",
        "@shared/x.tsx",
        "@shared/.ts",
        "@shared/x.ts?v=1",
        "@shared/x.ts#frag",
    ] {
        assert_eq!(refused(specifier), Refusal::NotAModuleExtension, "{specifier}");
    }
}

// ── Containment ──────────────────────────────────────────────────────────────

/// `root/functions/shared/{llm.ts, sub/}` and `root/functions/notify.ts`, a function.
fn tree() -> (TempDir, std::path::PathBuf) {
    let root = TempDir::new().unwrap();
    let shared = root.path().join("functions/shared");
    std::fs::create_dir_all(shared.join("sub")).unwrap();
    std::fs::write(shared.join("llm.ts"), "export const x = 1;\n").unwrap();
    std::fs::write(root.path().join("functions/notify.ts"), "export default () => 1;\n").unwrap();
    std::fs::write(root.path().join("outside.ts"), "export const secret = 1;\n").unwrap();
    let shared = shared.canonicalize().unwrap();
    (root, shared)
}

fn key(specifier: &str) -> ModuleKey {
    resolve_specifier(specifier).unwrap()
}

#[test]
fn a_file_in_the_shared_directory_is_located() {
    let (_root, shared) = tree();
    assert_eq!(locate(&shared, &key("@shared/llm.ts")).unwrap(), shared.join("llm.ts"));
}

#[test]
fn a_missing_file_is_refused() {
    let (_root, shared) = tree();
    assert_eq!(locate(&shared, &key("@shared/nope.ts")), Err(Refusal::Missing));
}

#[test]
fn a_directory_named_like_a_module_is_refused() {
    let (_root, shared) = tree();
    std::fs::create_dir(shared.join("dir.ts")).unwrap();
    assert_eq!(locate(&shared, &key("@shared/dir.ts")), Err(Refusal::NotAFile));
}

#[cfg(unix)]
#[test]
fn a_symlinked_file_pointing_outside_is_refused() {
    let (root, shared) = tree();
    std::os::unix::fs::symlink(root.path().join("outside.ts"), shared.join("link.ts")).unwrap();
    assert_eq!(
        locate(&shared, &key("@shared/link.ts")),
        Err(Refusal::Escapes {
            target: root.path().join("outside.ts").canonicalize().unwrap(),
        })
    );
}

/// A function's own file is outside the shared directory, so a link to it escapes.
#[cfg(unix)]
#[test]
fn a_symlink_to_a_function_file_is_refused() {
    let (root, shared) = tree();
    std::os::unix::fs::symlink(root.path().join("functions/notify.ts"), shared.join("fn.ts"))
        .unwrap();
    assert!(matches!(locate(&shared, &key("@shared/fn.ts")), Err(Refusal::Escapes { .. })));
}

#[cfg(unix)]
#[test]
fn a_symlinked_directory_pointing_outside_is_refused() {
    let (root, shared) = tree();
    std::os::unix::fs::symlink(root.path(), shared.join("up")).unwrap();
    assert!(matches!(
        locate(&shared, &key("@shared/up/outside.ts")),
        Err(Refusal::Escapes { .. })
    ));
}

/// `shared-evil` starts with the same characters as `shared` but is another directory.
#[cfg(unix)]
#[test]
fn a_sibling_directory_sharing_a_name_prefix_is_outside() {
    let (root, shared) = tree();
    let evil = root.path().join("functions/shared-evil");
    std::fs::create_dir(&evil).unwrap();
    std::fs::write(evil.join("x.ts"), "export const x = 1;\n").unwrap();
    std::os::unix::fs::symlink(&evil, shared.join("e")).unwrap();
    assert!(matches!(locate(&shared, &key("@shared/e/x.ts")), Err(Refusal::Escapes { .. })));
}

#[cfg(unix)]
#[test]
fn a_symlink_staying_inside_the_shared_directory_is_located() {
    let (_root, shared) = tree();
    std::os::unix::fs::symlink(shared.join("llm.ts"), shared.join("sub/alias.ts")).unwrap();
    assert_eq!(locate(&shared, &key("@shared/sub/alias.ts")).unwrap(), shared.join("llm.ts"));
}
