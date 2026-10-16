#![allow(clippy::unwrap_used)] // Reason: test code

use std::collections::HashMap;

use serde_json::json;

use super::by_id_tag;
use crate::db::{WhereClause, WhereOperator};

const A: &str = "00000000-0000-0000-0000-00000000157a";

fn sources() -> HashMap<String, String> {
    HashMap::from([("p.tv_user".to_string(), "User".to_string())])
}

fn by_id(value: serde_json::Value) -> WhereClause {
    WhereClause::NativeField {
        column: "id".to_string(),
        pg_cast: "uuid".to_string(),
        operator: WhereOperator::Eq,
        value,
    }
}

fn tag(view: &str, clause: Option<&WhereClause>, views: &[&str]) -> Option<(String, String)> {
    let views: Vec<String> = views.iter().map(ToString::to_string).collect();
    by_id_tag(&sources(), view, clause, &views)
}

#[test]
fn a_read_of_one_row_by_its_id_is_tagged_by_that_row() {
    assert_eq!(
        tag("p.tv_user", Some(&by_id(json!(A))), &["p.tv_user"]),
        Some(("User".to_string(), A.to_string()))
    );
}

/// PostgreSQL reads every spelling of a uuid as one value, and `pg_tviews` reports it in
/// one: the tag must be that one, or the row's write misses the read.
#[test]
fn a_uuid_is_tagged_in_the_form_the_report_names_it() {
    for spelled in [A.to_uppercase(), A.replace('-', ""), format!("{{{A}}}")] {
        assert_eq!(
            tag("p.tv_user", Some(&by_id(json!(spelled))), &["p.tv_user"]),
            Some(("User".to_string(), A.to_string())),
            "{spelled}"
        );
    }
}

#[test]
fn an_integer_id_is_tagged_by_its_digits() {
    let clause = WhereClause::NativeField {
        column:   "id".to_string(),
        pg_cast:  "int8".to_string(),
        operator: WhereOperator::Eq,
        value:    json!("007"),
    };
    assert_eq!(
        tag("p.tv_user", Some(&clause), &["p.tv_user"]),
        Some(("User".to_string(), "7".to_string()))
    );
}

/// Each of these may read more than the one row, or another row: the sweep keeps them.
#[test]
fn a_read_that_may_depend_on_more_than_its_row_is_not_tagged() {
    let id = by_id(json!(A));
    // Not a TVIEW source of the schema.
    assert_eq!(tag("p.v_user", Some(&id), &["p.v_user"]), None);
    // Another view read too (`additional_views`).
    assert_eq!(tag("p.tv_user", Some(&id), &["p.tv_user", "p.v_post"]), None);
    // No predicate: every row.
    assert_eq!(tag("p.tv_user", None, &["p.tv_user"]), None);
    // A second predicate: an injected parameter, an engine RLS filter.
    let and = WhereClause::And(vec![id.clone(), id.clone()]);
    assert_eq!(tag("p.tv_user", Some(&and), &["p.tv_user"]), None);
    // Another column, another operator.
    let other = |column: &str, operator: WhereOperator| WhereClause::NativeField {
        column: column.to_string(),
        pg_cast: "uuid".to_string(),
        operator,
        value: json!(A),
    };
    assert_eq!(
        tag("p.tv_user", Some(&other("fk_org", WhereOperator::Eq)), &["p.tv_user"]),
        None
    );
    assert_eq!(tag("p.tv_user", Some(&other("id", WhereOperator::Neq)), &["p.tv_user"]), None);
    // The document's id, which a TVIEW need not keep equal to its column.
    let document = WhereClause::Field {
        path:     vec!["id".to_string()],
        operator: WhereOperator::Eq,
        value:    json!(A),
    };
    assert_eq!(tag("p.tv_user", Some(&document), &["p.tv_user"]), None);
    // A value no uuid column holds reads nothing.
    assert_eq!(tag("p.tv_user", Some(&by_id(json!("not-a-uuid"))), &["p.tv_user"]), None);
}
