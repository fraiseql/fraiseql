//! Bracket operator parsing for REST queries.
//!
//! Handles `?field[op]=value` syntax.

/// Parse a bracket key like `name[icontains]` into `("name", "icontains")`.
pub fn parse_bracket_key(key: &str) -> Option<(String, String)> {
    let open = key.find('[')?;
    let close = key.find(']')?;
    if close <= open + 1 || close != key.len() - 1 {
        return None;
    }
    let field = &key[..open];
    let op = &key[open + 1..close];
    if field.is_empty() || op.is_empty() {
        return None;
    }
    Some((field.to_string(), op.to_string()))
}
