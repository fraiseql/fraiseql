//! Domain-specific string newtypes for GraphQL schema identifiers.
//!
//! Using distinct types for type names, field names, SQL sources, roles, and scopes
//! makes it a compile-time error to pass a [`FieldName`] where a [`TypeName`] is expected.
//!
//! All newtypes are:
//! - `serde(transparent)` — JSON round-trips as a plain string
//! - `Display` — usable directly in format strings
//! - `AsRef<str>` / `as_str()` — cheap reference conversion
//! - `From<String>` / `From<&str>` — ergonomic construction
//! - `PartialEq<str>` — compare against string literals without `.as_str()`
//!
//! # Example
//!
//! ```
//! use fraiseql_core::schema::domain_types::{TypeName, FieldName};
//!
//! let t: TypeName = "User".into();
//! let f: FieldName = "email".into();
//!
//! // Does not compile — type mismatch caught at compile time:
//! // let bad: TypeName = f;  // error[E0308]: mismatched types
//!
//! assert_eq!(t, "User");
//! assert_eq!(f, "email");
//! ```

macro_rules! string_newtype {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Construct from any `Into<String>`.
            pub fn new(s: impl Into<String>) -> Self {
                Self(s.into())
            }

            /// Return a reference to the underlying string slice.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Returns `true` if the inner string is empty.
            #[must_use]
            pub const fn is_empty(&self) -> bool {
                self.0.is_empty()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_owned())
            }
        }

        impl PartialEq<str> for $name {
            fn eq(&self, other: &str) -> bool {
                self.0 == other
            }
        }

        impl PartialEq<&str> for $name {
            fn eq(&self, other: &&str) -> bool {
                self.0 == *other
            }
        }

        impl PartialEq<String> for $name {
            fn eq(&self, other: &String) -> bool {
                &self.0 == other
            }
        }

        impl PartialEq<$name> for str {
            fn eq(&self, other: &$name) -> bool {
                self == other.0
            }
        }

        impl PartialEq<$name> for String {
            fn eq(&self, other: &$name) -> bool {
                self == &other.0
            }
        }
    };
}

string_newtype!(TypeName, "A validated GraphQL type name (e.g. `User`, `Post`).");
string_newtype!(FieldName, "A validated GraphQL field name (e.g. `id`, `createdAt`).");
string_newtype!(
    SqlSource,
    "A SQL table or view name used as the data source for a type (e.g. `v_user`)."
);
string_newtype!(RoleName, "A named security role (e.g. `admin`, `viewer`).");

/// A parsed OAuth2/JWT scope, typically in `resource:action` format.
///
/// # Example
///
/// ```
/// use fraiseql_core::schema::domain_types::Scope;
///
/// let s: Scope = "read:User.email".into();
/// assert_eq!(s.parts(), Some(("read", "User.email")));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct Scope(String);

impl Scope {
    /// Construct from any `Into<String>`.
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Return a reference to the underlying string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Split on the first colon, returning `(resource, action)` if present.
    ///
    /// Returns `None` if no colon is found (bare scope without separator).
    #[must_use]
    pub fn parts(&self) -> Option<(&str, &str)> {
        self.0.split_once(':')
    }
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<str> for Scope {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for Scope {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for Scope {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl PartialEq<str> for Scope {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for Scope {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<String> for Scope {
    fn eq(&self, other: &String) -> bool {
        &self.0 == other
    }
}

impl PartialEq<Scope> for str {
    fn eq(&self, other: &Scope) -> bool {
        self == other.0
    }
}

impl PartialEq<Scope> for String {
    fn eq(&self, other: &Scope) -> bool {
        self == &other.0
    }
}
