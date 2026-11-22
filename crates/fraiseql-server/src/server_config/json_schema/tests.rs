#![allow(clippy::unwrap_used)] // Reason: test code

use super::{SERVER_CONFIG_JSON_SCHEMA_PATH, server_config_json_schema};
use crate::server_config::build_features::{declared_features, missing_features};

/// Set by the CI leg that runs the drift test with every feature on. Where it is set, a build
/// that leaves a feature off fails the test instead of skipping it: that leg is the one place
/// the comparison is made.
const REQUIRE_ALL_FEATURES_ENV: &str = "FRAISEQL_TEST_REQUIRE_ALL_FEATURES";

/// What the drift test does in a build.
#[derive(Debug, PartialEq, Eq)]
enum Comparison {
    /// Every feature is on: the generated schema is the committed one's source.
    Compare,
    /// A feature is off, so this build generates a narrower schema; the reason names them.
    Skip(String),
}

/// The drift test's branch, from the features the build leaves off (comma-separated, as the
/// build script records them) and whether the run requires every feature.
fn comparison(missing: &str, required: bool) -> Comparison {
    if missing.is_empty() {
        return Comparison::Compare;
    }
    assert!(
        !required,
        "{REQUIRE_ALL_FEATURES_ENV} is set but this build leaves off {missing}: the run that \
         compares the committed configuration schema must build every feature"
    );
    Comparison::Skip(format!(
        "this build leaves off {missing}, so it generates a narrower schema than the \
         committed one, which every feature generates"
    ))
}

/// The committed schema is the one a build with every feature generates.
/// `make bless-server-config-schema` rewrites it. A narrower build lists it as ignored.
#[test]
#[cfg_attr(
    not(fraiseql_server_all_features),
    ignore = "the committed schema is the one a build with every feature generates"
)]
fn the_committed_schema_is_the_generated_one() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(SERVER_CONFIG_JSON_SCHEMA_PATH);
    let generated = serde_json::to_string_pretty(&server_config_json_schema()).unwrap() + "\n";
    if std::env::var_os("FRAISEQL_BLESS_SERVER_CONFIG_SCHEMA").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &generated).unwrap();
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        committed == generated,
        "{SERVER_CONFIG_JSON_SCHEMA_PATH} is not the schema the server generates; run \
         `make bless-server-config-schema`"
    );
}

/// The run that compares the committed schema builds every feature, so the drift test above
/// is not listed as ignored there. It runs in every build: where the run requires every
/// feature and this build leaves one off, it fails.
#[test]
fn the_run_that_compares_the_schema_builds_every_feature() {
    let required = std::env::var_os(REQUIRE_ALL_FEATURES_ENV).is_some();
    let missing = env!("FRAISEQL_SERVER_MISSING_FEATURES");
    let compares = comparison(missing, required) == Comparison::Compare;
    assert_eq!(
        compares,
        cfg!(fraiseql_server_all_features),
        "the build script's cfg and its list of features left off disagree: {missing:?}"
    );
}

#[test]
fn a_build_with_every_feature_compares() {
    assert_eq!(comparison("", false), Comparison::Compare);
    assert_eq!(comparison("", true), Comparison::Compare);
}

#[test]
fn a_narrower_build_skips_and_names_what_it_leaves_off() {
    let outcome = comparison("grpc,aws-s3", false);
    assert!(
        matches!(&outcome, Comparison::Skip(why) if why.contains("grpc,aws-s3")),
        "a build without grpc and aws-s3 must skip, naming them: {outcome:?}"
    );
}

#[test]
#[should_panic(
    expected = "FRAISEQL_TEST_REQUIRE_ALL_FEATURES is set but this build leaves off grpc"
)]
fn a_narrower_build_fails_where_the_run_requires_every_feature() {
    let _ = comparison("grpc", true);
}

/// The schema names itself by this release's URL.
#[test]
fn the_schema_is_identified_by_this_releases_url() {
    assert_eq!(
        server_config_json_schema()["$id"],
        format!(
            "https://raw.githubusercontent.com/fraiseql/fraiseql/v{}/docs/schemas/fraiseql-server.schema.json",
            env!("CARGO_PKG_VERSION")
        )
    );
}

// ── the features a build leaves off ───────────────────────────────────────────────

const MANIFEST: &str = r#"[package]
name = "demo"

[features]
# a comment = ["x"]
default = ["auth"]
auth = []
grpc = [
  "dep:tonic",
  "auth",
]
aws-s3 = ["dep:aws"]

[package.metadata.docs.rs]
all-features = true
"#;

#[test]
fn the_declared_features_are_the_features_tables_keys() {
    assert_eq!(declared_features(MANIFEST), ["auth", "grpc", "aws-s3"]);
}

#[test]
fn a_build_leaves_off_the_features_it_does_not_enable() {
    assert_eq!(missing_features(MANIFEST, |f| f == "auth"), ["grpc", "aws-s3"]);
    assert!(missing_features(MANIFEST, |_| true).is_empty());
}

/// Read from this crate's real manifest: its features, and none of the keys of the tables
/// around `[features]`.
#[test]
fn this_crates_manifest_declares_its_features() {
    let manifest = include_str!("../../../Cargo.toml");
    let declared = declared_features(manifest);
    for feature in ["auth", "grpc", "functions-runtime-deno", "cdc-kafka"] {
        assert!(declared.iter().any(|f| f == feature), "{feature} missing from {declared:?}");
    }
    for key in ["default", "name", "categories", "exclude"] {
        assert!(!declared.iter().any(|f| f == key), "{key} read as a feature: {declared:?}");
    }
}
