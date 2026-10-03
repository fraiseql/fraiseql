#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
//! Which `fraiseql.toml` a schema input compiles with (#1387).

use std::{fs, path::Path};

use tempfile::TempDir;

use super::{ConfigSource, PROJECT_CONFIG_FILE};

/// A repository with `.git` at its root, so discovery has a boundary to stop at.
fn repo() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join(".git")).unwrap();
    dir
}

fn touch(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, "").unwrap();
}

/// The issue's layout: two subgraphs, each with its own config. The schema of
/// one compiles with that subgraph's config, whatever the working directory.
#[test]
fn a_schema_compiles_with_the_config_of_its_own_subgraph() {
    let root = repo();
    let alpha = root.path().join("alpha");
    let beta = root.path().join("beta");
    touch(&alpha.join(PROJECT_CONFIG_FILE));
    touch(&beta.join(PROJECT_CONFIG_FILE));
    touch(&alpha.join("schema/schema.json"));

    let source = ConfigSource::resolve(&alpha.join("schema/schema.json"), None).unwrap();

    assert_eq!(source, ConfigSource::Discovered(alpha.join(PROJECT_CONFIG_FILE)));
}

/// The nearest config wins over one further up.
#[test]
fn the_nearest_config_wins() {
    let root = repo();
    touch(&root.path().join(PROJECT_CONFIG_FILE));
    touch(&root.path().join("sub").join(PROJECT_CONFIG_FILE));
    let schema = root.path().join("sub/schema.json");
    touch(&schema);

    let source = ConfigSource::resolve(&schema, None).unwrap();

    assert_eq!(
        source,
        ConfigSource::Discovered(root.path().join("sub").join(PROJECT_CONFIG_FILE))
    );
}

/// Discovery stops at the repository root: a config above it belongs to no one
/// in this repository.
#[test]
fn discovery_stops_at_the_repository_root() {
    let outer = tempfile::tempdir().unwrap();
    touch(&outer.path().join(PROJECT_CONFIG_FILE));
    let root = outer.path().join("repo");
    fs::create_dir_all(root.join(".git")).unwrap();
    let schema = root.join("schema.json");
    touch(&schema);

    let source = ConfigSource::resolve(&schema, None).unwrap();

    assert_eq!(
        source,
        ConfigSource::Absent {
            searched_from: root,
        }
    );
}

#[test]
fn an_explicit_config_wins_over_discovery() {
    let root = repo();
    touch(&root.path().join(PROJECT_CONFIG_FILE));
    let other = root.path().join("other.toml");
    touch(&other);
    let schema = root.path().join("schema.json");
    touch(&schema);

    let source = ConfigSource::resolve(&schema, Some(&other)).unwrap();

    assert_eq!(source, ConfigSource::Explicit(other));
}

#[test]
fn an_explicit_config_that_does_not_exist_is_refused() {
    let root = repo();
    let schema = root.path().join("schema.json");
    touch(&schema);

    let message = ConfigSource::resolve(&schema, Some(&root.path().join("missing.toml")))
        .unwrap_err()
        .to_string();

    assert!(
        message.contains("missing.toml") && message.contains("no such file"),
        "{message}"
    );
}

/// A TOML input is its own project config; a second one could only disagree.
#[test]
fn a_toml_input_is_its_own_config_and_refuses_another() {
    let root = repo();
    let input = root.path().join(PROJECT_CONFIG_FILE);
    touch(&input);

    assert_eq!(ConfigSource::resolve(&input, None).unwrap(), ConfigSource::Input(input.clone()));
    let message = ConfigSource::resolve(&input, Some(&input)).unwrap_err().to_string();
    assert!(message.contains("already the project config"), "{message}");
}

/// The line every compile prints names the file, or says plainly that none applied.
#[test]
fn the_source_describes_itself() {
    let absent = ConfigSource::Absent {
        searched_from: "/srv/app".into(),
    };
    assert!(absent.to_string().contains("defaults apply"), "{absent}");
    assert!(absent.project_config().is_none());

    let found = ConfigSource::Discovered("/srv/app/fraiseql.toml".into());
    assert_eq!(found.project_config(), Some(Path::new("/srv/app/fraiseql.toml")));
}
