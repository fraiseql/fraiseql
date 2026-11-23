//! The path a subscription filter reads from an event: one spelling in the compiled schema.
//!
//! Authors write a path the way every SDK documents it, `$.user.id`, or as `user.id` or the
//! JSON pointer `/user/id`. The compiler turns each into the pointer, and the compiled
//! schema holds only that, so every reader of a filter path (delivery, the localized-field
//! lookup, the subscription plan's read gate) reads the same keys.

/// The JSON pointer an authored filter path names.
///
/// Accepts `$.a.b`, `a.b` and `/a/b`; each names the key `a`, then the key `b` under it.
///
/// # Errors
///
/// A message naming the path and the accepted forms, for any other spelling: a bare `$`,
/// a `JSONPath` descendant (`$..a`) or bracket (`$['a']`, `$[0]`) step, an empty segment, or
/// a segment holding `$`, `[`, `]`, `~`, whitespace or (in a pointer) `.`.
pub fn to_pointer(path: &str) -> Result<String, String> {
    let segments: Vec<&str> = if let Some(pointer) = path.strip_prefix('/') {
        pointer.split('/').collect()
    } else {
        path.strip_prefix("$.").unwrap_or(path).split('.').collect()
    };
    if segments.iter().all(|segment| is_key(segment)) {
        Ok(format!("/{}", segments.join("/")))
    } else {
        Err(format!(
            "'{path}' is not a filter path: write the field it reads as `$.field`, `field` or \
             `/field`, and a nested one as `$.parent.field`, `parent.field` or `/parent/field`"
        ))
    }
}

/// Whether `path` is a filter path as the compiled schema holds it: the pointer
/// [`to_pointer`] produces.
#[must_use]
pub fn is_pointer(path: &str) -> bool {
    path.starts_with('/') && to_pointer(path).is_ok_and(|pointer| pointer == path)
}

/// A key every reader of a filter path reads alike: the pointer readers split on `/`, the
/// plan's read gate on `/` and `.`, and `~` would be a pointer escape.
fn is_key(segment: &str) -> bool {
    !segment.is_empty()
        && !segment
            .chars()
            .any(|c| matches!(c, '$' | '[' | ']' | '~' | '.' | '/') || c.is_whitespace())
}

#[cfg(test)]
#[path = "filter_path_tests.rs"]
mod tests;
