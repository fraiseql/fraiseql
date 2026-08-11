//! The SCIM filter subset this server supports (#946).
//!
//! RFC 7644 §3.4.2.2 defines a full expression language. Provisioning clients use a vanishing
//! fraction of it: Okta and Entra send `userName eq "…"` to check whether a user already
//! exists, and `displayName eq "…"` for groups. That is what is implemented.
//!
//! **An unsupported filter is refused, never ignored.** Dropping a filter we do not
//! understand would answer a "does this user exist?" probe with the *whole directory*, and
//! the client would read the first row as a match — a silent-widening bug that provisions
//! onto the wrong account. RFC 7644 has a status for exactly this (`400 invalidFilter`), and
//! that is what an unparseable filter gets.

/// A parsed filter: equality on one supported attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EqFilter {
    /// The attribute name, lowercased.
    pub attribute: String,
    /// The compared value, unescaped.
    pub value:     String,
}

/// Why a filter was refused. The message is safe to return to the client — it describes the
/// client's own input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterError(pub String);

/// Parse `<attribute> eq "<value>"`, the only shape supported.
///
/// The comparison operator is matched case-insensitively (`eq` / `EQ`), as is the attribute
/// name, per RFC 7644 §3.4.2.2. The value must be a quoted string; a bare token is refused
/// rather than guessed at.
///
/// # Errors
///
/// [`FilterError`] when the expression is not a single `eq` comparison on a quoted value.
pub fn parse_eq(filter: &str) -> Result<EqFilter, FilterError> {
    let trimmed = filter.trim();

    // Refuse composition explicitly, so a client sending `a eq "x" and b eq "y"` is told
    // the filter is unsupported instead of silently matching only the first term.
    let lowered = trimmed.to_ascii_lowercase();
    for connective in [" and ", " or ", " not "] {
        if lowered.contains(connective) {
            return Err(FilterError(format!(
                "unsupported filter: only a single 'attribute eq \"value\"' comparison is \
                 supported, and this one uses '{}'",
                connective.trim()
            )));
        }
    }

    let (attribute, rest) = trimmed.split_once(char::is_whitespace).ok_or_else(|| {
        FilterError("unsupported filter: expected 'attribute eq \"value\"'".to_string())
    })?;
    let rest = rest.trim_start();
    let (op, value) = rest.split_once(char::is_whitespace).ok_or_else(|| {
        FilterError("unsupported filter: expected 'attribute eq \"value\"'".to_string())
    })?;
    if !op.eq_ignore_ascii_case("eq") {
        return Err(FilterError(format!(
            "unsupported filter operator '{op}': only 'eq' is supported"
        )));
    }

    let value = value.trim();
    let unquoted = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')).ok_or_else(|| {
        FilterError("unsupported filter: value must be a quoted string".to_string())
    })?;

    Ok(EqFilter {
        attribute: attribute.trim().to_ascii_lowercase(),
        value:     unescape(unquoted),
    })
}

/// Undo the JSON-style escaping RFC 7644 borrows for quoted filter values.
fn unescape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('"') => out.push('"'),
                Some(other) => {
                    // A backslash before anything but a quote is literal, and a trailing
                    // one stays as itself rather than swallowing the end of the value.
                    out.push('\\');
                    if other != '\\' {
                        out.push(other);
                    }
                },
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Resolve a parsed filter against the one attribute a resource can be filtered on.
///
/// Returns the value to match, or an error naming the supported attribute. A filter on an
/// attribute we cannot index is refused for the same reason an unparseable one is: answering
/// it by ignoring the filter is worse than answering `400`.
///
/// # Errors
///
/// [`FilterError`] when the filter names a different attribute.
pub fn expect_attribute(filter: &EqFilter, supported: &str) -> Result<String, FilterError> {
    if filter.attribute == supported.to_ascii_lowercase() {
        return Ok(filter.value.clone());
    }
    Err(FilterError(format!(
        "unsupported filter attribute '{}': only '{supported}' is filterable here",
        filter.attribute
    )))
}
