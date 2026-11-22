#![no_main]
//! A provisioning client's SCIM `?filter=`, through the filter parser: the one shape the server
//! supports, `attribute eq "value"`, and the refusal of everything else.
//!
//! Beyond "no panic", an accepted filter round-trips: written back as
//! `attribute eq "value"`, with the two escapes the parser undoes (`\"` and `\\`), it
//! parses to the same filter. So what a client is answered for is exactly the attribute
//! and value it sent, and no accepted spelling carries a second comparison the parser
//! dropped. The attribute is lowercased and never empty or spaced.

use fraiseql_server::api::scim::parse_scim_filter;
use libfuzzer_sys::fuzz_target;

/// The escaping `parse_eq` undoes: a backslash and a quote, each behind a backslash.
fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        if c == '\\' || c == '"' {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let Some((attribute, value)) = parse_scim_filter(&text) else {
        return;
    };

    assert!(!attribute.is_empty(), "an empty attribute was accepted");
    assert!(
        !attribute.chars().any(char::is_whitespace),
        "an attribute with whitespace was accepted: {attribute:?}"
    );
    assert_eq!(attribute, attribute.to_ascii_lowercase());

    let written = format!("{attribute} eq \"{}\"", escape(&value));
    assert_eq!(
        parse_scim_filter(&written),
        Some((attribute.clone(), value.clone())),
        "{text:?} parsed to a filter that does not survive being written back as {written:?}"
    );
});
