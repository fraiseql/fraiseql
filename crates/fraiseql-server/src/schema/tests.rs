mod loader_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
    #![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
    #![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
    #![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code
    #![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site

    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::super::loader::*;

    #[tokio::test]
    async fn test_loader_not_found() {
        let loader = CompiledSchemaLoader::new("/nonexistent/path/schema.json");
        let result = loader.load().await;
        assert!(matches!(result, Err(SchemaLoadError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_loader_invalid_json() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "{{invalid json").unwrap();
        file.flush().unwrap();

        let loader = CompiledSchemaLoader::new(file.path());
        let result = loader.load().await;
        assert!(matches!(result, Err(SchemaLoadError::ParseError(_))));
    }
}
