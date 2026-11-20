#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use serde_json::json;

use super::*;
use crate::schema::{FieldDefinition, FieldType, QueryDefinition, TypeDefinition};

fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut product = TypeDefinition::new("Product", "v_product");
    product.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::new("name", FieldType::String),
        FieldDefinition::nullable("note", FieldType::String),
    ];
    schema.types.push(product);
    schema
        .queries
        .push(QueryDefinition::new("products", "Product").returning_list());
    schema
}

fn sel(name: &str, nested: Vec<FieldSelection>) -> FieldSelection {
    FieldSelection {
        name:          name.to_string(),
        alias:         None,
        arguments:     vec![],
        nested_fields: nested,
        directives:    vec![],
    }
}

#[test]
fn a_missing_non_null_key_in_a_non_null_list_nulls_data() {
    let types = OutputTypes::from_schema(&schema());
    let mut response = json!({"data": {"products": [{"id": "1", "name": "a"}, {"id": "2"}]}});
    let selections = [sel(
        "products",
        vec![sel("id", vec![]), sel("name", vec![])],
    )];
    types.complete(&mut response, "Query", &selections, None);
    assert_eq!(response["data"], Value::Null, "`[Product!]!` under the root");
    assert_eq!(response["errors"][0]["path"], json!(["products", 1, "name"]));
}

#[test]
fn every_violation_is_reported_before_the_list_is_nulled() {
    let types = OutputTypes::from_schema(&schema());
    let mut response = json!({"data": {"products": [{"id": "1"}, {"id": "2"}]}});
    let selections = [sel("products", vec![sel("name", vec![])])];
    types.complete(&mut response, "Query", &selections, None);
    assert_eq!(response["errors"].as_array().map(Vec::len), Some(2), "{response}");
}

#[test]
fn an_absent_nullable_field_is_left_absent() {
    let types = OutputTypes::from_schema(&schema());
    let mut response = json!({"data": {"products": [{"id": "1", "name": "a"}]}});
    let selections = [sel(
        "products",
        vec![sel("name", vec![]), sel("note", vec![])],
    )];
    types.complete(&mut response, "Query", &selections, None);
    assert_eq!(response, json!({"data": {"products": [{"id": "1", "name": "a"}]}}));
}

#[test]
fn a_type_introspection_does_not_publish_is_passed_through() {
    let types = OutputTypes::from_schema(&schema());
    let mut response = json!({"data": {"unknown": [{"name": null}]}});
    let selections = [sel("unknown", vec![sel("name", vec![])])];
    types.complete(&mut response, "Query", &selections, None);
    assert_eq!(response, json!({"data": {"unknown": [{"name": null}]}}));
}

#[test]
fn completion_keeps_the_documents_field_order() {
    let types = OutputTypes::from_schema(&schema());
    let mut response = json!({"data": {"products": [{"name": "a", "id": "1", "note": "n"}]}});
    let selections = [sel(
        "products",
        vec![sel("name", vec![]), sel("id", vec![]), sel("note", vec![])],
    )];
    types.complete(&mut response, "Query", &selections, None);
    let keys: Vec<&String> = response["data"]["products"][0].as_object().unwrap().keys().collect();
    assert_eq!(keys, ["name", "id", "note"], "{response}");
}

#[test]
fn a_violation_inside_a_matching_inline_fragment_is_reported() {
    let types = OutputTypes::from_schema(&schema());
    let mut response = json!({"data": {"products": [{"id": "1"}]}});
    let selections = [sel(
        "products",
        vec![
            sel("id", vec![]),
            sel("...on Product", vec![sel("name", vec![])]),
        ],
    )];
    types.complete(&mut response, "Query", &selections, None);
    assert_eq!(response["errors"][0]["path"], json!(["products", 0, "name"]), "{response}");
}

/// § 3.5.1 at its bounds: an integer in the signed 32-bit range is served, one past it is
/// refused; a whole number written with a fraction or an exponent is that integer; a
/// fractional one is refused; a numeric string is that integer only in its canonical form.
#[test]
fn an_int_position_serves_a_32_bit_integer_coerces_a_whole_decimal_and_refuses_the_rest() {
    let whole = |n: i32| Coercion::Rewrite(json!(n));
    let cases = [
        (json!(0), Coercion::Served),
        (json!(2_147_483_647), Coercion::Served),
        (json!(-2_147_483_648), Coercion::Served),
        (json!(2_147_483_648_i64), Coercion::Refused),
        (json!(-2_147_483_649_i64), Coercion::Refused),
        (json!(u64::MAX), Coercion::Refused),
        (json!(3.0), whole(3)),
        (json!(-0.0), whole(0)),
        (serde_json::from_str::<Value>("1e3").unwrap(), whole(1000)),
        (json!(2_147_483_647.0), whole(2_147_483_647)),
        (json!(-2_147_483_648.0), whole(-2_147_483_648)),
        (json!(2_147_483_648.0), Coercion::Refused),
        (json!(-2_147_483_649.0), Coercion::Refused),
        (json!(2.5), Coercion::Refused),
        (json!(1.5e300), Coercion::Refused),
        (json!("2147483647"), whole(2_147_483_647)),
        (json!("-2147483648"), whole(-2_147_483_648)),
        (json!("2147483648"), Coercion::Refused),
        (json!("3000000000"), Coercion::Refused),
        (json!("-0"), Coercion::Refused),
        (json!("00"), Coercion::Refused),
        (json!("-"), Coercion::Refused),
        (json!(""), Coercion::Refused),
        (json!("４２"), Coercion::Refused),
        (json!(true), Coercion::Refused),
        (json!([1]), Coercion::Refused),
        (json!({"n": 1}), Coercion::Refused),
        (json!(null), Coercion::Served),
    ];
    for (value, expected) in cases {
        assert_eq!(coerce_scalar("Int", &value), Some(expected), "{value}");
    }
}

