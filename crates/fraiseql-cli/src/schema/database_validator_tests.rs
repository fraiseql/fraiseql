//! Unit tests for native-column detection (pure; no database required).

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::collections::HashMap;

use fraiseql_core::schema::FieldType;

use super::{ValueCheck, arg_type_convertible, detect_query_native_columns};

fn cols(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect()
}

#[test]
fn inject_param_matching_a_column_is_native() {
    // The bug: an inject-only list query (no explicit args) whose view has a real
    // `tenant_id` column must filter on the native column, not the JSONB fallback
    // (`data->>'tenant_id'`, which is NULL when `data` lacks the key → 0 rows).
    let column_map = cols(&[("id", "uuid"), ("tenant_id", "uuid"), ("data", "jsonb")]);
    let (native, fallbacks) = detect_query_native_columns(
        &[],
        ["tenant_id"].into_iter(),
        &column_map,
        &std::collections::HashSet::new(),
    );
    assert_eq!(native.get("tenant_id").map(|c| c.pg_type.as_str()), Some("uuid"));
    assert!(fallbacks.is_empty(), "inject-param matches must not emit a fallback warning");
}

#[test]
fn inject_param_without_a_column_is_silent_jsonb_fallback() {
    // A claim that lives inside `data` (no matching column) stays on the JSONB path
    // with no warning — that is a legitimate shape for inject params.
    let column_map = cols(&[("id", "uuid"), ("data", "jsonb")]);
    let (native, fallbacks) = detect_query_native_columns(
        &[],
        ["tenant_id"].into_iter(),
        &column_map,
        &std::collections::HashSet::new(),
    );
    assert!(native.is_empty());
    assert!(fallbacks.is_empty());
}

#[test]
fn explicit_arg_matching_a_column_is_native() {
    let column_map = cols(&[("id", "uuid"), ("data", "jsonb")]);
    let (native, fallbacks) = detect_query_native_columns(
        &["id"],
        std::iter::empty::<&str>(),
        &column_map,
        &std::collections::HashSet::new(),
    );
    assert_eq!(native.get("id").map(|c| c.pg_type.as_str()), Some("uuid"));
    assert!(fallbacks.is_empty());
}

#[test]
fn explicit_arg_without_a_column_warns_fallback() {
    // Explicit args keep the existing fallback warning (the author likely expects an
    // indexed native-column lookup).
    let column_map = cols(&[("data", "jsonb")]);
    let (native, fallbacks) = detect_query_native_columns(
        &["slug"],
        std::iter::empty::<&str>(),
        &column_map,
        &std::collections::HashSet::new(),
    );
    assert!(native.is_empty());
    assert_eq!(fallbacks, vec!["slug".to_string()]);
}

#[test]
fn args_and_inject_params_combine() {
    let column_map = cols(&[("id", "uuid"), ("tenant_id", "uuid")]);
    let (native, fallbacks) = detect_query_native_columns(
        &["id"],
        ["tenant_id"].into_iter(),
        &column_map,
        &std::collections::HashSet::new(),
    );
    assert_eq!(native.len(), 2);
    assert_eq!(native.get("id").map(|c| c.pg_type.as_str()), Some("uuid"));
    assert_eq!(native.get("tenant_id").map(|c| c.pg_type.as_str()), Some("uuid"));
    assert!(fallbacks.is_empty());
}

// ─── arg↔column type-convertibility (pure; #384 Gap B) ──────────────────────

#[test]
fn int_arg_against_numeric_column_is_convertible() {
    assert!(arg_type_convertible(&FieldType::Int, "integer"));
    assert!(arg_type_convertible(&FieldType::Int, "bigint"));
    assert!(arg_type_convertible(&FieldType::Float, "numeric"));
    assert!(arg_type_convertible(&FieldType::Decimal, "double precision"));
}

#[test]
fn int_arg_against_non_numeric_column_is_not_convertible() {
    // The canonical bug: an Int filter bound against a uuid/text/bool column.
    assert!(!arg_type_convertible(&FieldType::Int, "uuid"));
    assert!(!arg_type_convertible(&FieldType::Int, "text"));
    assert!(!arg_type_convertible(&FieldType::Int, "boolean"));
}

#[test]
fn boolean_arg_only_matches_boolean_column() {
    assert!(arg_type_convertible(&FieldType::Boolean, "boolean"));
    assert!(!arg_type_convertible(&FieldType::Boolean, "text"));
    assert!(!arg_type_convertible(&FieldType::Boolean, "integer"));
}

#[test]
fn uuid_arg_matches_uuid_or_text_column() {
    assert!(arg_type_convertible(&FieldType::Uuid, "uuid"));
    // uuids are commonly stored as text in SQLite / portable schemas.
    assert!(arg_type_convertible(&FieldType::Uuid, "character varying"));
    assert!(!arg_type_convertible(&FieldType::Uuid, "bigint"));
}

