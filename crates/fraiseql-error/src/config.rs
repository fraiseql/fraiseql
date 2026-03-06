use std::path::PathBuf;

/// Errors that occur while loading or validating FraiseQL configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// No configuration file was found at the expected location(s).
    #[error("Configuration file not found")]
    NotFound,

    /// The configuration file was found but could not be read from disk.
    #[error("Failed to read configuration file {path}: {source}")]
    ReadError {
        /// Path to the file that could not be read.
        path:   PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },

    /// The configuration file was read successfully but contains invalid TOML
    /// or does not conform to the expected schema.
    #[error("Failed to parse configuration: {source}")]
    ParseError {
        /// The TOML deserialisation error.
        #[from]
        source: toml::de::Error,
    },

    /// A configuration value failed a semantic validation rule (e.g. a port
    /// number that is out of range, or conflicting options).
    #[error("Validation error in {field}: {message}")]
    ValidationError {
        /// Dot-separated path to the invalid configuration field.
        field:   String,
        /// Human-readable description of why the value is invalid.
        message: String,
    },

    /// A required environment variable was not set at startup.
    #[error("Missing required environment variable: {name}")]
    MissingEnvVar {
        /// Name of the missing environment variable.
        name: String,
    },

    /// Several configuration errors were collected together (e.g. during a
    /// full-file validation pass) and are reported as a single error.
    #[error("Multiple configuration errors")]
    MultipleErrors {
        /// All individual errors that were encountered.
        errors: Vec<ConfigError>,
    },
}

impl ConfigError {
    /// Returns a short, stable error code string suitable for API responses and
    /// structured logging.
    pub const fn error_code(&self) -> &'static str {
        match self {
            Self::NotFound => "config_not_found",
            Self::ReadError { .. } => "config_read_error",
            Self::ParseError { .. } => "config_parse_error",
            Self::ValidationError { .. } => "config_validation_error",
            Self::MissingEnvVar { .. } => "config_missing_env",
            Self::MultipleErrors { .. } => "config_multiple_errors",
        }
    }
}
