//! The function an accent-insensitive comparison folds both of its sides through.
//!
//! PostgreSQL's `unaccent()` is `STABLE` (its dictionary is looked up at call time), so
//! no index expression can use it. A deployment that declares accent-insensitive filters
//! creates an `IMMUTABLE` wrapper that pins the dictionary (`fraiseql compile --emit-ddl`
//! writes it), and the schema names that wrapper. Its name is interpolated into SQL, so
//! it is held to a strict shape here and quoted on every use.

use std::{fmt, sync::LazyLock};

use fraiseql_error::{FraiseQLError, Result};
use serde::{Deserialize, Serialize};

/// The default wrapper name, `fraiseql.unaccent`.
pub const DEFAULT_UNACCENT_FUNCTION: &str = "fraiseql.unaccent";

/// The longest identifier PostgreSQL keeps (`NAMEDATALEN - 1`).
const MAX_IDENTIFIER_BYTES: usize = 63;

/// A schema-qualified function name, `schema.function`, each part a lower-case SQL
/// identifier.
///
/// Qualified, because an unqualified name resolves through the session's `search_path`,
/// which is not the schema's to fix. Lower case, because PostgreSQL folds an unquoted name
/// to lower case: the quoted name rendered here is then the one an unquoted `CREATE
/// FUNCTION` made.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct UnaccentFunction {
    schema:   String,
    function: String,
}

impl UnaccentFunction {
    /// Parse `schema.function`.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Validation`] when `name` is not two lower-case SQL
    /// identifiers joined by one dot.
    pub fn parse(name: &str) -> Result<Self> {
        let refusal = || FraiseQLError::Validation {
            message: format!(
                "accent-insensitive function `{name}` must be a schema-qualified lower-case \
                 name, `schema.function` (for example `{DEFAULT_UNACCENT_FUNCTION}`), each part \
                 a letter or underscore followed by letters, digits or underscores, at most \
                 {MAX_IDENTIFIER_BYTES} bytes"
            ),
            path:    None,
        };
        let (schema, function) = name.split_once('.').ok_or_else(refusal)?;
        if !is_identifier(schema) || !is_identifier(function) {
            return Err(refusal());
        }
        Ok(Self {
            schema:   schema.to_string(),
            function: function.to_string(),
        })
    }

    /// The schema the function lives in.
    #[must_use]
    pub fn schema(&self) -> &str {
        &self.schema
    }

    /// The function's own name.
    #[must_use]
    pub fn function(&self) -> &str {
        &self.function
    }

    /// `"schema"."function"(<arg>)`.
    #[must_use]
    pub fn call(&self, arg: &str) -> String {
        format!("\"{}\".\"{}\"({arg})", self.schema, self.function)
    }
}

impl Default for UnaccentFunction {
    fn default() -> Self {
        Self {
            schema:   "fraiseql".to_string(),
            function: "unaccent".to_string(),
        }
    }
}

impl fmt::Display for UnaccentFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.schema, self.function)
    }
}

impl TryFrom<String> for UnaccentFunction {
    type Error = FraiseQLError;

    fn try_from(name: String) -> Result<Self> {
        Self::parse(&name)
    }
}

impl From<UnaccentFunction> for String {
    fn from(function: UnaccentFunction) -> Self {
        function.to_string()
    }
}

fn is_identifier(part: &str) -> bool {
    let mut bytes = part.bytes();
    part.len() <= MAX_IDENTIFIER_BYTES
        && bytes.next().is_some_and(|b| b.is_ascii_lowercase() || b == b'_')
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// The characters an accent-insensitive regex pattern may hold beyond ASCII: letters
/// (other than modifier letters) and combining marks.
///
/// The pattern is folded through `unaccent` before PostgreSQL compiles it, and the
/// dictionary folds many symbols into ASCII regex syntax: `（` to `(`, `＋` to `+`, `⑴`
/// to `(1)`. A pattern the length and nesting checks pass could then compile as one they
/// refuse. No letter of these classes folds into regex syntax or into more bytes than it
/// has, so with every other character refused, the checks made on the pattern as written
/// hold for the pattern PostgreSQL compiles.
static FOLD_SAFE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"^[\x00-\x7F\p{Lu}\p{Ll}\p{Lt}\p{Lo}\p{Mn}]*$")
        .expect("static regex is valid")
});

/// Refuse an accent-insensitive regex pattern holding a character the fold could turn
/// into regex syntax.
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] naming the first such character.
pub fn check_fold_safe_pattern(pattern: &str) -> Result<()> {
    if FOLD_SAFE.is_match(pattern) {
        return Ok(());
    }
    let offending = pattern
        .chars()
        .find(|c| !FOLD_SAFE.is_match(c.encode_utf8(&mut [0; 4])))
        .unwrap_or_default();
    Err(FraiseQLError::Validation {
        message: format!(
            "an accent-insensitive regex pattern may hold, beyond ASCII, only letters and \
             combining marks: `{offending}` (U+{:04X}) folds before the pattern is compiled",
            u32::from(offending)
        ),
        path:    None,
    })
}

#[cfg(test)]
#[path = "unaccent_tests.rs"]
mod unaccent_tests;
