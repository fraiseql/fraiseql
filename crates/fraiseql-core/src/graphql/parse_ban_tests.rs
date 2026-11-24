//! The clippy bans on a second GraphQL parse site still name the parser and the seam.
//!
//! `.clippy.toml` bans `apollo_parser::Parser` outside `document.rs` and
//! `graphql::document::parse` outside `parse_graphql_document`. A ban whose path stops
//! resolving is only a configuration warning, which `-D warnings` does not deny: rename or
//! move either and every second parse site passes the lint. This test names both by the
//! banned paths, so a rename or a move fails to compile here, and it checks the
//! configuration still bans exactly those paths.

#[test]
#[allow(clippy::disallowed_methods, clippy::disallowed_types)] // Reason: names the banned items to prove the paths resolve; parses nothing
fn the_banned_paths_name_the_parser_and_the_seam() {
    // A fn item is zero-sized: the path resolved to the function.
    assert_eq!(std::mem::size_of_val(&crate::graphql::document::parse), 0);
    assert!(std::mem::size_of::<apollo_parser::Parser>() > 0);
}

#[test]
fn the_clippy_configuration_bans_the_parser_and_the_seam() {
    let config = include_str!("../../../../.clippy.toml");
    for path in [
        "fraiseql_core::graphql::document::parse",
        "apollo_parser::Parser",
    ] {
        assert!(
            config.contains(&format!("{{ path = \"{path}\", reason = ")),
            "`.clippy.toml` no longer bans `{path}`"
        );
    }
}