/// The scalars § 3.5's rule leaves alone: the extended built-ins and a project's own.
#[test]
fn a_scalar_outside_the_five_is_not_adjudicated() {
    for scalar in ["UUID", "Decimal", "DateTime", "LocalDateTime", "Email"] {
        assert_eq!(coerce_scalar(scalar, &json!(true)), None, "{scalar}");
    }
}

/// An `ID` is served as a string (§ 3.5.5): a string as stored, an integer as its decimal
/// text, exact past `i64`; a fraction, a Boolean, an object or a list is refused.
#[test]
fn an_id_is_a_string_and_an_integer_its_decimal_text() {
    let rewritten = |text: &str| Some(Coercion::Rewrite(json!(text)));
    for (stored, served) in [
        (json!("u1"), Some(Coercion::Served)),
        (json!(""), Some(Coercion::Served)),
        (json!(null), Some(Coercion::Served)),
        (json!(7), rewritten("7")),
        (json!(-7), rewritten("-7")),
        (json!(12_345_678_901_234_567_890_u64), rewritten("12345678901234567890")),
        (json!(1.5), Some(Coercion::Refused)),
        (json!(3.0), Some(Coercion::Refused)),
        (json!(true), Some(Coercion::Refused)),
        (json!({"id": 7}), Some(Coercion::Refused)),
        (json!(["7"]), Some(Coercion::Refused)),
    ] {
        assert_eq!(coerce_scalar("ID", &stored), served, "{stored}");
    }
}

/// A numeric string at a `Float` is the number its JSON syntax writes, with nothing around
/// it and only when a `Float` can hold it.
#[test]
fn a_float_position_reads_a_numeric_string_in_json_syntax_only() {
    for (text, served) in [
        ("1.5", "1.5"),
        ("42", "42"),
        ("-0", "-0.0"),
        ("4.2e1", "42.0"),
    ] {
        let rewritten = match coerce_scalar("Float", &json!(text)) {
            Some(Coercion::Rewrite(value)) => value.to_string(),
            other => format!("{other:?}"),
        };
        assert_eq!(rewritten, served, "{text}");
    }
    for text in [
        "", " 1", "1 ", "+1", "01", ".5", "1.", "NaN", "Infinity", "1e400", "0x10",
    ] {
        assert_eq!(coerce_scalar("Float", &json!(text)), Some(Coercion::Refused), "{text}");
    }
}

/// A refusal's message names the field, the scalar and the stored value's kind, never the
/// value.
#[test]
fn a_refusal_names_the_kind_of_the_stored_value_and_never_the_value() {
    let message = |scalar: &str, stored: Value| {
        not_a_scalar(&"Kind.at", scalar, &stored, &[json!("at")])["message"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(
        message("Int", json!("s3cr3t")),
        "`Kind.at` is an `Int`, a signed 32-bit integer, and the stored value is a String that \
         is not one"
    );
    assert_eq!(
        message("Boolean", json!(3_000_000_000_u64)),
        "`Kind.at` is a `Boolean`, and the stored value is a number that is not one"
    );
    for (stored, kind) in [
        (json!(true), "a Boolean"),
        (json!([1]), "a list"),
        (json!({"s": 1}), "an object"),
    ] {
        assert!(message("String", stored).contains(kind), "{kind}");
    }
}

/// Aggregate rows are projected by the same `Int` rule (§ 3.5.1) before the response is
/// completed: a whole decimal is its integer, a fraction or a value past the range a field
/// error at its row's path.
#[test]
fn an_aggregate_row_projects_an_int_by_the_same_rule() {
    let mut schema = CompiledSchema::new();
    let mut row = TypeDefinition::new("CounterRow", "v_counter");
    row.fields = vec![FieldDefinition::nullable("n", FieldType::Int)];
    schema.types.push(row);
    schema.build_indexes();
    let types = OutputTypes::from_schema(&schema);
    let n = FieldSelection {
        name:          "n".to_string(),
        alias:         None,
        arguments:     Vec::new(),
        nested_fields: Vec::new(),
        directives:    Vec::new(),
    };

    let (rows, errors) = types.project_rows(
        vec![
            json!({"n": 3.0}),
            json!({"n": 2.5}),
            json!({"n": 3_000_000_000_i64}),
            json!({"n": 7}),
        ],
        "CounterRow",
        &[n],
        None,
        "c",
    );

    assert_eq!(rows, json!([{"n": 3}, {"n": null}, {"n": null}, {"n": 7}]));
    assert!(rows[0]["n"].is_i64(), "{rows}");
    let paths: Vec<&Value> = errors.iter().map(|e| &e["path"]).collect();
    assert_eq!(paths, vec![&json!(["c", 1, "n"]), &json!(["c", 2, "n"])], "{errors:?}");
}

/// An `ID` refusal reads as English and names the stored kind, never the value.
#[test]
fn an_id_refusal_names_the_scalar_and_the_stored_kind() {
    let error = not_a_scalar(&"Note.id", "ID", &json!({"id": 7}), &[]);
    let message = error["message"].as_str().unwrap();
    assert!(
        message.starts_with("`Note.id` is an `ID`, and the stored value is an object"),
        "{message}"
    );
    assert!(!message.contains('7'), "{message}");
}
