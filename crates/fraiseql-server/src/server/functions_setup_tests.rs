//! `FRAISEQL_FUNCTIONS_MODULE_DIR` over the compiled `module_dir`.

use std::path::PathBuf;

use super::resolve_module_dir;

#[test]
fn the_environment_overrides_the_compiled_module_dir() {
    let resolved = resolve_module_dir(PathBuf::from("functions"), |key| {
        (key == "FRAISEQL_FUNCTIONS_MODULE_DIR").then(|| "/srv/modules".to_owned())
    });
    assert_eq!(resolved, PathBuf::from("/srv/modules"));
}

#[test]
fn the_compiled_module_dir_stands_when_the_environment_is_silent() {
    for env in [None, Some(String::new()), Some("  ".to_owned())] {
        let resolved = resolve_module_dir(PathBuf::from("functions"), |_| env.clone());
        assert_eq!(resolved, PathBuf::from("functions"), "{env:?}");
    }
}
