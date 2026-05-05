//! Tests for the `response` module.

#![allow(clippy::unwrap_used)]
#![allow(clippy::missing_panics_doc)]

use axum::http::{HeaderMap, HeaderValue};
use serde_json::json;

use super::helpers::{
    build_offset_links, check_if_none_match, compute_etag, extract_id_from_data,
    extract_relay_page_info, extract_single_data, format_id_for_url,
};
use super::*;

// ---------------------------------------------------------------------------
// helpers tests
// ---------------------------------------------------------------------------

#[test]
fn compute_etag_is_consistent() {
    let data = b"test data";
    let etag1 = compute_etag(data);
    let etag2 = compute_etag(data);
    assert_eq!(etag1, etag2);
}

#[test]
fn compute_etag_differs_for_different_data() {
    let etag1 = compute_etag(b"data1");
    let etag2 = compute_etag(b"data2");
    assert_ne!(etag1, etag2);
}

#[test]
fn check_if_none_match_wildcard() {
    let mut headers = HeaderMap::new();
    headers.insert("if-none-match", HeaderValue::from_static("*"));
    assert!(check_if_none_match(&headers, "W/\"abc\"").unwrap());
}

#[test]
fn check_if_none_match_exact() {
    let mut headers = HeaderMap::new();
    headers.insert("if-none-match", HeaderValue::from_static("W/\"abc\""));
    assert!(check_if_none_match(&headers, "W/\"abc\"").unwrap());
}

#[test]
fn check_if_none_match_no_match() {
    let mut headers = HeaderMap::new();
    headers.insert("if-none-match", HeaderValue::from_static("W/\"abc\""));
    assert!(!check_if_none_match(&headers, "W/\"def\"").unwrap());
}

#[test]
fn extract_single_data_unwraps() {
    let result = json!({ "data": { "user": { "id": 1 } } });
    let data = extract_single_data(&result).unwrap();
    assert_eq!(data["id"], 1);
}

#[test]
fn extract_id_from_data_present() {
    let data = json!({ "id": 42, "name": "test" });
    assert_eq!(extract_id_from_data(&data).unwrap(), &json!(42));
}

#[test]
fn extract_id_from_data_missing() {
    let data = json!({ "name": "test" });
    assert!(extract_id_from_data(&data).is_none());
}

#[test]
fn format_id_for_url_string() {
    assert_eq!(format_id_for_url(&json!("user-123")), "user-123");
}

#[test]
fn format_id_for_url_number() {
    assert_eq!(format_id_for_url(&json!(42)), "42");
}

#[test]
fn extract_relay_page_info_present() {
    let data = json!({ "pageInfo": { "hasNextPage": true } });
    assert!(extract_relay_page_info(&data).is_some());
}

#[test]
fn build_offset_links_structure() {
    let links = build_offset_links("/users", 10, 0, Some(50));
    assert!(links["self"].is_string());
    assert!(links["first"].is_string());
    assert!(links["next"].is_string());
    assert!(links["prev"].is_null());
    assert!(links["last"].is_string());
}

// ---------------------------------------------------------------------------
// mod tests (RestResponseFormatter)
// ---------------------------------------------------------------------------

fn default_config() -> RestConfig {
    RestConfig::default()
}

fn no_etag_config() -> RestConfig {
    RestConfig {
        etag: false,
        ..RestConfig::default()
    }
}

fn entity_delete_config() -> RestConfig {
    RestConfig {
        delete_response: DeleteResponse::Entity,
        ..RestConfig::default()
    }
}

fn empty_headers() -> HeaderMap {
    HeaderMap::new()
}

#[test]
fn format_single_with_etag() {
    let config = default_config();
    let formatter = RestResponseFormatter::new(&config, "/rest/v1/users");
    let result = json!({ "data": { "users": { "id": 1, "name": "Alice" } } });
    let headers = empty_headers();

    let resp = formatter.format_single(&result, &headers).unwrap();
    assert_eq!(resp.status, StatusCode::OK);
    assert!(resp.headers.get("etag").is_some());
    assert!(resp.headers.get("x-request-id").is_some());
}

