//! UTF-8-safe string truncation helpers.
//!
//! Error, log, and audit paths routinely truncate user-controlled query text
//! for display. Slicing a `&str` at a fixed byte offset (`&s[..100]`) panics
//! when that offset lands inside a multi-byte UTF-8 character — a
//! remotely-triggerable abort if the truncated string is attacker-influenced
//! (audit H20). These helpers truncate on character boundaries instead.

/// Borrow the longest prefix of `s` that is at most `max_bytes` long and ends
/// on a UTF-8 character boundary.
///
/// Returns `s` unchanged when it already fits. Never panics and never splits a
/// character: if `max_bytes` falls inside a multi-byte character, the prefix is
/// shortened to the preceding boundary (so the result may be a few bytes
/// shorter than `max_bytes`).
#[must_use]
pub fn truncate_at_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    // `end` is now a valid char boundary in `0..=max_bytes`; slicing is safe.
    &s[..end]
}

/// Render `s` for inclusion in an error/log message, truncated to `max_bytes`.
///
/// Returns `s` unchanged when it fits; otherwise a character-boundary-safe
/// prefix followed by a trailing `...`. This is the char-safe replacement for
/// the `if s.len() > N { format!("{}...", &s[..N]) } else { s.to_string() }`
/// snippet that was copy-pasted across the query-timeout, audit-export, and
/// error-formatting paths (audit H20).
#[must_use]
pub fn truncate_for_display(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        s.to_string()
    } else {
        format!("{}...", truncate_at_char_boundary(s, max_bytes))
    }
}

#[cfg(test)]
mod text_tests {
    use super::*;

    #[test]
    fn short_string_unchanged() {
        assert_eq!(truncate_at_char_boundary("hello", 100), "hello");
        assert_eq!(truncate_for_display("hello", 100), "hello");
    }

    #[test]
    fn exact_length_unchanged() {
        assert_eq!(truncate_at_char_boundary("abcde", 5), "abcde");
        assert_eq!(truncate_for_display("abcde", 5), "abcde");
    }

    #[test]
    fn ascii_truncates_at_limit() {
        let s = "a".repeat(200);
        assert_eq!(truncate_at_char_boundary(&s, 50).len(), 50);
        let display = truncate_for_display(&s, 50);
        assert!(display.ends_with("..."));
        assert_eq!(display.len(), 53);
    }

    #[test]
    fn does_not_split_multibyte_char() {
        // "é" is two bytes (0xC3 0xA9). A cut at an odd byte must step back to
        // the preceding boundary rather than panic.
        let s = "é".repeat(10); // 20 bytes
        let truncated = truncate_at_char_boundary(&s, 5);
        // 5 lands mid-char → step back to 4 (two whole 'é').
        assert_eq!(truncated, "éé");
        assert!(s.is_char_boundary(truncated.len()));
    }

    #[test]
    fn display_preserves_validity_on_multibyte() {
        let s = "héllo wörld ".repeat(20);
        for max in 0..s.len() {
            let out = truncate_for_display(&s, max);
            // Must always be valid UTF-8 (String guarantees it) and never panic.
            assert!(out.is_char_boundary(0));
        }
    }

    #[test]
    fn zero_max_bytes() {
        assert_eq!(truncate_at_char_boundary("abc", 0), "");
        assert_eq!(truncate_for_display("abc", 0), "...");
    }
}
