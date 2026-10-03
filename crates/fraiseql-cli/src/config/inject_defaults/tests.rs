#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
//! `[inject_defaults]` in `fraiseql.toml`: the SDKs' shape is accepted (#1384), a misspelled
//! section is refused, and a copy in the schema document must agree.

use super::InjectDefaultsToml;
use crate::{config::TomlProjectConfig, schema::intermediate::IntermediateInjectDefaults};

/// The shape every SDK config loader documents, verbatim from the Python SDK.
const SDK_SHAPE: &str = r#"
[inject_defaults]
tenant_id = "jwt:tenant_id"

[inject_defaults.queries]
read_scope = "jwt:scope"

[inject_defaults.mutations]
user_id = "jwt:sub"
"#;

/// #1384: the project config the SDKs tell users to write is accepted by the compiler.
#[test]
fn the_sdk_documented_section_is_accepted_in_the_project_config() {
    let config: TomlProjectConfig = toml::from_str(SDK_SHAPE).unwrap();
    let defaults = config.inject_defaults.unwrap().to_intermediate().unwrap();

    assert_eq!(defaults.base.get("tenant_id").map(String::as_str), Some("jwt:tenant_id"));
    assert_eq!(defaults.queries.get("read_scope").map(String::as_str), Some("jwt:scope"));
    assert_eq!(defaults.mutations.get("user_id").map(String::as_str), Some("jwt:sub"));
}

/// The SDK loaders silently ignore a sub-table they do not know; the compiler refuses it,
/// because a misspelled `[inject_defaults.querys]` would otherwise leave the defaults off.
#[test]
fn a_misspelled_sub_table_is_refused() {
    let toml = "[inject_defaults.querys]\nread_scope = \"jwt:scope\"\n";
    assert!(toml::from_str::<TomlProjectConfig>(toml).is_err());
}

#[test]
fn a_source_without_a_prefix_is_refused() {
    let config: InjectDefaultsToml = toml::from_str("tenant_id = \"tenant_id\"\n").unwrap();
    let message = format!("{:#}", config.to_intermediate().unwrap_err());
    assert!(message.contains("inject_defaults"), "{message}");
}

fn document(base: &[(&str, &str)]) -> IntermediateInjectDefaults {
    IntermediateInjectDefaults {
        base: base.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect(),
        ..Default::default()
    }
}

#[test]
fn either_source_alone_is_used_as_is() {
    let config: InjectDefaultsToml = toml::from_str("tenant_id = \"jwt:tenant_id\"\n").unwrap();
    let from_config = InjectDefaultsToml::reconcile(Some(&config), None).unwrap().unwrap();
    assert_eq!(from_config, document(&[("tenant_id", "jwt:tenant_id")]));

    let from_document =
        InjectDefaultsToml::reconcile(None, Some(document(&[("org_id", "jwt:org_id")]))).unwrap();
    assert_eq!(from_document, Some(document(&[("org_id", "jwt:org_id")])));
}

/// An SDK emits the document's block from the same config, so equal is the normal case.
#[test]
fn an_equal_copy_in_the_schema_document_is_accepted() {
    let config: InjectDefaultsToml = toml::from_str("tenant_id = \"jwt:tenant_id\"\n").unwrap();
    let defaults = InjectDefaultsToml::reconcile(
        Some(&config),
        Some(document(&[("tenant_id", "jwt:tenant_id")])),
    )
    .unwrap();
    assert_eq!(defaults, Some(document(&[("tenant_id", "jwt:tenant_id")])));
}

/// A difference means one copy is stale; neither may silently win.
#[test]
fn a_differing_copy_in_the_schema_document_is_refused() {
    let config: InjectDefaultsToml = toml::from_str("tenant_id = \"jwt:tenant_id\"\n").unwrap();
    let message = InjectDefaultsToml::reconcile(
        Some(&config),
        Some(document(&[("tenant_id", "jwt:org_id")])),
    )
    .unwrap_err()
    .to_string();
    assert!(message.contains("differs"), "{message}");
}
