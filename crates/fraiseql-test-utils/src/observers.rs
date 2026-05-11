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
mod tests;
