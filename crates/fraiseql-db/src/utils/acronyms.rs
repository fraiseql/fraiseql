//! An acronym set held as a value, for a caller that resolves names before, or apart from,
//! the process-wide set [`set_runtime_acronyms`](super::set_runtime_acronyms) installs.

use std::collections::HashSet;

use super::{DEFAULT_ACRONYMS, snake_case_by};

/// The acronyms [`to_snake_case_with`] keeps whole: the built-in defaults plus a project's
/// `[naming] acronyms`.
///
/// Built by the same rule as the set [`set_runtime_acronyms`](super::set_runtime_acronyms)
/// installs. Once `extra` is installed, `to_snake_case_with(name,
/// &Acronyms::with_extra(extra))` equals `to_snake_case(name)` (see
/// [`to_snake_case`](super::to_snake_case)).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Acronyms(HashSet<String>);

impl Acronyms {
    /// The built-in defaults plus `extra`, each trimmed and lowercased; empty terms are
    /// ignored.
    #[must_use]
    pub fn with_extra(extra: &[String]) -> Self {
        let mut set: HashSet<String> = DEFAULT_ACRONYMS.iter().map(|s| (*s).to_string()).collect();
        for term in extra {
            let term = term.trim().to_ascii_lowercase();
            if !term.is_empty() {
                set.insert(term);
            }
        }
        Self(set)
    }

    /// Whether `candidate`, a lowercase `<word><digit>` token, is in the set.
    #[must_use]
    pub fn contains(&self, candidate: &str) -> bool {
        self.0.contains(candidate)
    }
}

/// [`to_snake_case`](super::to_snake_case) with `acronyms` in place of the process-wide
/// set.
///
/// ```
/// use fraiseql_db::utils::{Acronyms, to_snake_case_with};
///
/// let project = Acronyms::with_extra(&["acme9".to_string()]);
/// assert_eq!(to_snake_case_with("acme9Id", &project), "acme9_id");
/// assert_eq!(to_snake_case_with("acme9Id", &Acronyms::with_extra(&[])), "acme_9_id");
/// ```
#[must_use]
pub fn to_snake_case_with(name: &str, acronyms: &Acronyms) -> String {
    snake_case_by(name, |candidate| acronyms.contains(candidate))
}

#[cfg(test)]
#[path = "acronyms_tests.rs"]
mod tests;
