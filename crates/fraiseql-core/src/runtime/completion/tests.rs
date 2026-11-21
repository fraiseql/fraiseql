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

/// The scalars § 3.5's rule leaves alone: the extended built-ins GraphQL serves as read,
/// and a project's own.
#[test]
fn a_scalar_outside_the_five_is_not_adjudicated() {
    for scalar in ["UUID", "Decimal", "Email"] {
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
        not_a_scalar(&"Kind.at", scalar_named(scalar).unwrap(), &stored, &[json!("at")])["message"]
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
    let error = not_a_scalar(&"Note.id", Scalar::Id, &json!({"id": 7}), &[]);
    let message = error["message"].as_str().unwrap();
    assert!(
        message.starts_with("`Note.id` is an `ID`, and the stored value is an object"),
        "{message}"
    );
    assert!(!message.contains('7'), "{message}");
}

/// `Reading`, one nullable field of every kind REST publishes a scalar as, read by
/// `readings`.
fn published_schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut reading = TypeDefinition::new("Reading", "v_reading");
    reading.fields = [
        ("count", FieldType::Int),
        ("ratio", FieldType::Float),
        ("on", FieldType::Boolean),
        ("label", FieldType::String),
        ("key", FieldType::Id),
        ("price", FieldType::Decimal),
        ("at", FieldType::DateTime),
        ("local", FieldType::LocalDateTime),
        ("day", FieldType::Date),
        ("clock", FieldType::Time),
        ("uid", FieldType::Uuid),
        ("doc", FieldType::Json),
        ("mail", FieldType::Scalar("Email".to_string())),
        ("bits", FieldType::BitVector),
    ]
    .into_iter()
    .map(|(name, field_type)| FieldDefinition::nullable(name, field_type))
    .collect();
    schema.types.push(reading);
    schema
        .queries
        .push(QueryDefinition::new("readings", "Reading").returning_list());
    schema.build_indexes();
    schema
}

/// What REST serves for `stored` under `field` of a `Reading`, or the message refusing it.
fn served_by_rest(field: &str, stored: &Value) -> std::result::Result<Value, String> {
    let types = OutputTypes::from_schema(&published_schema());
    let mut response = json!({"data": {"readings": [{field: stored}]}});
    let selections = [sel("readings", vec![sel(field, vec![])])];
    types.complete_as_published(&mut response, "Query", &selections, "readings", &[]);
    match response.get("errors") {
        Some(errors) => Err(errors[0]["message"].as_str().unwrap().to_string()),
        None => Ok(response["data"]["readings"][0][field].clone()),
    }
}

/// REST holds a stored value to the OpenAPI type it publishes the field as: an `integer`
/// of any size, a `number`, a `boolean`, by the lossless rewrites GraphQL makes; anything
/// else is refused. A `null` is served at every kind.
#[test]
fn rest_serves_an_integer_number_or_boolean_as_its_published_type_or_refuses_it() {
    let u64_max = json!(u64::MAX);
    let cases: [(&str, Value, Option<Value>); 32] = [
        ("count", json!(7), Some(json!(7))),
        ("count", json!(3_000_000_000_i64), Some(json!(3_000_000_000_i64))),
        ("count", u64_max.clone(), Some(u64_max)),
        ("count", json!("7"), Some(json!(7))),
        ("count", json!("-7"), Some(json!(-7))),
        ("count", json!("3000000000"), Some(json!(3_000_000_000_i64))),
        ("count", json!("18446744073709551615"), Some(json!(u64::MAX))),
        ("count", json!(3.0), Some(json!(3))),
        ("count", serde_json::from_str("1e3").unwrap(), Some(json!(1000))),
        ("count", json!(2.5), None),
        ("count", json!("18446744073709551616"), None),
        ("count", json!("07"), None),
        ("count", json!("7.0"), None),
        ("count", json!("+7"), None),
        ("count", json!(true), None),
        ("count", json!({"n": 7}), None),
        ("count", json!([7]), None),
        ("count", json!(null), Some(json!(null))),
        ("ratio", json!(2.5), Some(json!(2.5))),
        ("ratio", json!(7), Some(json!(7))),
        ("ratio", json!("2.5"), Some(json!(2.5))),
        ("ratio", json!("1e400"), None),
        ("ratio", json!("x"), None),
        ("ratio", json!(false), None),
        ("ratio", json!({"r": 1}), None),
        ("on", json!(true), Some(json!(true))),
        ("on", json!("true"), Some(json!(true))),
        ("on", json!("false"), Some(json!(false))),
        ("on", json!("TRUE"), None),
        ("on", json!(1), None),
        ("on", json!({"on": true}), None),
        ("on", json!(null), Some(json!(null))),
    ];
    for (field, stored, served) in cases {
        let got = served_by_rest(field, &stored).ok();
        assert_eq!(got, served, "{field} storing {stored}");
    }
    let past_i64 = json!(1e20);
    assert_eq!(served_by_rest("count", &past_i64), Ok(past_i64), "a whole number past `i64`");
    assert!(served_by_rest("count", &json!("3000000000")).unwrap().is_i64());
    assert!(served_by_rest("count", &json!("18446744073709551615")).unwrap().is_u64());
    assert!(served_by_rest("count", &json!(3.0)).unwrap().is_i64());
}