#[test]
fn id_and_string_args_are_permissive() {
    // `ID` intentionally spans uuid / integer / text key columns — never warn.
    assert!(arg_type_convertible(&FieldType::Id, "uuid"));
    assert!(arg_type_convertible(&FieldType::Id, "bigint"));
    assert!(arg_type_convertible(&FieldType::Id, "text"));
    // `String` binds as a text-coercible parameter — never warn.
    assert!(arg_type_convertible(&FieldType::String, "integer"));
}

#[test]
fn unknown_column_family_is_never_flagged() {
    // Custom domains, enums, geometry, arrays … are not second-guessed.
    assert!(arg_type_convertible(&FieldType::Int, "mood_enum"));
    assert!(arg_type_convertible(&FieldType::Boolean, "ltree"));
}

#[test]
fn list_arg_is_checked_by_its_element_type() {
    // `[Int!]` against a uuid column is still a mismatch; `[ID!]` is permissive.
    let int_list = FieldType::list(FieldType::Int);
    assert!(!arg_type_convertible(&int_list, "uuid"));
    let id_list = FieldType::list(FieldType::Id);
    assert!(arg_type_convertible(&id_list, "uuid"));
}

mod tview_index_advice_tests {
    //! #1571: the `pg_tviews` `indexes` entry a TVIEW query's document filters need.

    use fraiseql_core::schema::{ArgumentDefinition, FieldType, QueryDefinition};
    use serde_json::{Value, json};

    use super::super::{
        DatabaseWarning, QueryFilter, tview_index_advice, unify_tview_index_advice,
    };

    fn query() -> QueryDefinition {
        let mut query = QueryDefinition::new("members", "Member");
        for (name, ty) in [
            ("email", FieldType::String),
            ("age", FieldType::Int),
            ("score", FieldType::Float),
            ("active", FieldType::Boolean),
        ] {
            query.arguments.push(ArgumentDefinition::new(name, ty));
        }
        query
    }

    fn declared(options: Value) -> fraiseql_db::TviewDeclaration {
        fraiseql_db::TviewDeclaration {
            tview: "app.tv_member".to_string(),
            options,
        }
    }

    fn arg(name: &str) -> (QueryFilter, String) {
        (QueryFilter::Argument(name.to_string()), name.to_string())
    }

    fn advice(options: Value, filters: &[(QueryFilter, String)]) -> Option<(Vec<String>, Value)> {
        match tview_index_advice(&query(), &declared(options), filters)? {
            DatabaseWarning::TviewIndexMissing { keys, options, .. } => Some((keys, options)),
            other => panic!("{other:?}"),
        }
    }

    /// Each key as FraiseQL compares it: an inject parameter and a string as text, a JSON
    /// number as `numeric`, a boolean as `boolean`; in the query's filter order, one index.
    #[test]
    fn the_entry_types_each_key_as_fraiseql_compares_it() {
        let filters = [
            (QueryFilter::Inject("org".to_string()), "org".to_string()),
            arg("email"),
            arg("age"),
            arg("score"),
            arg("active"),
        ];
        let (keys, options) = advice(json!({}), &filters).unwrap();
        assert_eq!(keys, ["org", "email", "age", "score", "active"]);
        assert_eq!(
            options["indexes"],
            json!([[
                "org",
                "email",
                { "path": "age", "type": "numeric" },
                { "path": "score", "type": "numeric" },
                { "path": "active", "type": "boolean" }
            ]])
        );
    }

    /// A replace's options are its whole declaration: the advice carries the others, and the
    /// indexes already declared.
    #[test]
    fn the_options_keep_every_other_declaration() {
        let (_, options) = advice(
            json!({ "fillfactor": 70, "typename": "Member", "indexes": [["region"]] }),
            &[arg("email")],
        )
        .unwrap();
        assert_eq!(
            options,
            json!({ "fillfactor": 70, "typename": "Member", "indexes": [["region"], ["email"]] })
        );
    }

    /// An index that leads with the keys, typed alike, serves them: no advice. A direction,
    /// or a longer index, does not matter to an equality.
    #[test]
    fn a_declared_index_leading_with_the_keys_serves_them() {
        for indexes in [
            json!([["email"]]),
            json!([[{ "path": "email" }]]),
            json!([[{ "path": "email", "type": "text", "desc": true }, "region"]]),
        ] {
            assert_eq!(advice(json!({ "indexes": indexes }), &[arg("email")]), None, "{indexes}");
        }
        assert_eq!(
            advice(json!({ "indexes": [[{ "path": "age", "type": "numeric" }]] }), &[arg("age")]),
            None
        );
    }

