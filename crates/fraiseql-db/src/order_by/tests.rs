#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;
use crate::types::sql_hints::OrderDirection;

#[test]
fn test_append_order_by_none() {
    let mut sql = "SELECT data FROM v_user".to_string();
    let bound =
        append_order_by(&mut sql, None, DatabaseType::PostgreSQL, 1, Tiebreak::Identity).unwrap();
    assert!(bound.is_empty());
    assert!(!sql.contains("ORDER BY"));
}

#[test]
fn test_append_order_by_empty() {
    let mut sql = "SELECT data FROM v_user".to_string();
    let bound =
        append_order_by(&mut sql, Some(&[]), DatabaseType::PostgreSQL, 1, Tiebreak::Identity)
            .unwrap();
    assert!(bound.is_empty());
    assert!(!sql.contains("ORDER BY"));
}

#[test]
fn test_append_order_by_single_clause_postgres() {
    let mut sql = "SELECT data FROM v_user".to_string();
    let clauses = [OrderByClause::new(
        "createdAt".to_string(),
        OrderDirection::Desc,
    )];
    let bound =
        append_order_by(&mut sql, Some(&clauses), DatabaseType::PostgreSQL, 1, Tiebreak::Identity)
            .unwrap();
    assert!(bound.is_empty(), "a field ordering binds nothing");
    assert_eq!(
        sql,
        "SELECT data FROM v_user ORDER BY data->>'created_at' DESC, data->>'id' ASC"
    );
}

#[test]
fn test_append_order_by_multiple_clauses_postgres() {
    let mut sql = "SELECT data FROM v_user".to_string();
    let clauses = [
        OrderByClause::new("lastName".to_string(), OrderDirection::Asc),
        OrderByClause::new("createdAt".to_string(), OrderDirection::Desc),
    ];
    let bound =
        append_order_by(&mut sql, Some(&clauses), DatabaseType::PostgreSQL, 1, Tiebreak::Identity)
            .unwrap();
    assert!(bound.is_empty(), "a field ordering binds nothing");
    assert_eq!(
        sql,
        "SELECT data FROM v_user ORDER BY data->>'last_name' ASC, data->>'created_at' DESC, \
         data->>'id' ASC"
    );
}

#[test]
fn test_append_order_by_invalid_field_name() {
    let mut sql = "SELECT data FROM v_user".to_string();
    let clauses = [OrderByClause::new(
        "field'; DROP TABLE users; --".to_string(),
        OrderDirection::Asc,
    )];
    let result =
        append_order_by(&mut sql, Some(&clauses), DatabaseType::PostgreSQL, 1, Tiebreak::Identity);
    assert!(result.is_err());
}

#[test]
fn test_append_order_by_snake_case_passthrough() {
    let mut sql = "SELECT data FROM v_user".to_string();
    let clauses = [OrderByClause::new("id".to_string(), OrderDirection::Asc)];
    let bound =
        append_order_by(&mut sql, Some(&clauses), DatabaseType::PostgreSQL, 1, Tiebreak::Identity)
            .unwrap();
    assert!(bound.is_empty(), "a field ordering binds nothing");
    assert_eq!(sql, "SELECT data FROM v_user ORDER BY data->>'id' ASC");
}

// ── typed ORDER BY ───────────────────────────────────────────────────

#[test]
fn test_append_order_by_numeric_cast_postgres() {
    use crate::types::sql_hints::ScalarFieldType;

    let mut sql = "SELECT data FROM v_order".to_string();
    let mut clause = OrderByClause::new("totalAmount".to_string(), OrderDirection::Desc);
    clause.field_type = ScalarFieldType::Numeric;
    let bound =
        append_order_by(&mut sql, Some(&[clause]), DatabaseType::PostgreSQL, 1, Tiebreak::Identity)
            .unwrap();
    assert!(bound.is_empty(), "a field ordering binds nothing");
    assert_eq!(
        sql,
        "SELECT data FROM v_order ORDER BY (data->>'total_amount')::numeric DESC, data->>'id' ASC"
    );
}

