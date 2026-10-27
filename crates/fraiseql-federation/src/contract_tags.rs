//! `@tag`s for role-gated declarations, so a gateway contract can leave them out.
//!
//! A gateway composes one supergraph from every subgraph's `_service { sdl }`, fetched
//! once and served to every client, so it lists a `requires_role` operation to callers the
//! subgraph itself would not show it to. A contract (Apollo `GraphOS`, Hive, Cosmo) derives
//! a narrower supergraph by including or excluding `@tag`ged declarations. Tagging what a
//! role gates lets an operator build a public contract without it, and an internal one
//! whose router still serves it.
//!
//! A contract matches tags by name, and accepts only letters, digits, `-`, `_` and `/`, at
//! most [`MAX_TAG_LEN`] characters. [`role_tag`] maps a role into that alphabet without
//! ever giving two roles one tag: a contract excluding one role must not exclude another.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
};

use fraiseql_error::{FraiseQLError, Result};

/// What every role tag starts with.
pub const ROLE_TAG_PREFIX: &str = "requires-role/";

/// The longest tag name a contract accepts.
pub const MAX_TAG_LEN: usize = 128;

/// The tags a subgraph's service SDL carries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractTags {
    /// Root type name (`Query`, `Mutation`) → field name → its tag.
    pub fields: BTreeMap<String, BTreeMap<String, String>>,
    /// Type name → its tags.
    pub types:  BTreeMap<String, BTreeSet<String>>,
}

impl ContractTags {
    /// No tags: the SDL of a schema no role gates.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            fields: BTreeMap::new(),
            types:  BTreeMap::new(),
        }
    }

    /// The tag on root field `field` of `root_type`, if any.
    #[must_use]
    pub fn field(&self, root_type: &str, field: &str) -> Option<&str> {
        self.fields.get(root_type)?.get(field).map(String::as_str)
    }

    /// The tags on type `name`, in order.
    pub fn of_type(&self, name: &str) -> impl Iterator<Item = &str> {
        self.types.get(name).into_iter().flatten().map(String::as_str)
    }
}

/// The tag naming `role`: [`ROLE_TAG_PREFIX`], then the role with each byte outside
/// letters, digits, `-` and `_` written as `/` and two hex digits.
///
/// `/` only ever starts an escape, so a tag reads back to exactly one role.
///
/// # Errors
///
/// [`FraiseQLError::Validation`] when the tag would be longer than [`MAX_TAG_LEN`].
pub fn role_tag(role: &str) -> Result<String> {
    let mut tag = String::from(ROLE_TAG_PREFIX);
    for byte in role.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_') {
            tag.push(char::from(byte));
        } else {
            let _ = write!(tag, "/{byte:02X}");
        }
    }
    if tag.len() > MAX_TAG_LEN {
        return Err(FraiseQLError::Validation {
            message: format!(
                "role `{role}` cannot be named by a gateway contract tag: `{tag}` is {} \
                 characters, and a contract accepts at most {MAX_TAG_LEN}",
                tag.len()
            ),
            path:    None,
        });
    }
    Ok(tag)
}

/// ` @tag(name: "…")` for each of `tags`, in order.
pub(crate) fn tag_suffix<'a>(tags: impl IntoIterator<Item = &'a str>) -> String {
    tags.into_iter().fold(String::new(), |mut suffix, tag| {
        let _ = write!(suffix, " @tag(name: \"{tag}\")");
        suffix
    })
}

#[cfg(test)]
mod tests;
