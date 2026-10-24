//! Tests for capability metadata.

use super::{CapabilityMeta, EntryPoints, Maturity, Release};

fn meta(release: Release) -> CapabilityMeta {
    CapabilityMeta {
        name: "x",
        feature: "some-feature",
        compiled_sections: &[],
        runtime_keys: &[],
        docs: None,
        maturity: Maturity::Stable,
        release,
        entry_points: EntryPoints::ALL,
    }
}

/// The remedy names the smallest published tag that has the capability, and always
/// the build flag.
#[test]
fn remedy_names_the_smallest_artifact_with_the_capability() {
    let cases = [
        (Release::Default, None),
        (Release::Full, Some("`-full` or `-platform`")),
        (Release::Platform, Some("`-platform` image")),
        (Release::Source, None),
    ];
    for (release, tag) in cases {
        let remedy = meta(release).remedy();
        assert!(remedy.contains("--features some-feature"), "{release:?}: {remedy}");
        match tag {
            Some(tag) => assert!(remedy.contains(tag), "{release:?}: {remedy}"),
            None => assert!(!remedy.contains("image tag"), "{release:?}: {remedy}"),
        }
    }
}