#[test]
fn test_append_order_by_datetime_cast_postgres() {
    use crate::types::sql_hints::ScalarFieldType;

    let mut sql = "SELECT data FROM v_event".to_string();
    let mut clause = OrderByClause::new("createdAt".to_string(), OrderDirection::Desc);
    clause.field_type = ScalarFieldType::DateTime;
    let bound =
        append_order_by(&mut sql, Some(&[clause]), DatabaseType::PostgreSQL, 1, Tiebreak::Identity)
            .unwrap();
    assert!(bound.is_empty(), "a field ordering binds nothing");
    assert_eq!(
        sql,
        "SELECT data FROM v_event ORDER BY (data->>'created_at')::timestamptz DESC, data->>'id' ASC"
    );
}

// ── native column ORDER BY ───────────────────────────────────────────

#[test]
fn test_append_order_by_native_column() {
    let mut sql = "SELECT data FROM tv_user".to_string();
    let clause = OrderByClause {
        field:         "createdAt".to_string(),
        direction:     OrderDirection::Desc,
        field_type:    crate::types::sql_hints::ScalarFieldType::DateTime,
        native_column: Some("created_at".to_string()),
        vector:        None,
        relevance:     None,
    };
    let bound =
        append_order_by(&mut sql, Some(&[clause]), DatabaseType::PostgreSQL, 1, Tiebreak::Identity)
            .unwrap();
    assert!(bound.is_empty(), "a field ordering binds nothing");
    // Native column is used directly — no JSON extraction, no cast.
    assert_eq!(sql, "SELECT data FROM tv_user ORDER BY created_at DESC, data->>'id' ASC");
}

#[test]
fn test_append_order_by_mixed_native_and_jsonb() {
    use crate::types::sql_hints::ScalarFieldType;

    let mut sql = "SELECT data FROM tv_user".to_string();
    let clauses = [
        OrderByClause {
            field:         "createdAt".to_string(),
            direction:     OrderDirection::Desc,
            field_type:    ScalarFieldType::DateTime,
            native_column: Some("created_at".to_string()),
            vector:        None,
            relevance:     None,
        },
        {
            let mut c = OrderByClause::new("name".to_string(), OrderDirection::Asc);
            c.field_type = ScalarFieldType::Text;
            c
        },
    ];
    let bound =
        append_order_by(&mut sql, Some(&clauses), DatabaseType::PostgreSQL, 1, Tiebreak::Identity)
            .unwrap();
    assert!(bound.is_empty(), "a field ordering binds nothing");
    assert_eq!(
        sql,
        "SELECT data FROM tv_user ORDER BY created_at DESC, data->>'name' ASC, data->>'id' ASC"
    );
}

// ── render_order_by_columns (bare list, for backends that supply the keyword) ──

#[test]
fn test_render_order_by_columns_none() {
    assert!(
        render_order_by_columns(None, DatabaseType::PostgreSQL, 1, Tiebreak::None)
            .unwrap()
            .is_none()
    );
}

#[test]
fn test_render_order_by_columns_empty() {
    assert!(
        render_order_by_columns(Some(&[]), DatabaseType::PostgreSQL, 1, Tiebreak::None)
            .unwrap()
            .is_none()
    );
}

#[test]
fn test_render_order_by_columns_no_keyword_prefix() {
    let clauses = [
        OrderByClause::new("lastName".to_string(), OrderDirection::Asc),
        OrderByClause::new("createdAt".to_string(), OrderDirection::Desc),
    ];
    let cols = render_order_by_columns(Some(&clauses), DatabaseType::PostgreSQL, 1, Tiebreak::None)
        .unwrap()
        .unwrap()
        .columns;
    // No leading "ORDER BY" — the backend's query builder supplies the keyword.
    assert_eq!(cols, "data->>'last_name' ASC, data->>'created_at' DESC");
    assert!(!cols.contains("ORDER BY"));
}

