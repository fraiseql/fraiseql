//! Server tuning a project file (`fraiseql.toml`) carries for `fraiseql run`.
//!
//! The compiler accepts `[observers.runtime]` in `fraiseql.toml` without reading it, so one
//! file can tune the server too. `fraiseql run` builds its server from that file, and hands
//! the sub-table here, where it is read as the server reads it from its own file.

use super::ServerConfig;

impl ServerConfig {
    /// Apply `fraiseql.toml`'s `[observers.runtime]` sub-table, `runtime`, as the observer
    /// runtime's tuning: the same strict reading the server gives the table in its own file,
    /// where an unknown key refuses.
    ///
    /// # Errors
    ///
    /// Returns a message naming the table when a key is unknown or a value has the wrong
    /// type, or, in a build without the `observers` feature, naming the feature: there is no
    /// runtime to tune, and the tuning is refused rather than dropped.
    pub fn apply_project_observer_runtime(&mut self, runtime: &toml::Value) -> Result<(), String> {
        #[cfg(feature = "observers")]
        {
            let settings: super::ObserverRuntimeSettings = runtime
                .clone()
                .try_into()
                .map_err(|e| format!("fraiseql.toml [observers.runtime]: {e}"))?;
            self.observers.get_or_insert_with(Default::default).runtime = settings;
            Ok(())
        }
        #[cfg(not(feature = "observers"))]
        {
            let _ = runtime;
            Err(
                "fraiseql.toml [observers.runtime] tunes the observer runtime, which requires a \
                 binary built with the `observers` feature; this build compiled it out. Rebuild \
                 with `--features observers` or remove the table."
                    .to_string(),
            )
        }
    }
}
