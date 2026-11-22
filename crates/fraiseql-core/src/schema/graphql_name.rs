//! The GraphQL Name production (§ 2.1.9), the one predicate every name check asks.

/// Whether `name` is a GraphQL Name: `/[_A-Za-z][_0-9A-Za-z]*/` (§ 2.1.9).
///
/// This is the lexical rule only. A declared name must also not start with `__`, which
/// introspection reserves (§ 3.1), and an enum value must not be `true`, `false` or
/// `null` (§ 3.9); the load-time name check applies both.
///
/// ```
/// use fraiseql_core::schema::is_graphql_name;
///
/// assert!(is_graphql_name("_orderTotal2"));
/// assert!(!is_graphql_name("2fa"));
/// assert!(!is_graphql_name("a'b"));
/// assert!(!is_graphql_name(""));
/// ```
#[must_use]
pub fn is_graphql_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}