#[test]
fn test_render_order_by_columns_matches_append_body() {
    // The bare list must equal append_order_by's output minus the " ORDER BY " prefix —
    // for the SAME tie-break question. Asking one for a total order and the other for a
    // bare render would compare two different contracts and fail for the wrong reason.
    let clauses = [OrderByClause::new(
        "createdAt".to_string(),
        OrderDirection::Desc,
    )];
    let cols = render_order_by_columns(Some(&clauses), DatabaseType::PostgreSQL, 1, Tiebreak::None)
        .unwrap()
        .unwrap()
        .columns;
    let mut sql = String::new();
    append_order_by(&mut sql, Some(&clauses), DatabaseType::PostgreSQL, 1, Tiebreak::None).unwrap();
    assert_eq!(sql, format!(" ORDER BY {cols}"));
}

#[test]
fn test_render_order_by_columns_invalid_field_name() {
    let clauses = [OrderByClause::new(
        "field'; DROP TABLE users; --".to_string(),
        OrderDirection::Asc,
    )];
    assert!(
        render_order_by_columns(Some(&clauses), DatabaseType::PostgreSQL, 1, Tiebreak::None)
            .is_err()
    );
}

// ── vector-distance ORDER BY (#386, #959) ────────────────────────────────

fn vector_order(operator: &str, literal: &str, kind: VectorOperandKind) -> OrderByClause {
    let mut clause = OrderByClause::new("embedding".to_string(), OrderDirection::Asc);
    clause.native_column = Some("\"embedding\"".to_string());
    clause.vector = Some(crate::types::sql_hints::VectorDistanceOrder {
        operator: operator.to_string(),
        query_vector: literal.to_string(),
        kind,
    });
    clause
}

#[test]
fn float_vector_order_casts_to_vector() {
    let cols = render_order_by_columns(
        Some(&[vector_order("<=>", "[1,0,0.5]", VectorOperandKind::Float)]),
        DatabaseType::PostgreSQL,
        1,
        Tiebreak::None,
    )
    .unwrap()
    .unwrap()
    .columns;
    assert_eq!(cols, "\"embedding\" <=> '[1,0,0.5]'::vector ASC");
}

/// `varbit`, never `bit`: `'1011'::bit` is `bit(1)`, which silently reduces
/// every comparison to the first bit — an ordering that looks plausible and is
/// wrong.
#[test]
fn bit_vector_order_casts_to_varbit() {
    for (op, expected) in [("<~>", "<~>"), ("<%>", "<%>")] {
        let cols = render_order_by_columns(
            Some(&[vector_order(op, "1011", VectorOperandKind::Bit)]),
            DatabaseType::PostgreSQL,
            1,
            Tiebreak::None,
        )
        .unwrap()
        .unwrap()
        .columns;
        assert_eq!(cols, format!("\"embedding\" {expected} '1011'::varbit ASC"));
    }
}

/// The operator is validated against the operand kind, not just against a list
/// of known operators: `<~>` over a `::vector` cast is an operator PostgreSQL
/// has no candidate for.
#[test]
fn a_vector_operator_of_the_other_kind_is_refused() {
    for (op, literal, kind) in [
        ("<~>", "[1,0]", VectorOperandKind::Float),
        ("<=>", "1011", VectorOperandKind::Bit),
    ] {
        let err = render_order_by_columns(
            Some(&[vector_order(op, literal, kind)]),
            DatabaseType::PostgreSQL,
            1,
            Tiebreak::None,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("unsupported vector distance operator"), "got: {err}");
    }
}

#[test]
fn a_malformed_bit_literal_is_refused() {
    for literal in ["10x1", "", "[1,0]"] {
        let err = render_order_by_columns(
            Some(&[vector_order("<~>", literal, VectorOperandKind::Bit)]),
            DatabaseType::PostgreSQL,
            1,
            Tiebreak::None,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("malformed vector literal"), "got for {literal:?}: {err}");
    }
}

