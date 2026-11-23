//! Server tuning `fraiseql run` reads from the project file, beyond `[server]` and
//! `[database]`.
//!
//! `fraiseql compile` accepts `[observers.runtime]` in `fraiseql.toml` without reading it,
//! so one file can tune the server as well. `fraiseql run` serves from that file, so it
//! carries the sub-table into the server's configuration rather than starting the runtime
//! on its defaults. It is the one server-owned table the compiler accepts without reading:
//! every other section of `fraiseql.toml` refuses a key it does not declare.

use std::path::Path;

use anyhow::{Context, Result};
use fraiseql_core::utils::config_diagnostic::{ConfigDiagnostic, DiagnosticCode};
use fraiseql_server::ServerConfig;

use super::ServerFile;
use crate::config::{ConfigSource, TomlSchema};

/// The server key a server file sets the observer runtime's tuning under.
pub(super) const OBSERVER_RUNTIME_KEY: &str = "observers.runtime";

/// The tuning the project file carries for the server.
#[derive(Debug, Default)]
pub(super) struct ProjectTuning {
    /// The project file, when one applies.
    file:             Option<String>,
    /// Its `[observers.runtime]` sub-table, as written.
    observer_runtime: Option<toml::Value>,
}

impl ProjectTuning {
    /// Read the tuning from the project file `input_path` names: the input itself when it
    /// is a `.toml`, else the project config the compile step reads.
    ///
    /// # Errors
    ///
    /// Returns an error if the project file cannot be loaded.
    pub(super) fn read(input_path: &Path) -> Result<Self> {
        let (path, schema) = if input_path.extension().and_then(|e| e.to_str()) == Some("toml") {
            (
                input_path.to_path_buf(),
                TomlSchema::from_file(input_path.to_str().unwrap_or("")),
            )
        } else {
            let source = ConfigSource::resolve(input_path, None)?;
            let Some(path) = source.project_config() else {
                return Ok(Self::default());
            };
            (
                path.to_path_buf(),
                TomlSchema::from_project_file(path.to_str().unwrap_or_default()),
            )
        };
        let schema = schema
            .with_context(|| format!("Failed to load runtime config from {}", path.display()))?;
        Ok(Self {
            file:             Some(path.display().to_string()),
            observer_runtime: schema.observers.runtime,
        })
    }

    /// Apply the tuning to `config`. A table the server file (`server_file`) sets too is
    /// refused, as `E-CONFIG-SET-TWICE`: neither file may win silently.
    ///
    /// # Errors
    ///
    /// Returns an error naming the table when it is set in both files, when the server
    /// refuses its contents, or when this build has no observer runtime to tune.
    pub(super) fn apply(
        &self,
        config: &mut ServerConfig,
        server_file: Option<&ServerFile>,
    ) -> Result<()> {
        let Some(runtime) = &self.observer_runtime else {
            return Ok(());
        };
        if let Some(file) = server_file.filter(|file| file.sets(OBSERVER_RUNTIME_KEY)) {
            let message = format!(
                "`[observers.runtime]` in fraiseql.toml and `[observers.runtime]` in {} \
                 (FRAISEQL_CONFIG) both set the same thing: set it in one of them",
                file.path
            );
            let mut diagnostic =
                ConfigDiagnostic::new(DiagnosticCode::SetTwice, "observers", "runtime", message)
                    .with_also_in(&file.path, OBSERVER_RUNTIME_KEY);
            diagnostic.file.clone_from(&self.file);
            return Err(diagnostic.into());
        }
        config.apply_project_observer_runtime(runtime).map_err(anyhow::Error::msg)
    }
}
