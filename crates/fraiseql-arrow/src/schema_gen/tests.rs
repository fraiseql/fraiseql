#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use super::*;

#[test]
fn test_graphql_to_arrow_scalars() {
    assert_eq!(graphql_type_to_arrow("String", false), DataType::Utf8);
    assert_eq!(graphql_type_to_arrow("Int", false), DataType::Int32);
    assert_eq!(graphql_type_to_arrow("Float", false), DataType::Float64);
    assert_eq!(graphql_type_to_arrow("Boolean", false), DataType::Boolean);
    assert_eq!(graphql_type_to_arrow("ID", false), DataType::Utf8);
}

#[test]
fn test_graphql_to_arrow_custom_scalars() {
    assert_eq!(graphql_type_to_arrow("UUID", false), DataType::Utf8);
    assert_eq!(graphql_type_to_arrow("JSON", false), DataType::Utf8);
    assert_eq!(graphql_type_to_arrow("Date", false), DataType::Date32);
    assert_eq!(graphql_type_to_arrow("Time", false), DataType::Time64(TimeUnit::Nanosecond));
    assert_eq!(graphql_type_to_arrow("Decimal", false), DataType::Decimal128(38, 10));
}

#[test]
fn test_datetime_mapping() {
    let dt_type = graphql_type_to_arrow("DateTime", false);
    match dt_type {
        DataType::Timestamp(TimeUnit::Nanosecond, Some(tz)) => {
            assert_eq!(tz.as_ref(), "UTC");
        },
        _ => panic!("Expected Timestamp(Nanosecond, UTC), got {:?}", dt_type),
    }
}

#[test]
fn test_unknown_type_defaults_to_string() {
    assert_eq!(graphql_type_to_arrow("UnknownCustomType", false), DataType::Utf8);
}

#[test]
fn test_generate_arrow_schema() {
    let fields = vec![
        ("id".to_string(), "ID".to_string(), false),
        ("name".to_string(), "String".to_string(), true),
        ("age".to_string(), "Int".to_string(), true),
    ];

    let schema = generate_arrow_schema(&fields);

    assert_eq!(schema.fields().len(), 3);

    assert_eq!(schema.field(0).name(), "id");
    assert_eq!(schema.field(0).data_type(), &DataType::Utf8);
    assert!(!schema.field(0).is_nullable());

    assert_eq!(schema.field(1).name(), "name");
    assert_eq!(schema.field(1).data_type(), &DataType::Utf8);
    assert!(schema.field(1).is_nullable());

    assert_eq!(schema.field(2).name(), "age");
    assert_eq!(schema.field(2).data_type(), &DataType::Int32);
    assert!(schema.field(2).is_nullable());
}

#[test]
fn test_generate_schema_with_datetime() {
    let fields = vec![
        ("created_at".to_string(), "DateTime".to_string(), false),
        ("updated_at".to_string(), "DateTime".to_string(), true),
    ];

    let schema = generate_arrow_schema(&fields);

    assert_eq!(schema.fields().len(), 2);
    assert!(!schema.field(0).is_nullable());
    assert!(schema.field(1).is_nullable());

    match schema.field(0).data_type() {
        DataType::Timestamp(TimeUnit::Nanosecond, Some(tz)) => {
            assert_eq!(tz.as_ref(), "UTC");
        },
        _ => panic!("Expected Timestamp type"),
    }
}

#[test]
fn test_empty_schema() {
    let fields: Vec<(String, String, bool)> = vec![];
    let schema = generate_arrow_schema(&fields);
    assert_eq!(schema.fields().len(), 0);
}

// --- Additional field mapping tests ---

#[test]
fn test_non_nullable_int_field_maps_to_required_int32() {
    let fields = vec![("count".to_string(), "Int".to_string(), false)];
    let schema = generate_arrow_schema(&fields);
    assert_eq!(schema.field(0).data_type(), &DataType::Int32);
    assert!(!schema.field(0).is_nullable());
}

#[test]
fn test_nullable_string_field_maps_to_nullable_utf8() {
    let fields = vec![("description".to_string(), "String".to_string(), true)];
    let schema = generate_arrow_schema(&fields);
    assert_eq!(schema.field(0).data_type(), &DataType::Utf8);
    assert!(schema.field(0).is_nullable());
}

#[test]
fn test_non_nullable_boolean_maps_to_required_boolean() {
    let fields = vec![("active".to_string(), "Boolean".to_string(), false)];
    let schema = generate_arrow_schema(&fields);
    assert_eq!(schema.field(0).data_type(), &DataType::Boolean);
    assert!(!schema.field(0).is_nullable());
}

