//! The clippy bans on `tracing::subscriber::{with_default, set_default}` still name them.
//!
//! A ban whose path stops resolving is only a configuration warning, which `-D warnings`
//! does not deny, so a `tracing` release that moved either function would lift its ban in
//! silence. This test names both by the banned paths, so a move fails to compile here, and
//! checks the configuration still bans them.

/// The paths `.clippy.toml` bans.
const BANNED: [&str; 2] = [
    "tracing::subscriber::with_default",
    "tracing::subscriber::set_default",
];

#[test]
#[allow(clippy::disallowed_methods)] // Reason: names the banned functions to prove the paths resolve; neither closure runs
fn the_banned_paths_name_the_subscriber_functions() {
    let scoped = || {
        tracing::subscriber::with_default(tracing::subscriber::NoSubscriber::default(), || ());
    };
    let guard = || tracing::subscriber::set_default(tracing::subscriber::NoSubscriber::default());
    let _ = (scoped, guard);
}

#[test]
fn the_clippy_configuration_bans_the_subscriber_functions() {
    let config = include_str!("../../../.clippy.toml");
    for path in BANNED {
        assert!(
            config.contains(&format!("{{ path = \"{path}\", reason = ")),
            "`.clippy.toml` no longer bans `{path}`"
        );
    }
}