/// A `String` or an `ID` serves any other kind as its JSON text and never refuses (a
/// REST client has no errors document to read a refusal from). A field GraphQL publishes
/// as a `String` (a bit vector's text) is held alike.
#[test]
fn rest_serves_a_string_or_id_holding_another_kind_as_its_json_text() {
    for field in ["label", "key", "bits"] {
        for (stored, served) in [
            (json!("x"), json!("x")),
            (json!(7), json!("7")),
            (json!(1.5), json!("1.5")),
            (json!(true), json!("true")),
            (json!({"a": 1}), json!(r#"{"a":1}"#)),
            (json!([1]), json!("[1]")),
            (json!(null), json!(null)),
        ] {
            assert_eq!(served_by_rest(field, &stored), Ok(served), "{field} storing {stored}");
        }
    }
}

/// A `Decimal` is published as a `string` of its digits: a string is served as stored, an
/// integer as its exact text; a fraction that reached completion as a binary float is
/// refused, its digits being lost; any other kind is refused.
#[test]
fn rest_serves_a_decimal_as_its_exact_text_or_refuses_it() {
    for (stored, served) in [
        (json!("12.50"), Some(json!("12.50"))),
        (json!(7), Some(json!("7"))),
        (json!(-7), Some(json!("-7"))),
        (json!(u64::MAX), Some(json!("18446744073709551615"))),
        (json!(null), Some(json!(null))),
        (json!(12.5), None),
        (json!(true), None),
        (json!({"d": "1"}), None),
        (json!(["1"]), None),
    ] {
        assert_eq!(served_by_rest("price", &stored).ok(), served, "{stored}");
    }
}

/// A date, a time, a date-time with or without an offset and a UUID are published as a
/// `string`: any other kind is refused. A JSON field and a custom scalar are served as read.
#[test]
fn rest_refuses_a_non_string_at_a_temporal_or_uuid_field_and_serves_json_as_read() {
    assert_eq!(served_by_rest("uid", &json!("anything")), Ok(json!("anything")));
    for field in ["at", "local", "day", "clock", "uid"] {
        assert_eq!(served_by_rest(field, &json!(null)), Ok(json!(null)), "{field}");
        for stored in [json!(7), json!(true), json!({"t": 1}), json!(["t"])] {
            assert!(served_by_rest(field, &stored).is_err(), "{field} storing {stored}");
        }
    }
    for field in ["doc", "mail"] {
        for stored in [json!(7), json!(true), json!({"t": 1}), json!("t")] {
            assert_eq!(served_by_rest(field, &stored), Ok(stored.clone()), "{field}");
        }
    }
}

/// A REST refusal names the field, the OpenAPI type it is published as and the stored
/// value's kind, never the value.
#[test]
fn a_rest_refusal_names_the_published_type_and_never_the_value() {
    let refusal = |field: &str, stored: Value| served_by_rest(field, &stored).unwrap_err();
    assert_eq!(
        refusal("count", json!({"s3cr3t": "v4lue"})),
        "`Reading.count` is published as an OpenAPI `integer`, and the stored value is an \
         object that is not one"
    );
    assert_eq!(
        refusal("ratio", json!("s3cr3t")),
        "`Reading.ratio` is published as an OpenAPI `number`, and the stored value is a String \
         that is not one"
    );
    assert_eq!(
        refusal("on", json!(7)),
        "`Reading.on` is published as an OpenAPI `boolean`, and the stored value is a number \
         that is not one"
    );
    assert_eq!(
        refusal("price", json!(true)),
        "`Reading.price` is published as an OpenAPI `string`, and the stored value is a \
         Boolean that is not one"
    );
}

/// GraphQL completion is unchanged by REST's table: a scalar outside the five built-ins
/// is served as read there, whatever it holds.
#[test]
fn graphql_serves_a_decimal_or_date_holding_another_kind_as_read() {
    let types = OutputTypes::from_schema(&published_schema());
    let mut response = json!({"data": {"readings": [{"price": 12.5, "uid": true}]}});
    let selections = [sel(
        "readings",
        vec![sel("price", vec![]), sel("uid", vec![])],
    )];
    types.complete(&mut response, "Query", &selections, None);
    assert_eq!(response.get("errors"), None, "{response}");
    assert_eq!(response["data"]["readings"][0], json!({"price": 12.5, "uid": true}));
}

/// The values outside each temporal field's format, and the reason a GraphQL refusal
/// gives, completing "the stored value is …".
fn outside_the_format(field: &str) -> Vec<(Value, &'static str)> {
    let not_one = "a String that is not one";
    match field {
        "at" => vec![
            (
                json!("2026-07-01T12:00:00"),
                "a String that has no time-zone offset, which a DateTime always carries",
            ),
            (
                json!("2026-07-01T12:00:00-00:00"),
                "a String that carries the unknown offset -00:00, which a DateTime does not take",
            ),
            (json!("2026-07-01 12:00:00+00"), not_one),
            (json!("1890-01-01T00:00:00+00:09:21"), not_one),
            (json!("2026-07-01"), not_one),
            (json!(1_782_907_200), "a number that is not one"),
        ],
        "local" => vec![
            (
                json!("2026-07-01T12:00:00+00:00"),
                "a String that carries a time-zone offset, which a LocalDateTime never has",
            ),
            (
                json!("2026-07-01T12:00:00Z"),
                "a String that carries a time-zone offset, which a LocalDateTime never has",
            ),
            (json!("2026-07-01 12:00:00"), not_one),
            (json!(true), "a Boolean that is not one"),
        ],
        "day" => vec![
            (json!("2026-07-01T00:00:00"), not_one),
            (json!("2026-02-30"), not_one),
            (json!({"y": 2026}), "an object that is not one"),
        ],
        "clock" => vec![
            (
                json!("09:30:00+02:00"),
                "a String that carries a time-zone offset, which a Time never has",
            ),
            (json!("09:30"), not_one),
            (json!("24:00:00"), not_one),
            (json!(["09:30:00"]), "a list that is not one"),
        ],
        _ => Vec::new(),
    }
}

/// Each temporal field of a `Reading`, its scalar, and a value in its format: a
/// `timestamptz` and a `timestamp` column write the first two into a document.
const TEMPORAL_SCALARS: [(&str, &str, &str); 4] = [
    ("at", "DateTime", "2026-07-01T12:00:00+00:00"),
    ("local", "LocalDateTime", "2026-07-01T12:00:00"),
    ("day", "Date", "2026-07-01"),
    ("clock", "Time", "09:30:00.25"),
];

/// What GraphQL serves for `stored` under `field` of a `Reading`, or the refusal's
/// message and path.
fn served_by_graphql(field: &str, stored: &Value) -> std::result::Result<Value, (String, Value)> {
    let types = OutputTypes::from_schema(&published_schema());
    let mut response = json!({"data": {"readings": [{field: stored}]}});
    let selections = [sel("readings", vec![sel(field, vec![])])];
    types.complete(&mut response, "Query", &selections, None);
    match response.get("errors") {
        Some(errors) => {
            assert_eq!(response["data"]["readings"][0][field], Value::Null, "{response}");
            Err((errors[0]["message"].as_str().unwrap().to_string(), errors[0]["path"].clone()))
        },
        None => Ok(response["data"]["readings"][0][field].clone()),
    }
}

/// GraphQL serves a temporal value in its scalar's format, and refuses any other with a
/// field error that names the field, the scalar and why, never the value: a `DateTime`
/// written without an offset would be read by each client in the zone it assumes.
#[test]
fn graphql_refuses_a_temporal_value_outside_its_scalars_format() {
    for (field, scalar, good) in TEMPORAL_SCALARS {
        assert_eq!(served_by_graphql(field, &json!(good)), Ok(json!(good)), "{field}");
        assert_eq!(served_by_graphql(field, &Value::Null), Ok(Value::Null), "{field}");
        let outside = outside_the_format(field);
        assert!(!outside.is_empty(), "{field} has values outside its format");
        for (stored, why) in outside {
            let (message, path) = served_by_graphql(field, &stored).unwrap_err();
            assert_eq!(
                message,
                format!("`Reading.{field}` is a `{scalar}`, and the stored value is {why}"),
                "{field} storing {stored}"
            );
            assert_eq!(path, json!(["readings", 0, field]), "{field} storing {stored}");
            assert!(!message.contains("12:00") && !message.contains("09:30"), "{message}");
        }
    }
}

/// REST serves a temporal value in its scalar's format and refuses any other, as it
/// refuses a value of another kind: naming the field and the OpenAPI type, never the value.
#[test]
fn rest_refuses_a_temporal_value_outside_its_scalars_format() {
    for (field, _, good) in TEMPORAL_SCALARS {
        assert_eq!(served_by_rest(field, &json!(good)), Ok(json!(good)), "{field}");
        let outside = outside_the_format(field);
        assert!(!outside.is_empty(), "{field} has values outside its format");
        for (stored, _) in outside {
            let message = served_by_rest(field, &stored).unwrap_err();
            assert!(
                message.starts_with(&format!(
                    "`Reading.{field}` is published as an OpenAPI `string`, and the stored value is "
                )),
                "{field} storing {stored}: {message}"
            );
            assert!(!message.contains("12:00") && !message.contains("09:30"), "{message}");
        }
    }
}

/// A non-null temporal field holding a value outside its format is a field error that
/// nulls the nearest nullable position, as a missing value is.
#[test]
fn a_refused_temporal_value_at_a_non_null_field_propagates() {
    let mut schema = published_schema();
    let reading = schema.types.iter_mut().find(|t| t.name == "Reading").unwrap();
    reading.fields.push(FieldDefinition::new("since", FieldType::DateTime));
    schema.build_indexes();
    let types = OutputTypes::from_schema(&schema);
    let mut response = json!({"data": {"readings": [
        {"since": "2026-07-01T12:00:00Z"},
        {"since": "2026-07-01T12:00:00"},
    ]}});
    let selections = [sel("readings", vec![sel("since", vec![])])];
    types.complete(&mut response, "Query", &selections, None);
    assert_eq!(response["data"], Value::Null, "`[Reading!]!` under the root: {response}");
    assert_eq!(response["errors"][0]["path"], json!(["readings", 1, "since"]), "{response}");
}

/// Aggregate rows hold a temporal group key to its scalar's format, as a document's field
/// is: the keys a `timestamptz`, a `timestamp`, a `date` column write are served, and a
/// key outside its format is a field error at its row's path.
#[test]
fn an_aggregate_row_holds_a_temporal_key_to_its_format() {
    let types = OutputTypes::from_schema(&published_schema());
    let selections: Vec<FieldSelection> =
        ["at", "local", "day"].iter().map(|name| sel(name, vec![])).collect();
    let served = json!({"at": "2026-07-01T00:00:00+00:00", "local": "2026-07-01T00:00:00", "day": "2026-07-01"});
    let (rows, errors) = types.project_rows(
        vec![served.clone(), json!({"at": "2026-07-01T00:00:00", "local": "2026-07-01T00:00:00Z", "day": "2026-07-01"})],
        "Reading",
        &selections,
        None,
        "r",
    );
    assert_eq!(rows[0], served, "{errors:?}");
    assert_eq!(rows[1], json!({"at": null, "local": null, "day": "2026-07-01"}));
    let messages: Vec<&str> = errors.iter().map(|e| e["message"].as_str().unwrap()).collect();
    assert_eq!(
        messages,
        [
            "`Reading.at` is a `DateTime`, and the stored value is a String that has no \
             time-zone offset, which a DateTime always carries",
            "`Reading.local` is a `LocalDateTime`, and the stored value is a String that \
             carries a time-zone offset, which a LocalDateTime never has",
        ]
    );
    let paths: Vec<&Value> = errors.iter().map(|e| &e["path"]).collect();
    assert_eq!(paths, [&json!(["r", 1, "at"]), &json!(["r", 1, "local"])]);
}

/// A transport with no completion of its own asks `temporal_refusal`: it refuses exactly the
/// temporal values completion refuses, in completion's own words, and holds no other field.
#[test]
fn temporal_refusal_is_completions_rule_and_message() {
    let temporals = [
        FieldType::DateTime,
        FieldType::LocalDateTime,
        FieldType::Date,
        FieldType::Time,
    ];
    let stored = [
        json!("2024-01-15T10:00:00Z"),
        json!("2024-01-15T10:00:00"),
        json!("2024-01-15"),
        json!("10:00:00"),
        json!("noon"),
        json!(7),
        json!(true),
        json!({"at": 1}),
        Value::Null,
    ];
    for field_type in &temporals {
        let scalar = Scalar::of(field_type).unwrap();
        for value in &stored {
            let expected = matches!(coerce_built_in::<false>(scalar, value), Coercion::Refused)
                .then(|| {
                    not_a_scalar(&"Kind.at", scalar, value, &[])["message"]
                        .as_str()
                        .unwrap()
                        .to_string()
                });
            assert_eq!(
                temporal_refusal(&"Kind.at", field_type, value),
                expected,
                "{field_type:?} {value}"
            );
        }
    }
    assert!(
        temporal_refusal(&"Kind.at", &FieldType::DateTime, &json!("2024-01-15T10:00:00")).is_some(),
        "an offset-less DateTime is refused"
    );
    for field_type in [
        FieldType::String,
        FieldType::Int,
        FieldType::Id,
        FieldType::Json,
    ] {
        assert_eq!(temporal_refusal(&"Kind.at", &field_type, &json!("noon")), None);
    }
}
