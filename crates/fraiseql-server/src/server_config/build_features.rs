//! Which of this crate's features a build leaves off.
//!
//! Shared by the build script, which records the answer for the configuration schema's drift
//! test, and by this crate's tests, which pin it. The crate's features are read from its own
//! manifest, so no list of them is kept here.

/// The features declared in `manifest`'s `[features]` table, `default` aside, in order.
pub fn declared_features(manifest: &str) -> Vec<String> {
    let mut in_features = false;
    let mut out = Vec::new();
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_features = line == "[features]";
            continue;
        }
        if !in_features || line.starts_with('#') {
            continue;
        }
        let Some((key, _)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key != "default"
            && !key.is_empty()
            && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            out.push(key.to_owned());
        }
    }
    out
}

/// The declared features `enabled` does not hold, in declaration order.
pub fn missing_features(manifest: &str, enabled: impl Fn(&str) -> bool) -> Vec<String> {
    declared_features(manifest)
        .into_iter()
        .filter(|feature| !enabled(feature))
        .collect()
}
