//! `fraiseql run` starts the observer runtime with the tuning `fraiseql.toml` writes under
//! `[observers.runtime]`, not with the runtime's defaults.
//!
//! Each test resolves a project as `run` does (`resolve_runtime_config`, then the project
//! tuning) and reads the configuration a serve starts with (`Serving::config`).

#![allow(clippy::unwrap_used)] // Reason: test code

use std::path::Path;

use fraiseql_server::capability::REGISTRY;
use serde_json::Value;

use super::{ServerFile, Serving, project_tuning::ProjectTuning, resolve_runtime_config};

fn observers_compiled_in() -> bool {
    REGISTRY.iter().any(|entry| entry.meta.name == "observers" && entry.compiled_in)
}

const PROJECT: &str = "[project]\nname = \"tuned\"\n\n[database]\nurl = \
                       \"postgresql://u@localhost/db\"\n\n[observers]\nenabled = true\n";

/// The configuration `fraiseql run` serves `input` (in `dir`) with, under the server file
/// `server_toml` when there is one, as JSON.
fn run_config(dir: &Path, input: &str, server_toml: Option<&str>) -> anyhow::Result<Value> {
    temp_env::with_vars_unset(
        [
            "DATABASE_URL",
            "FRAISEQL_CONFIG",
            "FRAISEQL_PORT",
            "FRAISEQL_HOST",
        ],
        || {
            let input = dir.join(input);
            let server_file = server_toml
                .map(|toml| {
                    let path = dir.join("server.toml");
                    std::fs::write(&path, toml).unwrap();
                    ServerFile::load(path.to_str().unwrap())
                })
                .transpose()?;
            let (db_url, bind_addr, server_cfg, db_cfg) =
                resolve_runtime_config(&input, None, None, None, server_file.as_ref())?;
            let tuning = ProjectTuning::read(&input)?;
            let serving = Serving {
                input_path: &input,
                db_url: &db_url,
                bind_addr,
                introspection: false,
                server_cfg: &server_cfg,
                db_cfg: &db_cfg,
                server_file: server_file.as_ref(),
                admin_url: None,
                tuning: &tuning,
            };
            Ok(serde_json::to_value(serving.config()?)?)
        },
    )
}

/// `project` written as the `fraiseql.toml` of a fresh directory, served by `fraiseql run`.
fn serve_project(project: &str, server_toml: Option<&str>) -> anyhow::Result<Value> {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("fraiseql.toml"), project).unwrap();
    run_config(dir.path(), "fraiseql.toml", server_toml)
}

/// The runtime starts with the project's tuning, key for key. In a build without an
/// observer runtime, the tuning is refused, naming the feature, rather than dropped.
#[test]
fn the_projects_observer_runtime_tuning_reaches_the_server() {
    let project =
        format!("{PROJECT}\n[observers.runtime]\nchannel_capacity = 7\nbatch_size = 250\n");
    let served = serve_project(&project, None);
    if !observers_compiled_in() {
        let err = format!("{:#}", served.unwrap_err());
        assert!(err.contains("`observers` feature"), "{err}");
        return;
    }
    let runtime = served.unwrap()["observers"]["runtime"].clone();
    assert_eq!(runtime["channel_capacity"], 7, "{runtime}");
    assert_eq!(runtime["batch_size"], 250, "{runtime}");
}

/// A `schema.json` input reads the tuning of the project file the compile step reads.
#[test]
fn a_schema_json_input_carries_the_tuning_of_its_project_file() {
    if !observers_compiled_in() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("fraiseql.toml"),
        format!("{PROJECT}\n[observers.runtime]\nchannel_capacity = 7\n"),
    )
    .unwrap();
    std::fs::write(dir.path().join("schema.json"), "{}").unwrap();
    let served = run_config(dir.path(), "schema.json", None).unwrap();
    assert_eq!(served["observers"]["runtime"]["channel_capacity"], 7, "{served}");
}

/// The table is read as strictly as the server reads it: an unknown key refuses.
#[test]
fn an_unknown_key_in_the_projects_observer_runtime_is_refused() {
    if !observers_compiled_in() {
        return;
    }
    let project = format!("{PROJECT}\n[observers.runtime]\npol_interval_ms = 5\n");
    let err = format!("{:#}", serve_project(&project, None).unwrap_err());
    assert!(err.contains("[observers.runtime]") && err.contains("pol_interval_ms"), "{err}");
}

/// Set in both files, the table is refused, naming both: neither may win silently.
#[test]
fn observer_runtime_tuning_set_in_both_files_is_refused() {
    if !observers_compiled_in() {
        return;
    }
    let project = format!("{PROJECT}\n[observers.runtime]\nchannel_capacity = 7\n");
    let err = format!(
        "{:#}",
        serve_project(&project, Some("[observers.runtime]\nchannel_capacity = 9\n")).unwrap_err()
    );
    assert!(err.contains("both set the same thing"), "{err}");
    assert!(err.contains("server.toml"), "{err}");
}

/// The server file's tuning holds when the project writes none, and a project that writes
/// none leaves the server's configuration as it was.
#[test]
fn without_project_tuning_the_server_file_decides() {
    if !observers_compiled_in() {
        return;
    }
    let served =
        serve_project(PROJECT, Some("[observers.runtime]\nchannel_capacity = 9\n")).unwrap();
    assert_eq!(served["observers"]["runtime"]["channel_capacity"], 9, "{served}");
    let served = serve_project(PROJECT, None).unwrap();
    assert!(served["observers"].is_null(), "{served}");
}