// ── full-text relevance ORDER BY (#1284) ─────────────────────────────────

use crate::types::sql_hints::RelevanceOrder;

fn relevance_order(fields: &[&str], query: &str) -> OrderByClause {
    OrderByClause::by_relevance(RelevanceOrder {
        fields: fields.iter().map(|f| (*f).to_string()).collect(),
        query:  query.to_string(),
    })
}

/// The rank is `ts_rank` over the searched document, descending, and the search
/// text is a **bound parameter** — never part of the SQL string.
///
/// Both halves matter. The expression is what `?search=` without `?sort=` had
/// promised and never emitted (`[{"_relevance":"desc"}]` failed to parse three
/// layers below the handler that wrote it); the parameter is why it can be
/// emitted at all, since the query text is whatever the client typed.
#[test]
fn a_relevance_clause_ranks_by_ts_rank_and_binds_its_query() {
    let rendered = render_order_by_columns(
        Some(&[relevance_order(&["label"], "row-42")]),
        DatabaseType::PostgreSQL,
        1,
        Tiebreak::None,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        rendered.columns,
        "ts_rank(to_tsvector(coalesce(data->>'label', '')), websearch_to_tsquery($1)) DESC"
    );
    assert_eq!(rendered.params, vec!["row-42".to_string()]);
    assert!(
        !rendered.columns.contains("row-42"),
        "the search text must be bound, not interpolated: {}",
        rendered.columns
    );
}

/// The placeholder is the one the caller says is free, not `$1`.
///
/// ORDER BY is appended after WHERE and before LIMIT, so its parameter number
/// depends on how many the filter already took. Rendering `$1` unconditionally
/// would re-bind the filter's first value as the search term — a wrong result
/// set under a 200, not an error.
#[test]
fn a_relevance_clause_binds_at_the_index_the_caller_offers() {
    let mut sql = "SELECT data FROM v_doc WHERE to_tsvector(data->>'label') @@ \
                   websearch_to_tsquery($1)"
        .to_string();
    let bound = append_order_by(
        &mut sql,
        Some(&[relevance_order(&["label"], "x")]),
        DatabaseType::PostgreSQL,
        2,
        Tiebreak::Identity,
    )
    .unwrap();
    assert!(sql.contains("websearch_to_tsquery($2)) DESC"), "{sql}");
    assert!(
        sql.ends_with("DESC, data->>'id' ASC"),
        "a ts_rank ordering is tie-broken too: {sql}"
    );
    assert_eq!(bound, vec!["x".to_string()]);
}

/// Several searchable fields rank over their concatenation, each `coalesce`d.
///
/// `'a' || NULL` is NULL in SQL, so a row missing one of the JSON keys would
/// have a NULL document and a NULL rank — every row tied, the ordering silently
/// gone under a 200. The matching predicate needs no coalesce (a NULL `@@`
/// query is simply not a match); the rank does.
#[test]
fn a_multi_field_relevance_clause_coalesces_every_operand() {
    let rendered = render_order_by_columns(
        Some(&[relevance_order(&["title", "body"], "q")]),
        DatabaseType::PostgreSQL,
        1,
        Tiebreak::None,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        rendered.columns,
        "ts_rank(to_tsvector(coalesce(data->>'title', '') || ' ' || coalesce(data->>'body', '')), \
         websearch_to_tsquery($1)) DESC"
    );
}

/// The field names reach SQL as `data->>'key'`, so they cross the same
/// injection boundary an ORDER BY field does — and are validated by the same
/// function, not by a second copy of the rule.
#[test]
fn a_relevance_field_name_passes_the_identifier_boundary() {
    let err = render_order_by_columns(
        Some(&[relevance_order(&["label'; DROP TABLE users; --"], "q")]),
        DatabaseType::PostgreSQL,
        1,
        Tiebreak::None,
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("orderBy"), "got: {err}");
}

