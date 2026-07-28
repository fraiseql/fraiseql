//! Tests for the bucket MIME allow-list (#876 item 4).

#![allow(missing_docs)] // Reason: test functions are self-describing

use super::{BucketAccess, BucketConfig};

fn bucket(allowed: Option<&[&str]>) -> BucketConfig {
    BucketConfig {
        name:               "docs".to_string(),
        max_object_bytes:   None,
        allowed_mime_types: allowed.map(|list| list.iter().map(|s| (*s).to_string()).collect()),
        access:             BucketAccess::Private,
        transform_presets:  None,
        serve_inline:       false,
    }
}

#[test]
fn parameters_do_not_defeat_an_exact_entry() {
    let b = bucket(Some(&["application/pdf", "text/plain"]));
    // What a browser actually sends for `new Blob([...], {type: 'text/plain'})`.
    assert!(b.allows_mime("text/plain;charset=UTF-8"));
    assert!(b.allows_mime("text/plain; charset=utf-8"));
    assert!(b.allows_mime("application/pdf; name=\"invoice.pdf\""));
    assert!(b.allows_mime("text/plain"));
}

#[test]
fn matching_is_case_insensitive() {
    let b = bucket(Some(&["image/PNG"]));
    assert!(b.allows_mime("IMAGE/png"));
}

#[test]
fn wildcards_work_at_both_enforcement_points() {
    let b = bucket(Some(&["image/*"]));
    assert!(b.allows_mime("image/jpeg"));
    assert!(b.allows_mime("image/svg+xml; charset=utf-8"));
    assert!(!b.allows_mime("text/plain"));
    // `image/*` must not match a type that merely starts with "image".
    assert!(!b.allows_mime("imagexml/foo"));

    assert!(bucket(Some(&["*/*"])).allows_mime("application/octet-stream"));
}

#[test]
fn disallowed_types_are_still_rejected() {
    let b = bucket(Some(&["application/pdf"]));
    assert!(!b.allows_mime("text/html"));
    assert!(!b.allows_mime("application/pdfx"));
}

#[test]
fn no_list_means_no_restriction() {
    assert!(bucket(None).allows_mime("anything/at-all"));
}

/// The documented meaning of `Some([])` is "none allowed". `put_handler` read
/// it as "no restriction", so a bucket configured `allowed_mime_types = []`
/// accepted every upload.
#[test]
fn an_empty_list_allows_nothing() {
    let b = bucket(Some(&[]));
    assert!(!b.allows_mime("text/plain"));
    assert!(!b.allows_mime("application/octet-stream"));
}
