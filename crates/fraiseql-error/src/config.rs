use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Configuration file not found")]
    NotFound,

    #[error("Failed to read configuration file {path}: {source}")]
    ReadError {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to parse configuration: {source}")]
    ParseError {
        #[from]
        source: toml::de::Error,
    },

    #[error("Validation error in {field}: {message}")]
    ValidationError {
        field: String,
        message: String,
    },

    #[error("Missing required environment variable: {name}")]
    MissingEnvVar { name: String },

    #[error("Multiple configuration errors")]
    MultipleErrors { errors: Vec<ConfigError> },
}

impl ConfigError {
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