#[test]
fn test_float_scalar_maps_to_float64() {
    let fields = vec![("price".to_string(), "Float".to_string(), true)];
    let schema = generate_arrow_schema(&fields);
    assert_eq!(schema.field(0).data_type(), &DataType::Float64);
}

#[test]
fn test_id_scalar_maps_to_utf8() {
    let fields = vec![("id".to_string(), "ID".to_string(), false)];
    let schema = generate_arrow_schema(&fields);
    assert_eq!(schema.field(0).data_type(), &DataType::Utf8);
    assert!(!schema.field(0).is_nullable());
}

#[test]
fn test_datetime_scalar_maps_to_timestamp_microsecond_utc() {
    let fields = vec![("created_at".to_string(), "DateTime".to_string(), false)];
    let schema = generate_arrow_schema(&fields);
    match schema.field(0).data_type() {
        DataType::Timestamp(TimeUnit::Nanosecond, Some(tz)) => {
            assert_eq!(tz.as_ref(), "UTC");
        },
        other => panic!("Expected Timestamp(Nanosecond, UTC), got {:?}", other),
    }
}

#[test]
fn test_date_scalar_maps_to_date32() {
    let fields = vec![("birth_date".to_string(), "Date".to_string(), false)];
    let schema = generate_arrow_schema(&fields);
    assert_eq!(schema.field(0).data_type(), &DataType::Date32);
}

#[test]
fn test_uuid_scalar_maps_to_utf8() {
    let fields = vec![("user_uuid".to_string(), "UUID".to_string(), false)];
    let schema = generate_arrow_schema(&fields);
    assert_eq!(schema.field(0).data_type(), &DataType::Utf8);
}

#[test]
fn test_json_scalar_maps_to_utf8() {
    let fields = vec![("metadata".to_string(), "JSON".to_string(), true)];
    let schema = generate_arrow_schema(&fields);
    assert_eq!(schema.field(0).data_type(), &DataType::Utf8);
}

#[test]
fn test_decimal_scalar_maps_to_decimal128() {
    let fields = vec![("amount".to_string(), "Decimal".to_string(), false)];
    let schema = generate_arrow_schema(&fields);
    assert_eq!(schema.field(0).data_type(), &DataType::Decimal128(38, 10));
}

#[test]
fn test_unknown_scalar_type_falls_back_to_utf8() {
    let fields = vec![("custom".to_string(), "MyCustomScalar".to_string(), true)];
    let schema = generate_arrow_schema(&fields);
    assert_eq!(schema.field(0).data_type(), &DataType::Utf8);
}

#[test]
fn test_schema_with_one_field() {
    let fields = vec![("only".to_string(), "Int".to_string(), false)];
    let schema = generate_arrow_schema(&fields);
    assert_eq!(schema.fields().len(), 1);
    assert_eq!(schema.field(0).name(), "only");
}

#[test]
fn test_schema_determinism_same_input_same_output() {
    let fields = vec![
        ("id".to_string(), "ID".to_string(), false),
        ("name".to_string(), "String".to_string(), true),
        ("score".to_string(), "Float".to_string(), true),
        ("active".to_string(), "Boolean".to_string(), false),
    ];
    let schema1 = generate_arrow_schema(&fields);
    let schema2 = generate_arrow_schema(&fields);
    assert_eq!(schema1.fields().len(), schema2.fields().len());
    for (f1, f2) in schema1.fields().iter().zip(schema2.fields().iter()) {
        assert_eq!(f1.name(), f2.name());
        assert_eq!(f1.data_type(), f2.data_type());
        assert_eq!(f1.is_nullable(), f2.is_nullable());
    }
}

#[test]
fn test_nullable_flag_propagated_correctly() {
    let fields = vec![
        ("required_field".to_string(), "String".to_string(), false),
        ("optional_field".to_string(), "String".to_string(), true),
    ];
    let schema = generate_arrow_schema(&fields);
    assert!(!schema.field(0).is_nullable(), "required_field must not be nullable");
    assert!(schema.field(1).is_nullable(), "optional_field must be nullable");
}

#[test]
fn test_field_names_are_preserved() {
    let fields = vec![
        ("user_id".to_string(), "ID".to_string(), false),
        ("email_address".to_string(), "String".to_string(), true),
        ("created_at_timestamp".to_string(), "DateTime".to_string(), false),
    ];
    let schema = generate_arrow_schema(&fields);
    assert_eq!(schema.field(0).name(), "user_id");
    assert_eq!(schema.field(1).name(), "email_address");
    assert_eq!(schema.field(2).name(), "created_at_timestamp");
}

