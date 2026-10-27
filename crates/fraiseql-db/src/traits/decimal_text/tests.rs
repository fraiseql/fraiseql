#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use serde_json::{Value, json};

use super::{
    DecimalFields, DecimalNode, DecimalShape, WriteColumn, WriteDecimals, quote_cascade_decimals,
    quote_decimals,
};

fn decimal() -> DecimalNode {
    DecimalNode::Decimal
}

fn object(type_name: &str) -> DecimalNode {
    DecimalNode::Object(type_name.to_string())
}

/// A shape read as `root`, with each listed type's fields.
fn shape(root: DecimalNode, types: &[(&str, &[(&str, DecimalNode)])]) -> DecimalShape {
    let types = types
        .iter()
        .map(|(name, entries)| {
            let mut fields = DecimalFields::new();
            for (key, node) in *entries {
                fields.insert(*key, node.clone());
            }
            ((*name).to_string(), fields)
        })
        .collect();
    DecimalShape::new(root, Arc::new(types))
}

/// A `Product` whose `price` is a `Decimal`.
fn product() -> DecimalShape {
    shape(object("Product"), &[("Product", &[("price", decimal())])])
}

fn quoted(src: &str, shape: &DecimalShape) -> String {
    let mut out = Vec::new();
    quote_decimals(src.as_bytes(), shape, &mut out);
    String::from_utf8(out).unwrap()
}

fn parsed(src: &str, shape: &DecimalShape) -> Value {
    serde_json::from_str(&quoted(src, shape)).unwrap()
}

