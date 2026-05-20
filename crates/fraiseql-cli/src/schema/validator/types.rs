//! Validation error types and the validation report.

/// Detailed validation error
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// Error message
    pub message: String,
    /// JSON path to the error (e.g., `"queries[0].return_type"`)
    pub path: String,
    /// Severity level
    pub severity: ErrorSeverity,
    /// Suggested fix
    pub suggestion: Option<String>,
}

/// Error severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorSeverity {
    /// Critical error - schema is invalid
    Error,
    /// Warning - schema is valid but may have issues
    Warning,
}

/// Validation report
#[derive(Debug, Default)]
pub struct ValidationReport {
    /// Validation errors and warnings
    pub errors: Vec<ValidationError>,
}

impl ValidationReport {
    /// Check if validation passed (no errors, warnings OK)
    pub fn is_valid(&self) -> bool {
        !self.has_errors()
    }

    /// Check if there are any errors
    pub fn has_errors(&self) -> bool {
        self.errors.iter().any(|e| e.severity == ErrorSeverity::Error)
    }

    /// Count errors
    pub fn error_count(&self) -> usize {
        self.errors.iter().filter(|e| e.severity == ErrorSeverity::Error).count()
    }

    /// Count warnings
    pub fn warning_count(&self) -> usize {
        self.errors.iter().filter(|e| e.severity == ErrorSeverity::Warning).count()
    }

    /// Print formatted report
    pub fn print(&self) {
        if self.errors.is_empty() {
            return;
        }

        println!("\nValidation Report:");

        let errors: Vec<_> =
            self.errors.iter().filter(|e| e.severity == ErrorSeverity::Error).collect();

        let warnings: Vec<_> =
            self.errors.iter().filter(|e| e.severity == ErrorSeverity::Warning).collect();

        if !errors.is_empty() {
            println!("\n  err: Errors ({}):", errors.len());
            for error in errors {
                println!("     {}", error.message);
                println!("     at: {}", error.path);
                if let Some(suggestion) = &error.suggestion {
                    println!("     hint: {suggestion}");
                }
                println!();
            }
        }

        if !warnings.is_empty() {
            println!("\n  warn: Warnings ({}):", warnings.len());
            for warning in warnings {
                println!("     {}", warning.message);
                println!("     at: {}", warning.path);
                if let Some(suggestion) = &warning.suggestion {
                    println!("     hint: {suggestion}");
                }
                println!();
            }
        }
    }
}
