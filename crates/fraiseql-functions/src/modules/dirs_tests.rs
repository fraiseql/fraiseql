use std::path::PathBuf;

use super::{MODULE_DIR_ENV, SHARED_DIR_ENV, apply_env_overrides};
use crate::types::FunctionsConfig;

fn compiled(shared_dir: Option<&str>) -> FunctionsConfig {
    FunctionsConfig {
        module_dir:  PathBuf::from("functions"),
        shared_dir:  shared_dir.map(PathBuf::from),
        definitions: Vec::new(),
        dlq_store:   None,
    }
}

#[test]
fn the_environment_overrides_the_compiled_module_dir() {
    let mut config = compiled(None);
    apply_env_overrides(&mut config, |key| {
        (key == MODULE_DIR_ENV).then(|| "/srv/modules".to_owned())
    });
    assert_eq!(config.module_dir, PathBuf::from("/srv/modules"));
    assert_eq!(config.shared_dir, None);
}

#[test]
fn the_environment_overrides_the_compiled_shared_dir() {
    let mut config = compiled(Some("functions/shared"));
    apply_env_overrides(&mut config, |key| {
        (key == SHARED_DIR_ENV).then(|| "/srv/shared".to_owned())
    });
    assert_eq!(config.shared_dir, Some(PathBuf::from("/srv/shared")));
    assert_eq!(config.module_dir, PathBuf::from("functions"));
}

#[test]
fn the_environment_can_declare_a_shared_dir() {
    let mut config = compiled(None);
    apply_env_overrides(&mut config, |key| {
        (key == SHARED_DIR_ENV).then(|| "/srv/shared".to_owned())
    });
    assert_eq!(config.shared_dir, Some(PathBuf::from("/srv/shared")));
}

#[test]
fn the_compiled_dirs_stand_when_the_environment_is_silent() {
    for env in [None, Some(String::new()), Some("  ".to_owned())] {
        let mut config = compiled(Some("functions/shared"));
        apply_env_overrides(&mut config, |_| env.clone());
        assert_eq!(config.module_dir, PathBuf::from("functions"), "{env:?}");
        assert_eq!(config.shared_dir, Some(PathBuf::from("functions/shared")), "{env:?}");
    }
}