#[test]
fn format_single_no_etag() {
    let config = no_etag_config();
    let formatter = RestResponseFormatter::new(&config, "/rest/v1/users");
    let result = json!({ "data": { "users": { "id": 1 } } });
    let headers = empty_headers();

    let resp = formatter.format_single(&result, &headers).unwrap();
    assert_eq!(resp.status, StatusCode::OK);
    assert!(resp.headers.get("etag").is_none());
}

#[test]
fn format_single_if_none_match_match() {
    let config = default_config();
    let formatter = RestResponseFormatter::new(&config, "/rest/v1/users");
    let result = json!({ "data": { "users": { "id": 1 } } });

    // Compute ETag first
    let serialized = serde_json::to_vec(&result["data"]["users"]).unwrap();
    let etag = compute_etag(&serialized);

    // Send request with matching If-None-Match
    let mut headers = HeaderMap::new();
    headers.insert("if-none-match", HeaderValue::from_str(&etag).unwrap());

    let resp = formatter.format_single(&result, &headers).unwrap();
    assert_eq!(resp.status, StatusCode::NOT_MODIFIED);
    assert!(resp.body.is_none());
}

#[test]
fn format_collection_with_pagination() {
    let config = default_config();
    let formatter = RestResponseFormatter::new(&config, "/rest/v1/users");
    let result = json!({ "data": { "users": [{ "id": 1 }, { "id": 2 }] } });
    let pagination = PaginationParams::Offset {
        limit:  10,
        offset: 0,
    };
    let headers = empty_headers();

    let resp = formatter.format_collection(&result, &pagination, &headers).unwrap();
    assert_eq!(resp.status, StatusCode::OK);
    let body = resp.body.unwrap();
    assert!(body["meta"]["limit"].is_number());
    assert!(body["links"]["self"].is_string());
}

#[test]
fn format_mutation_post() {
    let config = default_config();
    let formatter = RestResponseFormatter::new(&config, "/rest/v1/users");
    let result = json!({ "data": { "createUser": { "id": 42, "name": "Bob" } } });
    let headers = empty_headers();

    let resp = formatter
        .format_mutation_post(&result, "/rest/v1/users", &headers)
        .unwrap();
    assert_eq!(resp.status, StatusCode::CREATED);
    assert!(resp.headers.get("location").is_some());
}

#[test]
fn format_delete_entity() {
    let config = entity_delete_config();
    let formatter = RestResponseFormatter::new(&config, "/rest/v1/users");
    let result = json!({
        "data": {
            "deleteUser": {
                "entity": { "id": 1, "name": "Alice" }
            }
        }
    });
    let prefer = PreferHeader::default();
    let headers = empty_headers();

    let resp = formatter
        .format_delete(&result, &prefer, "deleteUser", &headers)
        .unwrap();
    assert_eq!(resp.status, StatusCode::OK);
    assert!(resp.body.is_some());
}

#[test]
fn format_delete_no_content() {
    let config = default_config();
    let formatter = RestResponseFormatter::new(&config, "/rest/v1/users");
    let result = json!({
        "data": {
            "deleteUser": {
                "entity": null
            }
        }
    });
    let prefer = PreferHeader::default();
    let headers = empty_headers();

    let resp = formatter
        .format_delete(&result, &prefer, "deleteUser", &headers)
        .unwrap();
    assert_eq!(resp.status, StatusCode::NO_CONTENT);
    assert!(resp.body.is_none());
}

#[test]
fn format_method_not_allowed() {
    let config = default_config();
    let formatter = RestResponseFormatter::new(&config, "/rest/v1/users");
    let headers = empty_headers();

    let resp = formatter.format_method_not_allowed(
        &[HttpMethod::Get, HttpMethod::Post],
        &headers,
    );
    assert_eq!(resp.status, StatusCode::METHOD_NOT_ALLOWED);
    assert!(resp.headers.get("allow").is_some());
}

#[test]
fn rest_error_method_not_allowed() {
    let err = RestError::method_not_allowed();
    assert_eq!(err.status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(err.code, "METHOD_NOT_ALLOWED");
}
