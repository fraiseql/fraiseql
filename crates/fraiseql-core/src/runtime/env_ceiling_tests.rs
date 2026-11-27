//! `FRAISEQL_MAX_PAGE_SIZE` and `FRAISEQL_MAX_OFFSET` take the meaning their
//! `[validation]` keys have in the file: a positive integer. Zero, `none` or an
//! unparseable value refuses the config, naming the variable, rather than lifting the
//! ceiling or being dropped for the compiled value.

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use super::{RuntimeConfig, offset_precedence, page_size_precedence};
use crate::schema::{CompiledSchema, ValidationConfig};

const REFUSED: [&str; 5] = ["0", "none", "None", "lots", "-5"];

#[test]
fn page_size_default_when_nothing_set() {
    assert_eq!(page_size_precedence(None, None).unwrap(), Some(1000));
}

#[test]
fn page_size_compiled_overrides_default() {
    assert_eq!(page_size_precedence(None, Some(250)).unwrap(), Some(250));
}

#[test]
fn page_size_env_overrides_compiled() {
    assert_eq!(page_size_precedence(Some(" 500 "), Some(250)).unwrap(), Some(500));
}

#[test]
fn page_size_env_that_is_not_a_positive_integer_is_refused_naming_it() {
    for raw in REFUSED {
        let refusal = page_size_precedence(Some(raw), Some(250)).unwrap_err();
        assert!(refusal.contains("FRAISEQL_MAX_PAGE_SIZE"), "{raw}: {refusal}");
    }
}

#[test]
fn offset_has_no_ceiling_when_nothing_set() {
    assert_eq!(offset_precedence(None, None).unwrap(), None);
}

#[test]
fn offset_env_overrides_compiled() {
    assert_eq!(offset_precedence(None, Some(250)).unwrap(), Some(250));
    assert_eq!(offset_precedence(Some("500"), Some(250)).unwrap(), Some(500));
    assert_eq!(offset_precedence(Some("500"), None).unwrap(), Some(500));
}

#[test]
fn offset_env_that_is_not_a_positive_integer_is_refused_naming_it() {
    for raw in REFUSED {
        let refusal = offset_precedence(Some(raw), Some(250)).unwrap_err();
        assert!(refusal.contains("FRAISEQL_MAX_OFFSET"), "{raw}: {refusal}");
    }
}

#[test]
fn the_runtime_config_refuses_an_unparseable_ceiling_from_the_environment() {
    let mut schema = CompiledSchema::new();
    schema.validation_config = Some(ValidationConfig {
        max_page_size: Some(250),
        max_offset: Some(250),
        ..ValidationConfig::default()
    });
    for var in ["FRAISEQL_MAX_PAGE_SIZE", "FRAISEQL_MAX_OFFSET"] {
        let refusal = RuntimeConfig::default()
            .with_compiled_schema_from(&schema, |key| (key == var).then(|| "lots".to_owned()))
            .unwrap_err();
        assert!(refusal.contains(var), "{var}: {refusal}");
    }
}