    /// What does not lead with them, or types them otherwise, serves nothing.
    #[test]
    fn an_index_that_does_not_lead_with_the_keys_serves_nothing() {
        for indexes in [
            json!([["region", "email"]]),
            json!([["age"]]),
            json!([[{ "path": "age", "type": "bigint" }]]),
        ] {
            let filters = [arg("email"), arg("age")];
            let filters = if indexes.to_string().contains("region") {
                &filters[..1]
            } else {
                &filters[1..]
            };
            assert!(advice(json!({ "indexes": indexes }), filters).is_some(), "{indexes}");
        }
    }

    /// An index on fewer keys than the query filters on serves a prefix, not the query.
    #[test]
    fn an_index_on_fewer_keys_serves_nothing() {
        assert!(advice(json!({ "indexes": [["email"]] }), &[arg("email"), arg("age")]).is_some());
    }

    #[test]
    fn a_query_with_no_document_filter_needs_no_index() {
        assert_eq!(advice(json!({}), &[]), None);
    }

    /// The printed statement replaces the TVIEW by its registry row, with those options.
    #[test]
    fn the_statement_quotes_what_it_embeds() {
        let warning = DatabaseWarning::TviewIndexMissing {
            query_name: "q".to_string(),
            tview:      "app.\"tv_o'brien\"".to_string(),
            keys:       vec!["email".to_string()],
            options:    json!({ "typename": "O'Brien", "indexes": [["email"]] }),
        };
        let statement = warning.statement().unwrap();
        // The options literal, unquoted, is the options.
        let literal = statement.split_once(", '").unwrap().1.split_once("'::jsonb").unwrap().0;
        let options: Value = serde_json::from_str(&literal.replace("''", "'")).unwrap();
        assert_eq!(options, json!({ "typename": "O'Brien", "indexes": [["email"]] }));
        assert!(statement.ends_with(r#"= 'app."tv_o''brien"';"#), "{statement}");
    }

    /// One compile advises several queries over one TVIEW: each statement carries every index
    /// advised for it, since a replace declaring only its own would drop the others'.
    #[test]
    fn every_statement_for_one_tview_declares_every_index_advised_for_it() {
        let warning = |query: &str, tview: &str, key: &str| DatabaseWarning::TviewIndexMissing {
            query_name: query.to_string(),
            tview:      tview.to_string(),
            keys:       vec![key.to_string()],
            options:    json!({ "fillfactor": 70, "indexes": [["region"], [key]] }),
        };
        let mut warnings = vec![
            warning("byEmail", "app.tv_member", "email"),
            DatabaseWarning::MissingRelation {
                query_name: "other".to_string(),
                sql_source: "v_other".to_string(),
            },
            warning("byAge", "app.tv_member", "age"),
            warning("byName", "app.tv_team", "name"),
        ];
        unify_tview_index_advice(&mut warnings);
        let options = |i: usize| match &warnings[i] {
            DatabaseWarning::TviewIndexMissing { options, .. } => options.clone(),
            other => panic!("{other:?}"),
        };
        let member = json!({ "fillfactor": 70, "indexes": [["region"], ["email"], ["age"]] });
        assert_eq!(options(0), member);
        assert_eq!(options(2), member);
        assert_eq!(options(3), json!({ "fillfactor": 70, "indexes": [["region"], ["name"]] }));
    }
}

/// The documents a `scoped_like_parent` route embeds, read as the runtime reads them.
mod embedded_scope_key_tests {
    use serde_json::{Value, json};

    use super::super::{
        DatabaseWarning, EmbeddedKeyTally, embedded_documents, tally_embedded_keys,
    };

    type Row = (Value, Vec<Value>);

    /// Rows whose holding value is their document's `tenant_id`.
    fn tally(rows: &[Row], path: &[&str]) -> EmbeddedKeyTally {
        tally_embedded_keys(rows, path, "tenant_id", |(document, _)| document.get("tenant_id"))
    }

    fn row(document: Value) -> Row {
        (document, Vec::new())
    }

    #[test]
    fn a_route_is_followed_through_an_object_and_a_list() {
        let document = json!({
            "billing": { "lines": [{ "id": 1 }, { "id": 2 }] },
            "shipping": { "lines": [] },
        });
        assert_eq!(embedded_documents(&document, &["billing", "lines"]).len(), 2);
        assert!(embedded_documents(&document, &["shipping", "lines"]).is_empty());
    }

    #[test]
    fn a_list_in_the_middle_of_a_route_is_walked_item_by_item() {
        let document = json!({
            "parcels": [
                { "line": { "id": 1 } },
                { "line": { "id": 2 } },
                { "line": null },
            ],
        });
        assert_eq!(embedded_documents(&document, &["parcels", "line"]).len(), 2);
    }

    /// The level is read under the snake_case key first, the camelCase one second.
    #[test]
    fn a_step_falls_back_to_the_camel_case_key() {
        let camel = json!({ "orderLines": [{ "id": 1 }] });
        let both =
            json!({ "order_lines": [{ "id": 1 }], "orderLines": [{ "id": 2 }, { "id": 3 }] });
        assert_eq!(embedded_documents(&camel, &["orderLines"]).len(), 1);
        assert_eq!(embedded_documents(&both, &["orderLines"]).len(), 1);
    }

    #[test]
    fn a_missing_or_null_key_is_missing() {
        let rows = [row(json!({
            "tenant_id": "t1",
            "lines": [{ "id": 1 }, { "id": 2, "tenant_id": null }, { "id": 3, "tenant_id": "t1" }],
        }))];
        assert_eq!(
            tally(&rows, &["lines"]),
            EmbeddedKeyTally {
                documents:   3,
                missing:     2,
                disagreeing: 0,
            }
        );
    }

    /// The runtime compares JSON values: a string never meets a number.
    #[test]
    fn a_key_of_another_json_type_disagrees() {
        let rows = [row(json!({
            "tenant_id": 5,
            "lines": [{ "tenant_id": "5" }, { "tenant_id": 6 }, { "tenant_id": 5 }],
        }))];
        assert_eq!(
            tally(&rows, &["lines"]),
            EmbeddedKeyTally {
                documents:   3,
                missing:     0,
                disagreeing: 2,
            }
        );
    }

    /// A row with no scope value is served to nobody: what it embeds is not counted.
    #[test]
    fn a_row_without_a_value_is_skipped() {
        let rows = [row(json!({ "lines": [{ "id": 1 }] }))];
        assert_eq!(tally(&rows, &["lines"]), EmbeddedKeyTally::default());
    }

    /// A native column holding SQL `NULL` arrives as JSON `null`: that row too is served
    /// to nobody.
    #[test]
    fn a_row_whose_native_column_is_null_is_skipped() {
        let rows = [(json!({ "lines": [{ "id": 1 }] }), vec![Value::Null])];
        let tally =
            tally_embedded_keys(&rows, &["lines"], "tenant_id", |(_, values)| values.first());
        assert_eq!(tally, EmbeddedKeyTally::default());
    }

    /// The holding value is read where the caller says: here a native column.
    #[test]
    fn the_holding_value_is_read_from_the_native_column() {
        let rows = [(json!({ "lines": [{ "tenant_id": "t1" }] }), vec![json!("t2")])];
        let tally =
            tally_embedded_keys(&rows, &["lines"], "tenant_id", |(_, values)| values.first());
        assert_eq!(
            tally,
            EmbeddedKeyTally {
                documents:   1,
                missing:     0,
                disagreeing: 1,
            }
        );
    }

    /// `compile --database` output lands in CI logs: it names columns and counts, never a
    /// sampled value.
    #[test]
    fn a_finding_names_columns_and_counts_not_values() {
        let disagreement = DatabaseWarning::EmbeddedScopeKeyDisagreement {
            query_name:     "orders".into(),
            sql_source:     "v_order".into(),
            owner:          "Order".into(),
            field:          "lines".into(),
            embedded:       "OrderLine".into(),
            route:          "lines".into(),
            key:            "tenant_id".into(),
            holding_native: true,
            holding_column: "tenant_id".into(),
            documents:      3,
            disagreeing:    2,
        }
        .to_string();
        for needle in [
            "Order.lines",
            "OrderLine",
            "`tenant_id`",
            "column `tenant_id`",
            "2 of 3",
        ] {
            assert!(disagreement.contains(needle), "names {needle}: {disagreement}");
        }
    }
}

/// An `Int` is held to the signed 32-bit range: a number outside it is counted, its two
/// ends are not, and a value of another kind is the completion's concern, not this range's.
#[test]
fn an_int_is_held_to_the_signed_32_bit_range_and_only_a_number_counts() {
    let check = ValueCheck::of(&FieldType::Int).unwrap();
    for (value, outside) in [
        (serde_json::json!(2_147_483_648_i64), true),
        (serde_json::json!(-2_147_483_649_i64), true),
        (serde_json::json!(3.0e9), true),
        (serde_json::json!(2_147_483_647), false),
        (serde_json::json!(-2_147_483_648_i64), false),
        (serde_json::json!("3000000000"), false),
        (serde_json::json!(true), false),
    ] {
        assert_eq!(check.refuses(&value), outside, "{value}");
    }
    assert!(ValueCheck::of(&FieldType::BigInt).is_none(), "a BigInt holds 64 bits");
}
