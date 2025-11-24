//! Mutation result transformation module
//!
//! Transforms PostgreSQL mutation_result_v2 JSON into GraphQL responses.

#[cfg(test)]
mod tests;

/// Build complete GraphQL mutation response
///
/// This is a stub implementation for phase 1.
/// The full implementation comes in phase 3.
pub fn build_mutation_response(
    _mutation_json: &str,
    _field_name: &str,
    _success_type: &str,
    _error_type: &str,
    _entity_field_name: Option<&str>,
    _entity_type: Option<&str>,
    _cascade_selections: Option<&str>,
) -> Result<Vec<u8>, String> {
    // Stub implementation - returns empty bytes for now
    // Full implementation in phase 3
    Ok(vec![])
}

#[cfg(test)]
mod test_stub {
    use super::*;

    #[test]
    fn test_stub_function() {
        let result = build_mutation_response("", "", "", "", None, None, None);
        assert!(result.is_ok());
    }
}

/// Mutation result status classification
#[derive(Debug, Clone, PartialEq)]
pub enum MutationStatus {
    Success(String),      // "success", "new", "updated", "deleted"
    Noop(String),         // "noop:reason" - no changes made
    Error(String),        // "failed:reason" - actual error
}

impl MutationStatus {
    /// Parse status string into enum
    ///
    /// Examples:
    /// - "success" -> Success("success")
    /// - "new" -> Success("new")
    /// - "noop:unchanged" -> Noop("unchanged")
    /// - "failed:validation" -> Error("validation")
    pub fn from_str(status: &str) -> Self {
        if status.starts_with("noop:") {
            MutationStatus::Noop(status[5..].to_string())
        } else if status.starts_with("failed:") {
            MutationStatus::Error(status[7..].to_string())
        } else {
            MutationStatus::Success(status.to_string())
        }
    }

    pub fn is_success(&self) -> bool {
        matches!(self, MutationStatus::Success(_))
    }

    pub fn is_noop(&self) -> bool {
        matches!(self, MutationStatus::Noop(_))
    }

    pub fn is_error(&self) -> bool {
        matches!(self, MutationStatus::Error(_))
    }

    /// Map status to HTTP code
    pub fn http_code(&self) -> i32 {
        match self {
            MutationStatus::Success(_) => 200,
            MutationStatus::Noop(_) => 422,
            MutationStatus::Error(reason) => {
                match reason.as_str() {
                    "not_found" => 404,
                    "unauthorized" => 401,
                    "forbidden" => 403,
                    "conflict" | "duplicate" => 409,
                    "validation" | "invalid" => 422,
                    _ => 500,
                }
            }
        }
    }
}
