//! Tests for the `TypeScript` per-file emitters.
use super::push_doc;

/// A description containing `*/` must not be able to close the doc comment it is
/// interpolated into.
///
/// `push_doc` wrote `/** {description} */` with no escaping, and every author-supplied
/// description in the generated client — type, field, enum, input, query, mutation,
/// union, interface — goes through it. A schema field documented
/// `"ends the comment */ export const OWNED = 1; /*"` therefore emitted `TypeScript`
/// that closed the comment early and declared whatever followed: comment injection
/// into a generated client, from a string the schema author controls.
#[test]
fn doc_comment_terminator_cannot_escape_the_comment() {
    let mut out = String::new();
    push_doc(&mut out, "", Some("ends the comment */ export const OWNED = 1; /*"));

    assert!(
        !out.contains("*/ export"),
        "the injected terminator survived into the generated source: {out}"
    );
    // Exactly one comment opener and one closer, both the ones this function wrote.
    assert_eq!(out.matches("*/").count(), 1, "more than one comment terminator: {out}");
    assert_eq!(out.matches("/**").count(), 1, "more than one comment opener: {out}");
    assert!(out.starts_with("/** "), "the comment must still open normally: {out}");
    assert!(out.trim_end().ends_with(" */"), "the comment must still close: {out}");
}

/// The escape must preserve the text, not delete it — a reader still needs the
/// description, and silently dropping content is its own defect.
#[test]
fn doc_comment_escape_preserves_the_description_text() {
    let mut out = String::new();
    push_doc(&mut out, "", Some("see docs/*.md for the glob"));

    assert!(out.contains("see docs/"), "the description text was lost: {out}");
    assert!(out.contains(".md for the glob"), "the description text was lost: {out}");
}

/// Newlines already collapse to keep the comment on one line; that must not regress.
#[test]
fn doc_comment_stays_on_one_line() {
    let mut out = String::new();
    push_doc(&mut out, "  ", Some("first\nsecond"));

    assert_eq!(out, "  /** first second */\n");
}

/// No description means no comment at all, not an empty one.
#[test]
fn doc_comment_is_omitted_when_absent() {
    let mut out = String::new();
    push_doc(&mut out, "", None);
    assert!(out.is_empty());
}
