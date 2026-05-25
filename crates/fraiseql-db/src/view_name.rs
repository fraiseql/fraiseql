//! [`ViewName`] — a typed identifier for SQL views and tables.
//!
//! Wraps an `Arc<str>` so cloning a name across cache index entries, cache
//! reverse indexes, and `Box<[ViewName]>` storage is a single atomic
//! reference-count bump instead of a heap allocation.
//!
//! ## Why a newtype
//!
//! View names flow through cache invalidation, SQL generation, and observer
//! triggers. Before this newtype existed they were passed as bare `&str`,
//! `String`, `&String`, `Vec<String>`, and `Box<[String]>` interchangeably.
//! Mixing a *view name* with an *arbitrary identifier* at one of those
//! boundaries was a silent class-of-bug: a typo or a misordered argument
//! compiled and ran without complaint.
//!
//! Wrapping the value in `ViewName(Arc<str>)` lets the type system enforce
//! the distinction at API boundaries while keeping look-ups ergonomic via
//! [`Borrow<str>`] and [`Deref<Target = str>`].
//!
//! ## Serialization
//!
//! `ViewName` is `#[serde(transparent)]` — the wire form is identical to the
//! raw string, so it can drop into any existing JSON/`bincode` payload that
//! previously held a `String`.

use std::{borrow::Borrow, fmt, ops::Deref, sync::Arc};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Typed name of a SQL view or table.
///
/// Backed by `Arc<str>` so cloning is a single atomic reference-count bump,
/// not a heap allocation. The type is intentionally distinct from `String`
/// and `&str` so callers cannot pass an arbitrary identifier where a view
/// name is required.
///
/// # Construction
///
/// ```rust
/// use fraiseql_db::ViewName;
///
/// let from_str: ViewName = "v_user".into();
/// let from_string: ViewName = String::from("v_user").into();
/// assert_eq!(from_str, from_string);
/// ```
///
/// # Look-up by `&str`
///
/// [`ViewName`] implements [`Borrow<str>`] so it can be used as a
/// `HashMap`/`DashMap` key looked up via `&str`:
///
/// ```rust
/// use std::collections::HashMap;
/// use fraiseql_db::ViewName;
///
/// let mut m: HashMap<ViewName, u32> = HashMap::new();
/// m.insert("v_user".into(), 1);
/// assert_eq!(m.get("v_user"), Some(&1));
/// ```
#[derive(Clone, Debug, Eq, Hash, PartialEq, Ord, PartialOrd)]
pub struct ViewName(Arc<str>);

// Reason: hand-written serde impls keep the wire form identical to the raw
//         string (matching `#[serde(transparent)]`) without forcing the
//         workspace `serde` declaration to enable the `rc` feature flag.
impl Serialize for ViewName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ViewName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Self::from(s))
    }
}

impl ViewName {
    /// Returns the view name as `&str` without allocating.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns a cheap clone of the underlying `Arc<str>` for callers that
    /// need to thread the name through long-lived structures without bumping
    /// the `ViewName` wrapper.
    #[must_use]
    pub fn as_arc(&self) -> Arc<str> {
        Arc::clone(&self.0)
    }
}

impl fmt::Display for ViewName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Deref for ViewName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<str> for ViewName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for ViewName {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ViewName {
    fn from(value: &str) -> Self {
        Self(Arc::from(value))
    }
}

impl From<String> for ViewName {
    fn from(value: String) -> Self {
        Self(Arc::from(value.into_boxed_str()))
    }
}

impl From<&String> for ViewName {
    fn from(value: &String) -> Self {
        Self(Arc::from(value.as_str()))
    }
}

impl From<Arc<str>> for ViewName {
    fn from(value: Arc<str>) -> Self {
        Self(value)
    }
}

impl From<ViewName> for String {
    fn from(value: ViewName) -> Self {
        value.0.as_ref().to_owned()
    }
}

impl PartialEq<str> for ViewName {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for ViewName {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<String> for ViewName {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other.as_str()
    }
}

#[cfg(test)]
mod tests;