/// A rank over no document is not an ordering. The producer cannot build one
/// (`?search=` is refused on a type with no searchable fields), so this is the
/// backstop for a future producer that forgets.
#[test]
fn a_relevance_clause_with_no_fields_is_refused() {
    let err = render_order_by_columns(
        Some(&[relevance_order(&[], "q")]),
        DatabaseType::PostgreSQL,
        1,
        Tiebreak::None,
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("at least one searchable field"), "got: {err}");
}

/// The predicate a builder without a parameter channel asks before rendering.
///
/// Both such builders — the relay keyset query and the fraiseql-wire adapter —
/// refuse a relevance clause in their own words. They ask this rather than
/// inspecting `relevance` themselves, so "does this ordering bind a value" has
/// one answer in one place.
#[test]
fn only_a_relevance_clause_reports_that_it_binds_a_parameter() {
    assert!(relevance_order(&["label"], "q").binds_parameter());
    assert!(!OrderByClause::new("id".to_string(), OrderDirection::Asc).binds_parameter());
    assert!(!vector_order("<=>", "[1,0]", VectorOperandKind::Float).binds_parameter());
}

/// A cursor-paginated read refuses a relevance ordering by name.
///
/// The relay builder assembles its `ORDER BY` into a `format!` string with
/// hand-managed parameter indices, so a relevance clause rendered there would
/// emit a placeholder that collides with the cursor's — a wrong page under a
/// `200`. And a rank is not a resumable sort key in the first place.
#[test]
fn cursor_pagination_refuses_a_relevance_ordering() {
    let err = refuse_relevance_under_cursor_pagination(Some(&[relevance_order(&["label"], "q")]))
        .unwrap_err()
        .to_string();
    assert!(err.contains("not a resumable sort key"), "got: {err}");
    // Every other ordering passes through: this guard is about the rank, not
    // about ordering a cursor query at all.
    assert!(
        refuse_relevance_under_cursor_pagination(Some(&[OrderByClause::new(
            "id".to_string(),
            OrderDirection::Asc
        )]))
        .is_ok()
    );
    assert!(refuse_relevance_under_cursor_pagination(None).is_ok());
}

/// The pagination tie-breaker (#1287).
///
/// `LIMIT`/`OFFSET` asks for a slice of a sequence, and a sequence exists only if
/// the ordering is total. `ORDER BY status` over four distinct statuses is not:
/// measured against PostgreSQL 16 over 2 000 rows, walking three pages of 100
/// returned 300 rows of which 156 were distinct — 144 duplicated, and as many
/// never seen — under `200`, with no error anywhere. With `data->>'id'` appended
/// the same walk returned 300 distinct rows.
mod pagination_tiebreak {
    use super::*;

    fn cols(clauses: &[OrderByClause], tiebreak: Tiebreak) -> String {
        render_order_by_columns(Some(clauses), DatabaseType::PostgreSQL, 1, tiebreak)
            .unwrap()
            .unwrap()
            .columns
    }

    #[test]
    fn a_non_unique_ordering_gains_the_identity() {
        let clauses = [OrderByClause::new(
            "status".to_string(),
            OrderDirection::Asc,
        )];
        assert_eq!(cols(&clauses, Tiebreak::Identity), "data->>'status' ASC, data->>'id' ASC");
    }

    #[test]
    fn tiebreak_none_renders_exactly_the_clauses_given() {
        // The relay builder's contract: it appends its cursor column itself, and a
        // term after that one would sit between the sort key and the cursor the
        // next page resumes from.
        let clauses = [OrderByClause::new(
            "status".to_string(),
            OrderDirection::Asc,
        )];
        assert_eq!(cols(&clauses, Tiebreak::None), "data->>'status' ASC");
    }

    #[test]
    fn an_ordering_that_already_names_the_identity_is_left_alone() {
        let clauses = [OrderByClause::new("id".to_string(), OrderDirection::Asc)];
        assert_eq!(cols(&clauses, Tiebreak::Identity), "data->>'id' ASC");
    }

