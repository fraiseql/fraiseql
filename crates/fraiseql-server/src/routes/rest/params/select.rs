//! Select entry parsing for REST queries.
//!
//! Supports `?select=` with parenthetical embedding syntax.

use fraiseql_error::FraiseQLError;

use super::EmbeddedSpec;
use crate::routes::rest::params::{SelectEntry, helpers::validation_error};

/// Maximum parenthetical nesting depth allowed during parsing.
///
/// Prevents stack overflow from deeply nested `?select=a(b(c(...)))` before
/// the post-parse [`validate_embedding_depth`] check runs.
const MAX_PARSE_DEPTH: usize = 32;

/// Parse a `?select=` value into a list of [`SelectEntry`] items.
///
/// Supports:
/// - Flat fields: `id`, `name`
/// - Embedded resources: `posts(id,title)`
/// - Nested embedding: `posts(id,comments(id,body))`
/// - Renamed embedding: `author:fk_user(id,name)`
/// - Count-only: `posts.count`
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` on unbalanced parentheses, empty field
/// names, or nesting exceeding `MAX_PARSE_DEPTH` levels.
pub fn parse_select_entries(input: &str) -> Result<Vec<SelectEntry>, FraiseQLError> {
    parse_select_entries_inner(input, 0)
}

/// Inner recursive parser with depth tracking.
fn parse_select_entries_inner(
    input: &str,
    depth: usize,
) -> Result<Vec<SelectEntry>, FraiseQLError> {
    if depth > MAX_PARSE_DEPTH {
        return Err(validation_error(format!(
            "Select nesting depth exceeds maximum of {MAX_PARSE_DEPTH}. \
             Reduce parenthetical nesting in `select` parameter."
        )));
    }
    let mut entries = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    // Byte offset of each char, plus a sentinel at index `len` equal to the
    // string's byte length. The walk indexes `chars` by char position, but
    // those positions MUST be translated to byte offsets before slicing
    // `input` — conflating char index and byte index panics on any multi-byte
    // UTF-8 char (e.g. `?select=%C3%A9`), a remotely-triggerable abort (H17).
    let byte_offsets: Vec<usize> = input
        .char_indices()
        .map(|(b, _)| b)
        .chain(std::iter::once(input.len()))
        .collect();
    let mut i = 0;

    while i < len {
        // Skip whitespace and leading commas.
        while i < len && (chars[i] == ',' || chars[i] == ' ') {
            i += 1;
        }
        if i >= len {
            break;
        }

        // Read the field/relationship name (until we hit '(', ',', '.', or end).
        let name_start = i;
        while i < len && chars[i] != '(' && chars[i] != ',' && chars[i] != '.' && chars[i] != ' ' {
            i += 1;
        }
        let name = &input[byte_offsets[name_start]..byte_offsets[i]];
        let name = name.trim();

        if name.is_empty() {
            return Err(validation_error("Empty field name in `select` parameter".to_string()));
        }

        // Skip whitespace.
        while i < len && chars[i] == ' ' {
            i += 1;
        }

        if i < len && chars[i] == '.' {
            // Count-only: posts.count
            i += 1; // skip '.'
            let suffix_start = i;
            while i < len && chars[i] != ',' && chars[i] != ' ' {
                i += 1;
            }
            let suffix = &input[byte_offsets[suffix_start]..byte_offsets[i]];
            if suffix == "count" {
                entries.push(SelectEntry::Count(name.to_string()));
            } else {
                return Err(validation_error(format!(
                    "Unsupported dot-suffix '{suffix}' in `select`. Only `.count` is supported."
                )));
            }
        } else if i < len && chars[i] == '(' {
            // Embedded resource: posts(id,title) or author:rel_name(id,name)
            let (rename, relationship) = if let Some(colon_pos) = name.find(':') {
                (Some(name[..colon_pos].to_string()), name[colon_pos + 1..].to_string())
            } else {
                (None, name.to_string())
            };

            // Find matching closing paren (handle nesting). This counter is
            // distinct from the recursion `depth` parameter: a previous
            // `let mut depth = 1` here shadowed `depth`, and the loop drove the
            // shadow to 0, so the recursive call below always passed `1` and
            // the `MAX_PARSE_DEPTH` guard never fired — unbounded recursion on
            // `?select=a(a(a(…)))` overflowed the stack and aborted the process
            // (H18).
            i += 1; // skip '('
            let inner_start = i;
            let mut paren_depth = 1;
            while i < len && paren_depth > 0 {
                if chars[i] == '(' {
                    paren_depth += 1;
                } else if chars[i] == ')' {
                    paren_depth -= 1;
                }
                if paren_depth > 0 {
                    i += 1;
                }
            }
            if paren_depth != 0 {
                return Err(validation_error(format!(
                    "Unbalanced parentheses in `select` for '{relationship}'"
                )));
            }
            let inner = &input[byte_offsets[inner_start]..byte_offsets[i]];
            i += 1; // skip ')'

            // Recurse with the real nesting depth so `MAX_PARSE_DEPTH` bounds it.
            let sub_entries = parse_select_entries_inner(inner, depth + 1)?;

            entries.push(SelectEntry::Embedded(EmbeddedSpec {
                relationship,
                rename,
                fields: sub_entries,
            }));
        } else {
            // Check for rename syntax on flat field (shouldn't happen, but handle gracefully).
            entries.push(SelectEntry::Field(name.to_string()));
        }
    }

    Ok(entries)
}

/// Validate that embedding depth does not exceed the configured maximum.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if `current_depth` exceeds `max_depth`
/// or if any nested embedded spec violates the depth limit.
pub fn validate_embedding_depth(
    spec: &EmbeddedSpec,
    current_depth: usize,
    max_depth: usize,
) -> Result<(), FraiseQLError> {
    if current_depth > max_depth {
        return Err(validation_error(format!(
            "Embedding depth {current_depth} exceeds maximum of {max_depth}. \
             Reduce nesting in `select` parameter."
        )));
    }
    for field in &spec.fields {
        if let SelectEntry::Embedded(nested) = field {
            validate_embedding_depth(nested, current_depth + 1, max_depth)?;
        }
    }
    Ok(())
}