#[test]
fn a_decimal_number_becomes_a_string_of_its_exact_digits() {
    for (stored, digits) in [
        ("1.50", "1.50"),
        ("100.00", "100.00"),
        ("12345678901234567890.123456789", "12345678901234567890.123456789"),
        ("-0.001", "-0.001"),
        ("0", "0"),
        ("1e400", "1e400"),
    ] {
        assert_eq!(
            parsed(&format!(r#"{{"price": {stored}}}"#), &product()),
            json!({"price": digits}),
            "{stored}"
        );
    }
}

#[test]
fn every_other_byte_is_copied_as_it_is() {
    assert_eq!(
        quoted(r#"{"n": 1.50, "price": 2.50, "s": "x"}"#, &product()),
        r#"{"n": 1.50, "price": "2.50", "s": "x"}"#
    );
}

#[test]
fn a_null_or_a_string_at_a_decimal_position_is_left_alone() {
    let shape = shape(object("T"), &[("T", &[("a", decimal()), ("b", decimal())])]);
    let src = r#"{"a": null, "b": "1.50"}"#;
    assert_eq!(quoted(src, &shape), src);
}

#[test]
fn a_decimal_inside_a_nested_object_and_a_list_of_objects_is_quoted() {
    let shape = shape(
        object("Customer"),
        &[
            ("Customer", &[("order", object("Order"))]),
            ("Order", &[("lines", object("Line")), ("total", decimal())]),
            ("Line", &[("amount", decimal())]),
        ],
    );
    assert_eq!(
        parsed(
            r#"{"order": {"total": 3.00, "lines": [{"amount": 1.50, "qty": 1}, {"amount": 1.50, "qty": 1}]}}"#,
            &shape
        ),
        json!({"order": {"total": "3.00", "lines": [{"amount": "1.50", "qty": 1}, {"amount": "1.50", "qty": 1}]}})
    );
}

#[test]
fn a_recursive_type_is_quoted_at_every_depth_the_document_reaches() {
    let shape = shape(
        object("User"),
        &[("User", &[("balance", decimal()), ("friends", object("User"))])],
    );
    assert_eq!(
        parsed(
            r#"{"balance": 1.10, "friends": [{"balance": 2.20, "friends": [{"balance": 3.30}]}]}"#,
            &shape
        ),
        json!({"balance": "1.10", "friends": [{"balance": "2.20", "friends": [{"balance": "3.30"}]}]})
    );
}

#[test]
fn a_list_of_decimals_is_quoted_element_by_element() {
    let shape = shape(object("T"), &[("T", &[("rates", decimal())])]);
    assert_eq!(
        parsed(r#"{"rates": [1.10, [2.20, null], 3]}"#, &shape),
        json!({"rates": ["1.10", ["2.20", null], "3"]})
    );
}

#[test]
fn a_key_not_named_is_skipped_whatever_it_holds() {
    let src =
        r#"{"other": {"price": 1.50, "s": "} ] \" {"}, "list": [{"price": 2.5}], "price": 9.90}"#;
    assert_eq!(
        parsed(src, &product()),
        json!({"other": {"price": 1.5, "s": "} ] \" {"}, "list": [{"price": 2.5}], "price": "9.90"})
    );
}

#[test]
fn a_type_absent_from_the_shape_is_copied_as_it_is() {
    let shape =
        shape(object("T"), &[("T", &[("child", object("NoDecimal")), ("price", decimal())])]);
    assert_eq!(
        parsed(r#"{"child": {"price": 1.50}, "price": 1.50}"#, &shape),
        json!({"child": {"price": 1.5}, "price": "1.50"})
    );
}

#[test]
fn a_string_holding_json_punctuation_does_not_shift_the_scan() {
    assert_eq!(
        parsed(r#"{"note": "\"price\": 1.50, {", "price": 7.70}"#, &product()),
        json!({"note": "\"price\": 1.50, {", "price": "7.70"})
    );
}

#[test]
fn an_escaped_key_is_compared_by_its_decoded_text() {
    assert_eq!(parsed(r#"{"pr\u0069ce": 1.50}"#, &product()), json!({"price": "1.50"}));
}

/// A union of `Invoice`, whose `amount` is a `Decimal`, and `Rating`, whose is not.
fn union_shape(fallback: Option<&str>) -> DecimalShape {
    shape(
        DecimalNode::Abstract(fallback.map(str::to_string)),
        &[("Invoice", &[("amount", decimal())])],
    )
}

#[test]
fn an_abstract_object_is_quoted_by_the_type_it_names_wherever_the_name_sits() {
    assert_eq!(
        parsed(r#"{"amount": 1.50, "__typename": "Invoice"}"#, &union_shape(None)),
        json!({"amount": "1.50", "__typename": "Invoice"})
    );
    assert_eq!(
        parsed(r#"{"__typename": "Rating", "amount": 1.50}"#, &union_shape(Some("Invoice"))),
        json!({"__typename": "Rating", "amount": 1.5}),
        "the same key on a type where it is not a Decimal stays a number"
    );
}

#[test]
fn an_abstract_object_naming_no_type_reads_the_fallback() {
    assert_eq!(
        parsed(r#"{"amount": 1.50}"#, &union_shape(Some("Invoice"))),
        json!({"amount": "1.50"})
    );
    assert_eq!(parsed(r#"{"amount": 1.50}"#, &union_shape(None)), json!({"amount": 1.5}));
}

#[test]
fn a_list_of_abstract_objects_is_quoted_per_element() {
    let mut shape = union_shape(None);
    shape.root = object("Feed");
    Arc::make_mut(&mut shape.types).insert("Feed".to_string(), {
        let mut f = DecimalFields::new();
        f.insert("items", DecimalNode::Abstract(None));
        f
    });
    assert_eq!(
        parsed(
            r#"{"items": [{"__typename": "Invoice", "amount": 2.50}, {"__typename": "Rating", "amount": 2.50}]}"#,
            &shape
        ),
        json!({"items": [{"__typename": "Invoice", "amount": "2.50"}, {"__typename": "Rating", "amount": 2.5}]})
    );
}

#[test]
fn a_root_list_of_documents_is_quoted_per_document() {
    assert_eq!(
        parsed(r#"[{"price": 1.50}, {"price": 2.00}]"#, &product()),
        json!([{"price": "1.50"}, {"price": "2.00"}])
    );
}

#[test]
fn a_document_nested_past_any_schema_is_copied_without_recursing_into_it() {
    let deep_lists = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
    let deep_users = format!("{}{}", r#"{"friends": "#.repeat(10_000), "}".repeat(10_000));
    let users = shape(
        object("User"),
        &[("User", &[("balance", decimal()), ("friends", object("User"))])],
    );
    for (src, shape) in [
        (format!(r#"{{"other": {deep_lists}, "price": 1.50}}"#), product()),
        (format!(r#"{{"friends": {deep_lists}, "balance": 1.50}}"#), users.clone()),
        (format!(r#"{{"friends": {deep_users}, "balance": 1.50}}"#), users),
    ] {
        let out = quoted(&src, &shape);
        assert_eq!(out.replace('"', ""), src.replace('"', ""));
        assert!(out.ends_with(r#""1.50"}"#), "the member after the deep value is still read");
    }
    let src = format!(r#"{{"price": {deep_lists}}}"#);
    assert_eq!(quoted(&src, &product()), src, "a Decimal list holding no number is copied");
}

#[test]
fn a_truncated_text_is_copied_to_its_end_without_panicking() {
    let shape = shape(
        object("T"),
        &[
            ("T", &[("price", decimal()), ("o", object("O"))]),
            ("O", &[("p", decimal())]),
        ],
    );
    let src = r#"{"o": {"p": [1.5, {"x": "a\"b"}]}, "price": 1.50, "s": "x"}"#;
    for cut in 0..=src.len() {
        let text = &src[..cut];
        assert_eq!(quoted(text, &shape).replace('"', ""), text.replace('"', ""), "cut at {cut}");
    }
}

fn composed(src: &str, shape: &DecimalShape) -> Value {
    let mut out = Vec::new();
    super::quote_composed_decimals(src.as_bytes(), shape, &mut out);
    serde_json::from_slice(&out).unwrap()
}

/// A `Post` with a `price`, embedding its `comments`, each with a `tip`, and its `author`,
/// with a `balance`.
fn posts() -> DecimalShape {
    shape(
        object("Post"),
        &[
            (
                "Post",
                &[
                    ("price", decimal()),
                    ("comments", object("Comment")),
                    ("author", object("User")),
                ],
            ),
            ("Comment", &[("tip", decimal())]),
            ("User", &[("balance", decimal())]),
        ],
    )
}

#[test]
fn a_composed_row_is_read_level_by_level_through_its_embeds() {
    let row = r#"{"d": {"price": 1.50, "n": 1.50},
                  "e": {"comments": [{"d": {"tip": 0.10}, "e": {}}, {"d": {"tip": 2}}],
                        "author": {"d": {"balance": 100.00}, "e": null},
                        "other": {"d": {"tip": 1.5}}}}"#;
    assert_eq!(
        composed(row, &posts()),
        json!({"d": {"price": "1.50", "n": 1.5},
               "e": {"comments": [{"d": {"tip": "0.10"}, "e": {}}, {"d": {"tip": "2"}}],
                     "author": {"d": {"balance": "100.00"}, "e": null},
                     "other": {"d": {"tip": 1.5}}}})
    );
}

#[test]
fn a_composed_row_whose_document_comes_after_its_embeds_is_read_the_same() {
    let row = r#"{"e": {"author": {"e": {}, "d": {"balance": 1.10}}}, "d": {"price": 2.20}}"#;
    assert_eq!(
        composed(row, &posts()),
        json!({"e": {"author": {"e": {}, "d": {"balance": "1.10"}}}, "d": {"price": "2.20"}})
    );
}

#[test]
fn an_abstract_composed_row_embeds_by_the_type_its_document_names() {
    let mut shape = posts();
    shape.root = DecimalNode::Abstract(None);
    let row = r#"{"e": {"comments": [{"d": {"tip": 0.10}}]}, "d": {"__typename": "Post", "price": 1.50}}"#;
    assert_eq!(
        composed(row, &shape),
        json!({"e": {"comments": [{"d": {"tip": "0.10"}}]}, "d": {"__typename": "Post", "price": "1.50"}})
    );
}

fn cascade(src: &str, shape: &DecimalShape) -> Value {
    let mut out = Vec::new();
    quote_cascade_decimals(src.as_bytes(), shape, &mut out);
    serde_json::from_slice(&out).unwrap()
}

#[test]
fn a_cascade_entity_is_read_as_the_type_its_entry_names() {
    let shape = shape(
        DecimalNode::Abstract(None),
        &[
            ("Product", &[("price", decimal())]),
            ("Order", &[("total", decimal())]),
        ],
    );
    let src = r#"{
        "updated": [
            {"entity": {"price": 1.50, "total": 2}, "__typename": "Product", "id": 1},
            {"__typename": "Order", "id": 2, "entity": {"price": 3, "total": 4.00}},
            {"__typename": "Tag", "id": 3, "entity": {"price": 5}},
            {"id": 4, "entity": {"price": 6}}
        ],
        "deleted": [{"__typename": "Product", "id": 5, "entity": {"price": 7}}],
        "metadata": {"affectedCount": 4}
    }"#;
    assert_eq!(
        cascade(src, &shape),
        json!({
            "updated": [
                {"entity": {"price": "1.50", "total": 2}, "__typename": "Product", "id": 1},
                {"__typename": "Order", "id": 2, "entity": {"price": 3, "total": "4.00"}},
                {"__typename": "Tag", "id": 3, "entity": {"price": 5}},
                {"id": 4, "entity": {"price": 6}}
            ],
            "deleted": [{"__typename": "Product", "id": 5, "entity": {"price": 7}}],
            "metadata": {"affectedCount": 4}
        })
    );
}

#[test]
fn what_is_not_a_cascade_is_copied_as_it_is() {
    for src in [
        "null",
        "[1.5]",
        r#"{"updated": {"price": 1.5}}"#,
        r#"{"updated": [1.5, null]}"#,
    ] {
        assert_eq!(
            cascade(src, &product()).to_string(),
            serde_json::from_str::<Value>(src).unwrap().to_string(),
            "{src}"
        );
    }
}

#[test]
fn a_write_reads_its_entity_and_its_error_as_the_type_the_row_stamps() {
    let types = Arc::new(
        [
            ("Product", ("price", decimal())),
            ("Fault", ("loss", decimal())),
        ]
        .into_iter()
        .map(|(name, (key, node))| {
            let mut fields = DecimalFields::new();
            fields.insert(key, node);
            (name.to_string(), fields)
        })
        .collect(),
    );
    let write = WriteDecimals::new(types)
        .with_entity(Some(object("Product")))
        .with_error(Some(object("Fault")))
        .with_result(Some(object("Product")));
    let root = |column: &str, stamp: Option<&str>| match write.column(column, stamp) {
        Some(WriteColumn::Document(shape)) => Some(shape.root().clone()),
        Some(WriteColumn::Cascade(_)) | None => None,
    };
    assert_eq!(root("entity", None), Some(object("Product")));
    assert_eq!(root("entity", Some("Fault")), Some(object("Fault")));
    assert_eq!(root("error_detail", None), Some(object("Fault")));
    assert_eq!(root("error_detail", Some("Product")), Some(object("Product")));
    assert_eq!(
        root("result", Some("Fault")),
        Some(object("Product")),
        "a payload is not stamped"
    );
    assert_eq!(root("metadata", None), None, "observability is never read for Decimals");
    assert!(matches!(write.column("cascade", None), Some(WriteColumn::Cascade(_))));

    let unknown = WriteDecimals::new(Arc::default());
    assert_eq!(unknown.column("entity", None), None, "no type to read it as");
    assert_eq!(unknown.column("result", None), None);
}