    #[test]
    fn the_identity_counts_wherever_it_sits_in_the_ordering() {
        // Not only as the last term: an ordering whose SECOND key is the identity
        // is already total, and appending a third copy is noise.
        let clauses = [
            OrderByClause::new("status".to_string(), OrderDirection::Asc),
            OrderByClause::new("id".to_string(), OrderDirection::Desc),
        ];
        assert_eq!(cols(&clauses, Tiebreak::Identity), "data->>'status' ASC, data->>'id' DESC");
    }

    fn native(field: &str, column: &str) -> OrderByClause {
        OrderByClause {
            field:         field.to_string(),
            direction:     OrderDirection::Asc,
            field_type:    crate::types::sql_hints::ScalarFieldType::Text,
            native_column: Some(column.to_string()),
            vector:        None,
            relevance:     None,
        }
    }

    #[test]
    fn a_native_id_column_counts_as_the_identity() {
        assert_eq!(cols(&[native("id", "id")], Tiebreak::Identity), "id ASC");
    }

    #[test]
    fn a_field_named_id_reading_another_native_column_does_not_count() {
        // `native_column` is what the ordering actually READS. A clause whose
        // GraphQL field is `id` but whose native column is `tenant_id` orders by
        // the tenant, which is emphatically not unique — reading the field name
        // instead would drop the tie-breaker exactly where it is most needed.
        assert_eq!(
            cols(&[native("id", "tenant_id")], Tiebreak::Identity),
            "tenant_id ASC, data->>'id' ASC"
        );
    }

    #[test]
    fn a_field_whose_name_merely_ends_in_id_is_not_the_identity() {
        // The comparison is `storage_key() == "id"`, not a suffix test. `userId`
        // snake_cases to `user_id`, which is a foreign key and repeats freely —
        // reading it as the identity would drop the tie-breaker on one of the most
        // common orderings there is.
        let clauses = [OrderByClause::new(
            "userId".to_string(),
            OrderDirection::Asc,
        )];
        assert_eq!(cols(&clauses, Tiebreak::Identity), "data->>'user_id' ASC, data->>'id' ASC");
    }

    #[test]
    fn a_relevance_ordering_is_tie_broken() {
        // The case #1287 calls worst: `ts_rank` scores collide readily — every row
        // matching one term once scores identically — and the ordering is the
        // SERVER's choice since #1284, so a client cannot see that it needs one.
        let rendered = render_order_by_columns(
            Some(&[relevance_order(&["label"], "q")]),
            DatabaseType::PostgreSQL,
            1,
            Tiebreak::Identity,
        )
        .unwrap()
        .unwrap();
        assert!(rendered.columns.ends_with("DESC, data->>'id' ASC"), "{}", rendered.columns);
        assert_eq!(rendered.params, vec!["q".to_string()]);
    }

    #[test]
    fn the_tiebreaker_binds_no_parameter() {
        // It is a column expression, not a value, so it cannot disturb the
        // placeholder run WHERE → ORDER BY → LIMIT/OFFSET that #1284 established.
        let clauses = [OrderByClause::new(
            "status".to_string(),
            OrderDirection::Asc,
        )];
        let rendered = render_order_by_columns(
            Some(&clauses),
            DatabaseType::PostgreSQL,
            7,
            Tiebreak::Identity,
        )
        .unwrap()
        .unwrap();
        assert!(rendered.params.is_empty());
    }

    #[test]
    fn no_requested_ordering_still_renders_nothing() {
        // A read that asked for no order is not a sequence to begin with, and
        // manufacturing one would turn every unordered list read into a sort.
        assert!(
            render_order_by_columns(None, DatabaseType::PostgreSQL, 1, Tiebreak::Identity)
                .unwrap()
                .is_none()
        );
        assert!(
            render_order_by_columns(Some(&[]), DatabaseType::PostgreSQL, 1, Tiebreak::Identity)
                .unwrap()
                .is_none()
        );
    }
}
