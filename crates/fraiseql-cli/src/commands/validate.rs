//! Schema validation command
//!
//! Validates schema.json without compilation

use anyhow::Result;

/// Run the validate command
///
/// This is just a wrapper around compile with check=true
///
/// # Arguments
///
/// * `input` - Path to schema.json file to validate
pub async fn run(input: &str) -> Result<()> {
    // Validate is just compile --check
    super::compile::run(input, "unused", true).await
}
