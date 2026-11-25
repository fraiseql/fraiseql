#![allow(clippy::unwrap_used)] // Reason: test module

use std::path::Path;

use tempfile::TempDir;

use super::{SharedDirError, check_shared_dir};

fn tree() -> TempDir {
    let root = TempDir::new().unwrap();
    std::fs::create_dir_all(root.path().join("functions/shared")).unwrap();
    std::fs::write(root.path().join("functions/notify.ts"), "export default () => 1;\n").unwrap();
    root
}

#[test]
fn a_shared_dir_inside_module_dir_is_its_canonical_root() {
    let root = tree();
    let modules = root.path().join("functions");
    let shared = check_shared_dir(&modules, &modules.join("shared")).unwrap();
    assert_eq!(shared, modules.join("shared").canonicalize().unwrap());
}

#[test]
fn a_sibling_shared_dir_is_accepted() {
    let root = tree();
    std::fs::create_dir(root.path().join("lib")).unwrap();
    check_shared_dir(&root.path().join("functions"), &root.path().join("lib")).unwrap();
}

#[test]
fn a_missing_shared_dir_is_refused() {
    let root = tree();
    let missing = root.path().join("functions/nope");
    assert_eq!(
        check_shared_dir(&root.path().join("functions"), &missing),
        Err(SharedDirError::NotADirectory {
            shared_dir: missing,
        })
    );
}

#[test]
fn a_file_as_shared_dir_is_refused() {
    let root = tree();
    let file = root.path().join("functions/notify.ts");
    assert!(matches!(
        check_shared_dir(&root.path().join("functions"), &file),
        Err(SharedDirError::NotADirectory { .. })
    ));
}

#[test]
fn module_dir_itself_is_refused_as_shared_dir() {
    let root = tree();
    let modules = root.path().join("functions");
    assert!(matches!(
        check_shared_dir(&modules, &modules),
        Err(SharedDirError::HoldsFunctionFiles { .. })
    ));
}

/// Spelled differently, the same directory: the check compares canonical paths.
#[test]
fn module_dir_spelled_another_way_is_refused_as_shared_dir() {
    let root = tree();
    let modules = root.path().join("functions");
    assert!(matches!(
        check_shared_dir(&modules, &modules.join("shared/..")),
        Err(SharedDirError::HoldsFunctionFiles { .. })
    ));
}

#[test]
fn an_ancestor_of_module_dir_is_refused_as_shared_dir() {
    let root = tree();
    assert!(matches!(
        check_shared_dir(&root.path().join("functions"), root.path()),
        Err(SharedDirError::HoldsFunctionFiles { .. })
    ));
}

#[cfg(unix)]
#[test]
fn a_symlink_to_module_dir_is_refused_as_shared_dir() {
    let root = tree();
    let modules = root.path().join("functions");
    let link = root.path().join("link");
    std::os::unix::fs::symlink(&modules, &link).unwrap();
    assert!(matches!(
        check_shared_dir(&modules, &link),
        Err(SharedDirError::HoldsFunctionFiles { .. })
    ));
}

/// `functions-extra` is not inside `functions`: the containment check compares
/// path components, not string prefixes.
#[test]
fn a_name_prefix_is_not_containment() {
    let root = tree();
    std::fs::create_dir(root.path().join("functions-extra")).unwrap();
    std::fs::create_dir(root.path().join("func")).unwrap();
    check_shared_dir(&root.path().join("functions-extra"), &root.path().join("func")).unwrap();
}

#[test]
fn an_absent_module_dir_is_not_a_layout_error() {
    let root = tree();
    check_shared_dir(Path::new("/no/such/module/dir"), &root.path().join("functions/shared"))
        .unwrap();
}
