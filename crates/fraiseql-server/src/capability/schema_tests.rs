//! Every section and key a compiled-in registration lists has a schema, of the type serde
//! reads it into.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // Reason: test code, panics are acceptable

use super::{REGISTRY, schema};

/// The lookup is by name, with a wildcard for a key this build cannot describe, so a key
/// added to a registration without an arm reads as "not described" rather than failing to
/// compile. This is what refuses it, for every build this test runs in.
#[test]
fn every_listed_section_of_a_compiled_in_registration_has_a_schema() {
    for r in REGISTRY {
        for section in r.meta.compiled_sections {
            assert!(
                schema::of_compiled(section.key).is_some(),
                "{}: compiled section `{}` has no schema",
                r.meta.name,
                section.key
            );
        }
        if !r.compiled_in {
            continue;
        }
        for key in r.meta.runtime_keys {
            assert!(
                schema::of_runtime_key(key).is_some(),
                "{}: server-config key `{key}` has no schema",
                r.meta.name
            );
        }
    }
}

/// `sources` names a compiled section (the definitions) and a server-config key (the
/// scheduler's switch). Looked up in one table, the key was described as the definitions.
#[cfg(feature = "sources")]
#[test]
fn the_sources_key_is_the_scheduler_config_not_the_definitions() {
    let runtime = schema::of_runtime_key("sources").unwrap();
    let compiled = schema::of_compiled("sources").unwrap();
    assert_eq!(runtime["type"], "object", "{runtime}");
    assert!(runtime["properties"]["allowed_env_vars"].is_object(), "{runtime}");
    assert_eq!(compiled["type"], "array", "{compiled}");
}

/// A key the boot refuses by name is held only to name where it went: it is not offered.
#[cfg(feature = "sources")]
#[test]
fn a_refused_key_is_not_in_the_schema() {
    let runtime = schema::of_runtime_key("sources").unwrap();
    assert!(runtime["properties"].get("allowed_domains").is_none(), "{runtime}");
}
