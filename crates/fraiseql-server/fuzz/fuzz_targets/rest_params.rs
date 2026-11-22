#![no_main]
//! The REST query string, through `RestParamExtractor::extract`: bracket filters,
//! `filter=` JSON, `select=` with embedding, `sort=`, pagination (offset and relay),
//! logical groups and search. Every value is the client's.
//!
//! The first byte picks the route shape: a list, a relay connection, or a single
//! resource with a path id; and whether `Prefer: handling=lenient` is set. The rest
//! is the raw query string, decoded as the router decodes it. The path id, for the
//! single-resource route, is the part before the first `?`.
//!
//! Any input must be answered with `Ok` or `Err`, never a panic, and the size limits
//! the extractor promises hold: an accepted request never exceeds `max_filter_bytes`
//! of filter JSON.

use fraiseql_core::schema::{
    ArgumentDefinition, AutoParams, FieldDefinition, FieldType, QueryDefinition, RestConfig,
    TypeDefinition,
};
use fraiseql_server::routes::rest::params::RestParamExtractor;
use libfuzzer_sys::fuzz_target;

const MAX_FILTER_BYTES: u64 = 4096;

fn config() -> RestConfig {
    RestConfig {
        max_page_size: 100,
        default_page_size: 20,
        max_filter_bytes: MAX_FILTER_BYTES,
        ..RestConfig::default()
    }
}

fn user_type() -> TypeDefinition {
    TypeDefinition::new("User", "v_user")
        .with_field(FieldDefinition::new("id", FieldType::Uuid))
        .with_field(FieldDefinition::new("name", FieldType::String))
        .with_field(FieldDefinition::new("email", FieldType::String))
        .with_field(FieldDefinition::new("age", FieldType::Int))
        .with_field(FieldDefinition::new("active", FieldType::Boolean))
}

fn query(shape: u8) -> QueryDefinition {
    let base = QueryDefinition::new("users", "User");
    match shape % 3 {
        0 => QueryDefinition {
            returns_list: true,
            auto_params: AutoParams::all(),
            arguments: vec![
                ArgumentDefinition::optional("where", FieldType::Json),
                ArgumentDefinition::optional("orderBy", FieldType::Json),
                ArgumentDefinition::optional("limit", FieldType::Int),
                ArgumentDefinition::optional("offset", FieldType::Int),
            ],
            ..base
        },
        1 => QueryDefinition {
            returns_list: true,
            relay: true,
            relay_cursor_column: Some("pk_user".to_string()),
            auto_params: AutoParams::all(),
            arguments: vec![
                ArgumentDefinition::optional("first", FieldType::Int),
                ArgumentDefinition::optional("after", FieldType::String),
                ArgumentDefinition::optional("last", FieldType::Int),
                ArgumentDefinition::optional("before", FieldType::String),
            ],
            ..base
        },
        _ => QueryDefinition {
            name: "user".to_string(),
            returns_list: false,
            arguments: vec![ArgumentDefinition::new("id", FieldType::Uuid)],
            ..base
        },
    }
}

fuzz_target!(|data: &[u8]| {
    let Some((&selector, rest)) = data.split_first() else {
        return;
    };
    let text = String::from_utf8_lossy(rest);
    let (path_id, query_string) = text.split_once('?').unwrap_or(("", &text));

    let config = config();
    let query_def = query(selector);
    let type_def = user_type();
    let extractor = RestParamExtractor::new(&config, &query_def, Some(&type_def))
        .with_lenient_handling(selector & 0x80 != 0);

    let pairs: Vec<(String, String)> = url::form_urlencoded::parse(query_string.as_bytes())
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let pairs: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let path: Vec<(&str, &str)> = if query_def.returns_list {
        Vec::new()
    } else {
        vec![("id", path_id)]
    };

    if extractor.extract(&path, &pairs).is_ok() {
        // The extractor reads the last `filter=`; that is the one the limit binds.
        if let Some((_, filter)) = pairs.iter().rev().find(|(k, _)| *k == "filter") {
            assert!(
                u64::try_from(filter.len()).is_ok_and(|n| n <= MAX_FILTER_BYTES),
                "a filter of {} bytes was accepted past max_filter_bytes",
                filter.len()
            );
        }
    }
});
