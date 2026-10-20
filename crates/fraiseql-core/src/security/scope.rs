//! The one rule by which a granted scope pattern satisfies a required scope.

/// Whether the granted scope `pattern` satisfies the `required` scope.
///
/// - `*` grants every scope.
/// - An exact match grants.
/// - `read:*` grants scopes below `read:` only, never `readwrite:…`, which merely shares the string
///   prefix.
/// - A `*` after a `.` or `:` boundary (`read:User.*`) grants scopes below that prefix; a bare
///   prefix (`admin*`) grants nothing beyond its exact spelling.
///
/// `SecurityContext::has_scope`, `RoleDefinition::has_scope` and `@requirePermission` all
/// answer by it (`security::tests::scope_grant_tests`). The admin-scope check
/// (`AuthenticatedUser::has_scope`) is deliberately exact and does not use it.
#[must_use]
pub fn scope_grants(pattern: &str, required: &str) -> bool {
    if pattern == "*" || pattern == required {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix(":*") {
        return required.strip_prefix(prefix).is_some_and(|rest| rest.starts_with(':'));
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return (prefix.ends_with('.') || prefix.ends_with(':')) && required.starts_with(prefix);
    }
    false
}
