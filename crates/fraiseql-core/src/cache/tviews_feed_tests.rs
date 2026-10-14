#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::collections::{BTreeMap, BTreeSet};

use super::*;

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(ToString::to_string).collect()
}

fn change(payload: &str) -> TviewsChange {
    match decode(payload) {
        TviewsMessage::Change(change) => change,
        TviewsMessage::Everything => panic!("evicts everything: {payload}"),
    }
}

#[test]
fn rows_inserted_updated_and_deleted_are_read_by_type() {
    let c = change(r#"{"v":2,"u":{"User":["u1"],"Comment":["c1","c2"]},"d":{"Comment":["c3"]}}"#);
    assert_eq!(
        c.rows,
        BTreeMap::from([
            ("Comment".to_string(), set(&["c1", "c2", "c3"])),
            ("User".to_string(), set(&["u1"])),
        ])
    );
    assert!(c.collections.is_empty() && c.types.is_empty(), "{c:?}");
}

#[test]
fn an_integer_id_is_read_as_its_decimal_text() {
    let c = change(r#"{"v":2,"u":{"Tag":[42]}}"#);
    assert_eq!(c.rows["Tag"], set(&["42"]));
}

#[test]
fn collections_are_read_by_child_type_and_column() {
    let c = change(r#"{"v":2,"c":{"Comment.fk_post":["p1","p2"],"Comment.fk_author":[7]}}"#);
    assert_eq!(
        c.collections,
        BTreeMap::from([
            (Collection::new("Comment", "fk_author"), set(&["7"])),
            (Collection::new("Comment", "fk_post"), set(&["p1", "p2"])),
        ])
    );
}

#[test]
fn a_reference_column_may_hold_a_dot() {
    let c = change(r#"{"v":2,"c":{"Comment.fk.post":["p1"]}}"#);
    assert_eq!(c.collections.keys().next().unwrap(), &Collection::new("Comment", "fk.post"));
}

#[test]
fn types_are_read() {
    let c = change(r#"{"v":2,"t":["Audit","Post"]}"#);
    assert_eq!(c.types, set(&["Audit", "Post"]));
}

#[test]
fn an_id_that_is_neither_text_nor_a_number_evicts_its_whole_type() {
    let c = change(r#"{"v":2,"u":{"User":["u1",null]},"d":{"Tag":[{"x":1}]}}"#);
    assert_eq!(c.types, set(&["Tag", "User"]));
}

#[test]
fn a_collection_whose_parent_cannot_be_read_evicts_its_child_type() {
    let c = change(r#"{"v":2,"c":{"Comment.fk_post":[true]}}"#);
    assert!(c.collections.is_empty(), "{c:?}");
    assert_eq!(c.types, set(&["Comment"]));
}

#[test]
fn an_empty_message_evicts_nothing() {
    assert_eq!(change(r#"{"v":2}"#), TviewsChange::default());
}

#[test]
fn another_version_evicts_everything() {
    for payload in [
        r#"{"v":1,"u":{"User":["u1"]}}"#,
        r#"{"v":3}"#,
        r#"{"u":{}}"#,
    ] {
        assert_eq!(decode(payload), TviewsMessage::Everything, "{payload}");
    }
}

#[test]
fn an_unreadable_message_evicts_everything() {
    for payload in [
        "",
        "not json",
        r#"{"v":2,"u":["User"]}"#,
        r#"{"v":2,"t":"User"}"#,
        r#"{"v":2,"c":{"no_dot":["p1"]}}"#,
        r#"{"v":2,"c":{".fk_post":["p1"]}}"#,
        r#"{"v":2,"c":{"Comment.":["p1"]}}"#,
    ] {
        assert_eq!(decode(payload), TviewsMessage::Everything, "{payload:?}");
    }
}