#[test]
fn test_infer_schema_from_empty_rows_returns_error() {
    let rows: Vec<HashMap<String, Value>> = vec![];
    let result = infer_schema_from_rows(&rows);
    assert!(
        matches!(result, Err(crate::error::ArrowFlightError::SchemaNotFound(_))),
        "expected SchemaNotFound, got: {result:?}"
    );
}

#[test]
fn test_infer_schema_from_rows_with_integer() {
    let mut row = HashMap::new();
    row.insert("count".to_string(), Value::from(42i64));
    let rows = vec![row];
    let schema = infer_schema_from_rows(&rows).unwrap();
    assert_eq!(schema.fields().len(), 1);
    let field = schema.field(0);
    assert_eq!(field.name(), "count");
    assert_eq!(field.data_type(), &DataType::Int64);
}

#[test]
fn test_infer_schema_from_rows_with_float() {
    let mut row = HashMap::new();
    row.insert("price".to_string(), Value::from(9.99f64));
    let rows = vec![row];
    let schema = infer_schema_from_rows(&rows).unwrap();
    let field = schema.field(0);
    assert_eq!(field.data_type(), &DataType::Float64);
}

#[test]
fn test_infer_schema_from_rows_with_string() {
    let mut row = HashMap::new();
    row.insert("name".to_string(), Value::from("Alice"));
    let rows = vec![row];
    let schema = infer_schema_from_rows(&rows).unwrap();
    let field = schema.field(0);
    assert_eq!(field.data_type(), &DataType::Utf8);
}

/// #1180, inverted. This test used to assert that the field *set* came from row
/// 0 alone — `assert_eq!(schema.fields().len(), 1)` — which is the silent drop
/// itself, pinned deliberately so the choice would be explicit rather than
/// incidental. It is now the rule the other way round: the schema is the union
/// of every row's keys.
#[test]
fn test_infer_schema_from_rows_unions_keys_across_rows() {
    let mut row1 = HashMap::new();
    row1.insert("id".to_string(), Value::from(1i64));

    let mut row2 = HashMap::new();
    row2.insert("extra_column".to_string(), Value::from("extra"));

    let rows = vec![row1, row2];
    let schema = infer_schema_from_rows(&rows).unwrap();
    assert_eq!(schema.fields().len(), 2, "a key seen in any row is a column");
    // Sorted, so the field order does not depend on `HashMap` iteration order.
    assert_eq!(schema.field(0).name(), "extra_column");
    assert_eq!(schema.field(1).name(), "id");
}

#[test]
fn test_infer_schema_all_fields_are_nullable() {
    let mut row = HashMap::new();
    row.insert("a".to_string(), Value::from(1i64));
    row.insert("b".to_string(), Value::from("test"));
    let rows = vec![row];
    let schema = infer_schema_from_rows(&rows).unwrap();
    for field in schema.fields() {
        assert!(field.is_nullable(), "inferred fields must be nullable");
    }
}

#[test]
fn test_infer_schema_from_rows_null_value_gives_utf8() {
    // Previously asserted `DataType::Null` — codifying the H37 bug. A null
    // value must infer a nullable Utf8 column, which the array converters
    // accept (DataType::Null would be rejected and poison the result).
    let mut row = HashMap::new();
    row.insert("unknown".to_string(), Value::Null);
    let rows = vec![row];
    let schema = infer_schema_from_rows(&rows).unwrap();
    assert_eq!(schema.field(0).data_type(), &DataType::Utf8);
}

#[test]
fn test_infer_schema_from_rows_array_value_gives_utf8() {
    let mut row = HashMap::new();
    row.insert("tags".to_string(), Value::Array(vec![Value::from("a"), Value::from("b")]));
    let rows = vec![row];
    let schema = infer_schema_from_rows(&rows).unwrap();
    assert_eq!(schema.field(0).data_type(), &DataType::Utf8);
}

#[test]
fn test_infer_schema_from_rows_object_value_gives_utf8() {
    use serde_json::json;
    let mut row = HashMap::new();
    row.insert("meta".to_string(), json!({"key": "value"}));
    let rows = vec![row];
    let schema = infer_schema_from_rows(&rows).unwrap();
    assert_eq!(schema.field(0).data_type(), &DataType::Utf8);
}

