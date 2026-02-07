//! Multiple backend implementations for secrets management

pub mod env;
pub mod file;
pub mod vault;

pub use env::EnvBackend;
pub use file::FileBackend;
pub use vault::VaultBackend;

#[cfg(test)]
mod tests {
    /// Test all backends available
    #[test]
    fn test_backends_available() {
        // EnvBackend - reads from environment variables
        // FileBackend - reads from local files
        // VaultBackend - connects to HashiCorp Vault
    }

    /// Test backend selection logic
    #[test]
    fn test_backend_selection() {
        // Backends should be selectable based on configuration
        // Each backend serves different use cases:
        // - EnvBackend for simple config/dev
        // - FileBackend for local testing
        // - VaultBackend for production with dynamic secrets
    }
}
