//! Observer test infrastructure helpers.

/// Returns a unique test run identifier (UUID v4 as string).
///
/// Use this to namespace test data so concurrent test runs do not interfere
/// with each other.
///
/// # Example
///
/// ```
/// use fraiseql_test_utils::observers::get_test_id;
///
/// let id = get_test_id();
/// assert!(!id.is_empty());
/// ```
#[must_use]
pub fn get_test_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_test_id_is_unique() {
        let id1 = get_test_id();
        let id2 = get_test_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_get_test_id_is_valid_uuid() {
        let id = get_test_id();
        assert!(uuid::Uuid::parse_str(&id).is_ok(), "Expected valid UUID, got: {id}");
    }
}
