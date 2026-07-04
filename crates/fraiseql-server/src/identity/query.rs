//! Safe named-parameter binding for identity-resolution queries.
//!
//! Ported verbatim from #242 (`routes/enrichment.rs`, `v2.2.1`). `$name` tokens
//! in the configured query are rewritten to positional `$1..$N` placeholders and
//! the corresponding claim **values** are bound out-of-band by the caller — they
//! are **never** interpolated into the SQL string, so a hostile claim value
//! cannot alter the query structure. The adversarial test suite lives in the
//! sibling `tests` module.

use std::collections::HashMap;

/// A query with named `$name` parameters rewritten to positional `$N` and the
/// ordered list of claim values to bind.
#[derive(Debug)]
pub(super) struct BoundQuery {
    /// SQL with `$1`, `$2`, … placeholders.
    pub(super) sql:   String,
    /// Ordered bind values matching the positional placeholders.
    pub(super) binds: Vec<serde_json::Value>,
}

/// Rewrite `$name` tokens in `query` to positional `$1`, `$2`, … and look up the
/// corresponding values in `claims`.
///
/// A named parameter starts with `$` followed by an ASCII letter or underscore;
/// `$` followed by a digit (a PostgreSQL positional placeholder) is passed
/// through unchanged. Repeated names reuse the same positional index. Values are
/// returned in `binds` for the caller to bind positionally — never spliced into
/// the SQL text.
///
/// # Errors
///
/// Returns an error string if a referenced parameter is missing from `claims`.
pub(super) fn prepare_enrichment_query(
    query: &str,
    claims: &HashMap<String, serde_json::Value>,
) -> Result<BoundQuery, String> {
    let mut sql = String::with_capacity(query.len());
    let mut binds: Vec<serde_json::Value> = Vec::new();
    let mut param_index: HashMap<String, usize> = HashMap::new();

    let bytes = query.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() && is_name_start(bytes[i + 1]) {
            // Extract parameter name.
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() && is_name_char(bytes[end]) {
                end += 1;
            }
            let name = &query[start..end];

            // Reuse an existing position or assign a new one.
            let pos = if let Some(&existing) = param_index.get(name) {
                existing
            } else {
                let value = claims.get(name).ok_or_else(|| {
                    format!("Enrichment query references ${name} but it is not in the JWT claims")
                })?;
                binds.push(value.clone());
                let pos = binds.len();
                param_index.insert(name.to_owned(), pos);
                pos
            };

            sql.push('$');
            sql.push_str(&pos.to_string());
            i = end;
        } else {
            // Index byte-by-byte within ASCII SQL text.
            sql.push(char::from(bytes[i]));
            i += 1;
        }
    }

    Ok(BoundQuery { sql, binds })
}

const fn is_name_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

const fn is_name_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}
