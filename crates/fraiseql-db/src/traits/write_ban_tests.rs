//! The clippy ban on the adapter's write still names the write.
//!
//! `.clippy.toml` bans `Writer::execute_write` and `execute_writes` outside the engine's
//! mutation chokepoint. A ban whose path stops resolving is only a configuration warning,
//! which `-D warnings` does not deny: rename or move the method and every call passes the
//! lint. This test names both methods by the banned paths, so a rename or a move fails to
//! compile here, and it checks the configuration still bans exactly those paths.

/// The paths `.clippy.toml` bans: where the methods are defined, since clippy does not
/// follow the `traits::Writer` re-export.
const BANNED: [&str; 2] = [
    "fraiseql_db::traits::mutations::Writer::execute_write",
    "fraiseql_db::traits::mutations::Writer::execute_writes",
];

#[test]
#[allow(clippy::disallowed_methods)] // Reason: names the banned methods to prove the paths resolve; calls neither
fn the_banned_paths_name_the_writers_methods() {
    // A fn item is zero-sized: both paths resolved to the trait's methods.
    assert_eq!(std::mem::size_of_val(&<dyn crate::traits::mutations::Writer>::execute_write), 0);
    assert_eq!(
        std::mem::size_of_val(&<dyn crate::traits::mutations::Writer>::execute_writes),
        0
    );
}

#[test]
fn the_clippy_configuration_bans_the_writers_methods() {
    let config = include_str!("../../../../.clippy.toml");
    for path in BANNED {
        assert!(
            config.contains(&format!("{{ path = \"{path}\", reason = ")),
            "`.clippy.toml` no longer bans `{path}`"
        );
    }
}
