#![allow(clippy::unwrap_used)] // Reason: test code

use fraiseql_db::where_clause::UnaccentFunction;

use super::{WrapperState, ddl, drift};

fn custom() -> UnaccentFunction {
    UnaccentFunction::parse("search.fold").unwrap()
}

#[test]
fn the_ddl_creates_the_schema_the_extension_then_an_immutable_wrapper_pinning_its_dictionary() {
    assert_eq!(
        ddl(&custom()),
        "CREATE SCHEMA IF NOT EXISTS \"search\";\n\
         CREATE EXTENSION IF NOT EXISTS unaccent WITH SCHEMA \"public\";\n\
         CREATE OR REPLACE FUNCTION \"search\".\"fold\"(text) RETURNS text\n    \
         LANGUAGE sql IMMUTABLE PARALLEL SAFE STRICT\n    \
         RETURN \"public\".unaccent('\"public\".unaccent'::regdictionary, $1);\n"
    );
}

#[test]
fn a_ready_database_reports_nothing() {
    let errors = drift(&custom(), Some("public"), &WrapperState::Ready);
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn each_missing_object_is_reported_with_the_statement_that_creates_it() {
    let errors = drift(&custom(), None, &WrapperState::Missing);
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(errors[0].contains("no `unaccent` extension"), "{}", errors[0]);
    assert!(errors[0].contains("CREATE EXTENSION IF NOT EXISTS unaccent"), "{}", errors[0]);
    assert!(errors[1].contains("no function search.fold(text)"), "{}", errors[1]);
    assert!(
        errors[1].contains("CREATE OR REPLACE FUNCTION \"search\".\"fold\""),
        "{}",
        errors[1]
    );
}

/// The suggested wrapper reads the dictionary of the schema the extension is in.
#[test]
fn the_suggested_wrapper_reads_the_dictionary_where_the_extension_is() {
    let errors = drift(&custom(), Some("ext"), &WrapperState::Missing);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0].contains("\"ext\".unaccent('\"ext\".unaccent'::regdictionary"),
        "{}",
        errors[0]
    );
}

#[test]
fn a_wrapper_that_is_not_immutable_or_not_text_is_reported() {
    let stable = drift(&custom(), Some("public"), &WrapperState::NotImmutable('s'));
    assert!(stable[0].contains("is STABLE, not IMMUTABLE"), "{stable:?}");
    let volatile = drift(&custom(), Some("public"), &WrapperState::NotImmutable('v'));
    assert!(volatile[0].contains("is VOLATILE, not IMMUTABLE"), "{volatile:?}");
    let varchar =
        drift(&custom(), Some("public"), &WrapperState::NotText("character varying".into()));
    assert!(varchar[0].contains("returns character varying, not text"), "{varchar:?}");
}
