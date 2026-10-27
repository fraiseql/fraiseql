#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

/// Roles whose escapes, or whose characters, look like another's.
const LOOKALIKES: [&str; 16] = [
    "a:b", "a/3Ab", "a/3ab", "a/b", "a/2Fb", "a//b", "a b", "a-b", "a_b", "é", "/C3/A9", "", "-",
    "/", "admin", "Admin",
];

fn in_contract_alphabet(tag: &str) -> bool {
    tag.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '/'))
}

#[test]
fn a_role_already_in_the_contract_alphabet_is_named_as_it_is() {
    for role in ["admin", "super-admin", "org_admin", "Role42"] {
        assert_eq!(role_tag(role).unwrap(), format!("requires-role/{role}"));
    }
}

#[test]
fn any_other_character_is_escaped_as_its_utf8_bytes() {
    assert_eq!(role_tag("org:admin").unwrap(), "requires-role/org/3Aadmin");
    assert_eq!(role_tag("a/b").unwrap(), "requires-role/a/2Fb");
    assert_eq!(role_tag("a b").unwrap(), "requires-role/a/20b");
    assert_eq!(role_tag("é").unwrap(), "requires-role//C3/A9");
}

#[test]
fn every_tag_is_in_the_contract_alphabet() {
    for role in LOOKALIKES {
        let tag = role_tag(role).unwrap();
        assert!(tag.starts_with(ROLE_TAG_PREFIX), "{role:?} → {tag}");
        assert!(in_contract_alphabet(&tag), "{role:?} → {tag}");
    }
}

#[test]
fn no_two_roles_share_a_tag() {
    let tags: BTreeSet<String> = LOOKALIKES.iter().map(|r| role_tag(r).unwrap()).collect();
    assert_eq!(tags.len(), LOOKALIKES.len(), "{tags:?}");
}

#[test]
fn a_tag_longer_than_a_contract_accepts_is_refused() {
    let longest = "a".repeat(MAX_TAG_LEN - ROLE_TAG_PREFIX.len());
    assert_eq!(role_tag(&longest).unwrap().len(), MAX_TAG_LEN);

    let over = format!("{longest}a");
    let err = role_tag(&over).unwrap_err();
    assert!(matches!(err, FraiseQLError::Validation { .. }), "{err:?}");
    assert!(err.to_string().contains(&over), "the refusal names the role: {err}");

    // Escaping counts: one character escaped is three in the tag.
    let escaped = format!("{}:", "a".repeat(MAX_TAG_LEN - ROLE_TAG_PREFIX.len() - 2));
    assert!(role_tag(&escaped).is_err(), "{escaped}");
}

#[test]
fn a_tag_is_rendered_as_one_directive_each() {
    assert_eq!(tag_suffix([]), "");
    assert_eq!(
        tag_suffix(["requires-role/a", "requires-role/b"]),
        r#" @tag(name: "requires-role/a") @tag(name: "requires-role/b")"#
    );
}
