//! Building an RLS policy outside its module is refused, and the policies' wire forms.
//!
//! `.clippy.toml` bans the three concrete policies outside `rls_policy.rs`: a transport
//! consults the policy the deployment configured and never builds its own. The ban sees a
//! path that names a type, so every construction must name one: `NoRLSPolicy` is not a
//! unit value and `DefaultRLSPolicy` cannot be written as a literal outside its module.
//! A ban whose path stops resolving is only a configuration warning, so these tests also
//! name each type by the banned path and check the configuration still bans it. The path is
//! where the type is defined, `security::rls_policy`: clippy does not follow the `security`
//! re-export.

#![allow(clippy::unwrap_used)] // Reason: test code

const BANNED: [&str; 3] = [
    "fraiseql_core::security::rls_policy::DefaultRLSPolicy",
    "fraiseql_core::security::rls_policy::NoRLSPolicy",
    "fraiseql_core::security::rls_policy::CompiledRLSPolicy",
];

#[test]
#[allow(clippy::disallowed_types)] // Reason: names the banned types to prove the paths resolve
fn the_banned_paths_name_the_three_policies() {
    assert!(std::mem::size_of::<crate::security::rls_policy::DefaultRLSPolicy>() > 0);
    assert_eq!(std::mem::size_of::<crate::security::rls_policy::NoRLSPolicy>(), 0);
    assert!(std::mem::size_of::<crate::security::rls_policy::CompiledRLSPolicy>() > 0);
}

#[test]
fn the_clippy_configuration_bans_the_three_policies() {
    let config = include_str!("../../../../.clippy.toml");
    for path in BANNED {
        assert!(
            config.contains(&format!("{{ path = \"{path}\", reason = ")),
            "`.clippy.toml` no longer bans `{path}`"
        );
    }
}

/// The field that keeps a literal out of other modules is not on the wire.
#[test]
#[allow(clippy::disallowed_types)] // Reason: the test reads the policy's own serde form
fn a_default_policys_wire_form_is_unchanged() {
    let before =
        r#"{"enable_tenant_isolation":true,"tenant_field":"tenant_id","owner_field":"author_id"}"#;
    assert_eq!(
        serde_json::to_string(&crate::security::DefaultRLSPolicy::new()).unwrap(),
        before
    );
    let read: crate::security::DefaultRLSPolicy = serde_json::from_str(before).unwrap();
    assert_eq!(serde_json::to_string(&read).unwrap(), before);
}

/// `NoRLSPolicy` was a unit struct, written `null`; it is an empty struct, written `{}`.
#[test]
#[allow(clippy::disallowed_types)] // Reason: the test reads the policy's own serde form
fn a_no_op_policy_is_written_as_an_empty_object() {
    assert_eq!(serde_json::to_string(&crate::security::NoRLSPolicy::new()).unwrap(), "{}");
    let _: crate::security::NoRLSPolicy = serde_json::from_str("{}").unwrap();
}

/// The private field is what refuses a `DefaultRLSPolicy` literal outside `rls_policy.rs`.
/// It is not on the wire (above), so its derived `Debug` is where a test can see it.
#[test]
#[allow(clippy::disallowed_types)] // Reason: the test reads the policy's own fields
fn a_default_policy_carries_the_field_that_refuses_a_literal() {
    let shown = format!("{:?}", crate::security::DefaultRLSPolicy::new());
    assert!(shown.contains("_private"), "{shown}");
}