// ── H37: JSON null must infer Utf8, not DataType::Null ────────────────────────

// The array converters reject `DataType::Null`; a column whose first row was
// JSON null poisoned the whole result. Inference must match `metadata.rs`
// semantics (null → nullable Utf8 column).
#[test]
fn test_infer_schema_from_rows_null_value_gives_utf8_not_null() {
    let mut row = HashMap::new();
    row.insert("maybe".to_string(), Value::Null);
    let rows = vec![row];
    let schema = infer_schema_from_rows(&rows).unwrap();
    assert_eq!(
        schema.field(0).data_type(),
        &DataType::Utf8,
        "JSON null must infer Utf8, not DataType::Null"
    );
}

// The shared single-source-of-truth helper that both `schema_gen` and
// `metadata` now route through.
#[test]
fn test_json_value_to_arrow_type_covers_all_json_shapes() {
    use serde_json::json;
    assert_eq!(json_value_to_arrow_type(&Value::Null), DataType::Utf8);
    assert_eq!(json_value_to_arrow_type(&json!(true)), DataType::Boolean);
    assert_eq!(json_value_to_arrow_type(&json!(7)), DataType::Int64);
    assert_eq!(json_value_to_arrow_type(&json!(7.5)), DataType::Float64);
    assert_eq!(json_value_to_arrow_type(&json!("s")), DataType::Utf8);
    assert_eq!(json_value_to_arrow_type(&json!(["a"])), DataType::Utf8);
    assert_eq!(json_value_to_arrow_type(&json!({"k": 1})), DataType::Utf8);
}

// ---------------------------------------------------------------------------
// #1002 (column order) and #1042 (row-0 typing) — both in `infer_schema_from_rows`
// ---------------------------------------------------------------------------

