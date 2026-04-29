//! Builder for assembling server subsystems with cross-subsystem validation.

use super::{FunctionsSubsystem, RealtimeSubsystem, ServerSubsystems, StorageSubsystem};

/// Error returned when [`ServerSubsystemsBuilder::build`] detects a configuration problem.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SubsystemBuildError {
    /// A required dependency between subsystems is not satisfied.
    ///
    /// For example: functions define `after:storage` triggers but no storage subsystem
    /// was registered with the builder.
    #[error("{dependant} requires {dependency}: {reason}")]
    MissingDependency {
        /// The subsystem that has an unmet dependency.
        dependant: &'static str,
        /// The subsystem that is missing.
        dependency: &'static str,
        /// Human-readable explanation.
        reason: String,
    },
}

/// Builder for [`ServerSubsystems`].
///
/// Use the fluent API to register each optional subsystem, then call
/// [`build`][Self::build] to validate cross-subsystem dependencies and
/// produce the final [`ServerSubsystems`].
///
/// # Example
///
/// ```rust,ignore
/// let subsystems = ServerSubsystemsBuilder::new()
///     .with_storage(storage_subsystem)
///     .with_functions(functions_subsystem)
///     .with_realtime(realtime_subsystem)
///     .build()?;
/// ```
#[derive(Default)]
pub struct ServerSubsystemsBuilder {
    storage: Option<StorageSubsystem>,
    functions: Option<FunctionsSubsystem>,
    realtime: Option<RealtimeSubsystem>,
}

impl ServerSubsystemsBuilder {
    /// Create a new builder with no subsystems registered.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register the storage subsystem.
    #[must_use]
    pub fn with_storage(mut self, subsystem: StorageSubsystem) -> Self {
        self.storage = Some(subsystem);
        self
    }

    /// Register the functions subsystem.
    #[must_use]
    pub fn with_functions(mut self, subsystem: FunctionsSubsystem) -> Self {
        self.functions = Some(subsystem);
        self
    }

    /// Register the realtime subsystem.
    #[must_use]
    pub fn with_realtime(mut self, subsystem: RealtimeSubsystem) -> Self {
        self.realtime = Some(subsystem);
        self
    }

    /// Validate cross-subsystem dependencies and build [`ServerSubsystems`].
    ///
    /// # Errors
    ///
    /// Returns [`SubsystemBuildError::MissingDependency`] if the functions subsystem
    /// contains `after:storage` triggers but no storage subsystem has been registered.
    pub fn build(self) -> Result<ServerSubsystems, SubsystemBuildError> {
        self.validate()?;
        Ok(ServerSubsystems {
            storage: self.storage,
            functions: self.functions,
            realtime: self.realtime,
        })
    }

    /// Check cross-subsystem dependency constraints.
    fn validate(&self) -> Result<(), SubsystemBuildError> {
        // If functions define after:storage triggers, the storage subsystem must be present.
        if let Some(functions) = &self.functions {
            let has_storage_triggers = functions.config.definitions.iter().any(|d| {
                d.trigger.starts_with("after:storage:")
            });

            if has_storage_triggers && self.storage.is_none() {
                return Err(SubsystemBuildError::MissingDependency {
                    dependant: "functions",
                    dependency: "storage",
                    reason: "one or more function definitions use after:storage triggers but \
                             no storage subsystem is configured; either add a [storage] section \
                             to the compiled schema or remove the after:storage triggers"
                        .to_string(),
                });
            }
        }
        Ok(())
    }
}
