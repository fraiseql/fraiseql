#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::{is_pointer, to_pointer};

#[test]
fn every_documented_spelling_names_the_same_pointer() {
    for path in ["$.id", "id", "/id"] {
        assert_eq!(to_pointer(path).unwrap(), "/id", "{path}");
    }
    for path in ["$.user.id", "user.id", "/user/id"] {
        assert_eq!(to_pointer(path).unwrap(), "/user/id", "{path}");
    }
}

#[test]
fn a_path_no_reader_reads_alike_is_refused_naming_the_forms() {
    for path in [
        "",
        "$",
        "$.",
        "$..id",
        "$['id']",
        "$[0]",
        "$.items[0]",
        "/",
        "/$/id",
        "//id",
        "/a.b",
        "/a~1b",
        "a..b",
        "a b",
        "a.",
    ] {
        let err = to_pointer(path).unwrap_err();
        assert!(err.contains(&format!("'{path}'")) && err.contains("`$.field`"), "{path}: {err}");
    }
}

#[test]
fn only_the_pointer_is_the_compiled_spelling() {
    assert!(is_pointer("/id"));
    assert!(is_pointer("/user/id"));
    for path in ["$.id", "id", "user.id", "/$/id", "/a.b", "/"] {
        assert!(!is_pointer(path), "{path}");
    }
}