/// Build a row from `(name, value)` pairs.
fn row(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, Value> {
    pairs.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect()
}

fn field_names(schema: &Schema) -> Vec<String> {
    schema.fields().iter().map(|f| f.name().clone()).collect()
}

fn type_of(schema: &Schema, name: &str) -> DataType {
    schema.field_with_name(name).unwrap().data_type().clone()
}

/// #1002 — the inferred field order came from `HashMap::iter()`, which is
/// unspecified and differs between map instances. The #717 heterogeneous-schema
/// guard compares whole `Schema` values and `Schema` equality is order-sensitive,
/// so two batched queries with an identical column set were refused
/// nondeterministically. Enough columns here that an accidental sorted order is
/// not a plausible false pass.
#[test]
fn inferred_field_order_is_deterministic() {
    use serde_json::json;

    let schema = infer_schema_from_rows(&[row(&[
        ("name", json!("x")),
        ("id", json!(1)),
        ("email", json!("e")),
        ("created_at", json!("t")),
        ("total", json!(1.5)),
        ("active", json!(true)),
        ("zip", json!("z")),
        ("age", json!(3)),
    ])])
    .unwrap();

    assert_eq!(
        field_names(&schema),
        [
            "active",
            "age",
            "created_at",
            "email",
            "id",
            "name",
            "total",
            "zip"
        ],
        "inferred fields must be in a deterministic order"
    );
}

/// The same column set must infer the same schema regardless of which map
/// instance it arrives in — this is the property the #717 guard depends on.
#[test]
fn identical_column_sets_infer_equal_schemas() {
    use serde_json::json;

    let first = infer_schema_from_rows(&[row(&[
        ("id", json!(1)),
        ("name", json!("a")),
        ("email", json!("e")),
        ("created_at", json!("t")),
    ])])
    .unwrap();

    let second = infer_schema_from_rows(&[row(&[
        ("created_at", json!("t")),
        ("email", json!("e")),
        ("name", json!("b")),
        ("id", json!(2)),
    ])])
    .unwrap();

    assert_eq!(first, second, "identical column sets must produce equal schemas");
}

/// #1042 — a leading NULL typed the whole column `Utf8`, and every later number
/// was then stringified by the `Utf8` catch-all. The column type flipped between
/// otherwise-identical requests depending on which row came back first.
#[test]
fn leading_null_does_not_retype_a_numeric_column_as_string() {
    use serde_json::json;

    let schema = infer_schema_from_rows(&[
        row(&[("total", json!(null))]),
        row(&[("total", json!(99.99))]),
    ])
    .unwrap();

    assert_eq!(type_of(&schema, "total"), DataType::Float64);
}

/// The mirror case failed loudly: row 0 typed `Int64`, and the first later
/// fractional value killed the whole request with
/// `Conversion("Cannot convert 100.5 to Int64")`. Widening to `Float64` converts
/// both, because the `Float64` arm accepts integer JSON numbers.
#[test]
fn whole_number_then_fractional_widens_to_float64() {
    use serde_json::json;

    let schema = infer_schema_from_rows(&[
        row(&[("total", json!(100))]),
        row(&[("total", json!(100.5))]),
    ])
    .unwrap();

    assert_eq!(type_of(&schema, "total"), DataType::Float64);
}

/// A column of whole numbers stays `Int64` — widening must not blanket-promote.
#[test]
fn all_whole_numbers_stay_int64() {
    use serde_json::json;

    let schema =
        infer_schema_from_rows(&[row(&[("n", json!(1))]), row(&[("n", json!(2))])]).unwrap();

    assert_eq!(type_of(&schema, "n"), DataType::Int64);
}

/// An all-null column stays `Utf8`. `DataType::Null` is rejected by the array
/// converters, so a null column previously poisoned the whole result (H37) — that
/// fix must survive this one.
#[test]
fn all_null_column_stays_utf8() {
    use serde_json::json;

    let schema = infer_schema_from_rows(&[
        row(&[("maybe", json!(null))]),
        row(&[("maybe", json!(null))]),
    ])
    .unwrap();

    assert_eq!(type_of(&schema, "maybe"), DataType::Utf8);
}

/// Genuinely mixed types fall back to `Utf8`, which every value can be rendered
/// as, rather than failing the request.
#[test]
fn mixed_string_and_number_falls_back_to_utf8() {
    use serde_json::json;

    let schema =
        infer_schema_from_rows(&[row(&[("mixed", json!(1))]), row(&[("mixed", json!("one"))])])
            .unwrap();

    assert_eq!(type_of(&schema, "mixed"), DataType::Utf8);
}

/// A boolean column keeps its type across rows.
#[test]
fn boolean_column_stays_boolean() {
    use serde_json::json;

    let schema = infer_schema_from_rows(&[
        row(&[("flag", json!(true))]),
        row(&[("flag", json!(false))]),
    ])
    .unwrap();

    assert_eq!(type_of(&schema, "flag"), DataType::Boolean);
}

/// #1180: the field *set* is the union of every row's keys, as the column
/// *types* have been since #1042. A key that first appears in a later row used
/// to be dropped from the schema — and `convert_db_rows_to_arrow` looks columns
/// up **by name**, so a key that never became a field was never read: its values
/// did not reach the client and nothing was logged. The stream was
/// self-consistent, so no error surfaced; the data was simply absent.
#[test]
fn the_field_set_is_the_union_of_every_rows_keys() {
    use serde_json::json;

    let schema = infer_schema_from_rows(&[
        row(&[("id", json!(1))]),
        row(&[("id", json!(2)), ("late", json!("v"))]),
    ])
    .unwrap();

    assert_eq!(field_names(&schema), ["id", "late"]);
    assert_eq!(type_of(&schema, "late"), DataType::Utf8, "typed from the row that has it");
}

/// The consequence the schema assertion above only implies: the **values** of a
/// late-appearing column reach the caller, and the row that lacks it reads null
/// rather than failing.
#[test]
fn a_late_appearing_columns_values_survive_conversion() {
    use serde_json::json;

    use crate::db_convert::convert_db_rows_to_arrow;

    let rows = [
        row(&[("id", json!(1))]),
        row(&[("id", json!(2)), ("late", json!("v"))]),
    ];
    let schema = infer_schema_from_rows(&rows).unwrap();
    let converted = convert_db_rows_to_arrow(&rows, &schema).unwrap();

    let late = schema.index_of("late").expect("`late` must be a column");
    assert!(
        converted[0][late].is_none(),
        "row 0 does not carry `late`; a nullable field is the honest rendering"
    );
    assert!(
        converted[1][late].is_some(),
        "row 1's `late` value must reach the caller — this is the silent drop (#1180)"
    );
}

/// **Control.** A column present in row 0 and absent later is unchanged — the
/// union must widen the set, never narrow it.
#[test]
fn a_column_missing_from_a_later_row_is_still_a_column() {
    use serde_json::json;

    let schema = infer_schema_from_rows(&[
        row(&[("id", json!(1)), ("early", json!("v"))]),
        row(&[("id", json!(2))]),
    ])
    .unwrap();

    assert_eq!(field_names(&schema), ["early", "id"]);
}
