//! Tests for the `handler` module.

#![allow(clippy::unwrap_used)] // Reason: test code
#![allow(clippy::missing_panics_doc)] // Reason: test code

// ---------------------------------------------------------------------------
// routing tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod routing {
    use super::super::routing::match_route_path;
    use crate::routes::rest::resource::{
        HttpMethod, RestResource, RestRoute, RestRouteTable, RouteSource,
    };

    fn make_test_route_table() -> RestRouteTable {
        RestRouteTable {
            base_path:   "/rest/v1".to_string(),
            resources:   vec![RestResource {
                name:      "users".to_string(),
                type_name: "User".to_string(),
                id_arg:    Some("id".to_string()),
                routes:    vec![
                    RestRoute {
                        method:          HttpMethod::Get,
                        path:            "/users".to_string(),
                        source:          RouteSource::Query {
                            name: "users".to_string(),
                        },
                        update_coverage: None,
                        success_status:  200,
                    },
                    RestRoute {
                        method:          HttpMethod::Get,
                        path:            "/users/{id}".to_string(),
                        source:          RouteSource::Query {
                            name: "user".to_string(),
                        },
                        update_coverage: None,
                        success_status:  200,
                    },
                    RestRoute {
                        method:          HttpMethod::Post,
                        path:            "/users".to_string(),
                        source:          RouteSource::Mutation {
                            name: "createUser".to_string(),
                        },
                        update_coverage: None,
                        success_status:  201,
                    },
                    RestRoute {
                        method:          HttpMethod::Put,
                        path:            "/users/{id}".to_string(),
                        source:          RouteSource::Mutation {
                            name: "updateUser".to_string(),
                        },
                        update_coverage: None,
                        success_status:  200,
                    },
                    RestRoute {
                        method:          HttpMethod::Patch,
                        path:            "/users/{id}".to_string(),
                        source:          RouteSource::Mutation {
                            name: "updateUser".to_string(),
                        },
                        update_coverage: None,
                        success_status:  200,
                    },
                    RestRoute {
                        method:          HttpMethod::Patch,
                        path:            "/users/{id}/update-email".to_string(),
                        source:          RouteSource::Mutation {
                            name: "updateUserEmail".to_string(),
                        },
                        update_coverage: None,
                        success_status:  200,
                    },
                    RestRoute {
                        method:          HttpMethod::Delete,
                        path:            "/users/{id}".to_string(),
                        source:          RouteSource::Mutation {
                            name: "deleteUser".to_string(),
                        },
                        update_coverage: None,
                        success_status:  204,
                    },
                    RestRoute {
                        method:          HttpMethod::Post,
                        path:            "/users/{id}/archive".to_string(),
                        source:          RouteSource::Mutation {
                            name: "archiveUser".to_string(),
                        },
                        update_coverage: None,
                        success_status:  200,
                    },
                ],
            }],
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn resolve_collection_get() {
        let table = make_test_route_table();
        let resolved = table.resolve("/users", HttpMethod::Get).unwrap();
        assert_eq!(
            resolved.route.source,
            RouteSource::Query {
                name: "users".to_string(),
            }
        );
        assert!(resolved.path_params.is_empty());
    }

    #[test]
    fn resolve_single_get() {
        let table = make_test_route_table();
        let resolved = table.resolve("/users/42", HttpMethod::Get).unwrap();
        assert_eq!(
            resolved.route.source,
            RouteSource::Query {
                name: "user".to_string(),
            }
        );
        assert_eq!(resolved.path_params.len(), 1);
        assert_eq!(resolved.path_params[0], ("id".to_string(), "42".to_string()));
    }

    #[test]
    fn resolve_post_collection() {
        let table = make_test_route_table();
        let resolved = table.resolve("/users", HttpMethod::Post).unwrap();
        assert_eq!(
            resolved.route.source,
            RouteSource::Mutation {
                name: "createUser".to_string(),
            }
        );
    }

    #[test]
    fn resolve_put_single() {
        let table = make_test_route_table();
        let resolved = table.resolve("/users/42", HttpMethod::Put).unwrap();
        assert_eq!(
            resolved.route.source,
            RouteSource::Mutation {
                name: "updateUser".to_string(),
            }
        );
    }

    #[test]
    fn resolve_patch_single() {
        let table = make_test_route_table();
        let resolved = table.resolve("/users/42", HttpMethod::Patch).unwrap();
        assert_eq!(
            resolved.route.source,
            RouteSource::Mutation {
                name: "updateUser".to_string(),
            }
        );
    }

    #[test]
    fn resolve_patch_nested() {
        let table = make_test_route_table();
        let resolved = table.resolve("/users/42/update-email", HttpMethod::Patch).unwrap();
        assert_eq!(
            resolved.route.source,
            RouteSource::Mutation {
                name: "updateUserEmail".to_string(),
            }
        );
        assert_eq!(resolved.path_params.len(), 1);
    }

    #[test]
    fn resolve_delete_single() {
        let table = make_test_route_table();
        let resolved = table.resolve("/users/42", HttpMethod::Delete).unwrap();
        assert_eq!(
            resolved.route.source,
            RouteSource::Mutation {
                name: "deleteUser".to_string(),
            }
        );
    }

    #[test]
    fn resolve_post_action() {
        let table = make_test_route_table();
        let resolved = table.resolve("/users/42/archive", HttpMethod::Post).unwrap();
        assert_eq!(
            resolved.route.source,
            RouteSource::Mutation {
                name: "archiveUser".to_string(),
            }
        );
    }

    #[test]
    fn resolve_not_found() {
        let table = make_test_route_table();
        assert!(table.resolve("/nonexistent", HttpMethod::Get).is_none());
    }

    #[test]
    fn resolve_wrong_method() {
        let table = make_test_route_table();
        assert!(table.resolve("/users", HttpMethod::Delete).is_none());
    }

    #[test]
    fn match_route_path_static() {
        let path_params = match_route_path("/users", &["users"]);
        assert!(path_params.is_some());
        assert!(path_params.unwrap().is_empty());
    }

    #[test]
    fn match_route_path_dynamic() {
        let path_params = match_route_path("/users/{id}", &["users", "42"]);
        assert!(path_params.is_some());
        let params = path_params.unwrap();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0], ("id".to_string(), "42".to_string()));
    }

    #[test]
    fn match_route_path_multiple_params() {
        let path_params =
            match_route_path("/users/{uid}/posts/{pid}", &["users", "1", "posts", "2"]);
        assert!(path_params.is_some());
        let params = path_params.unwrap();
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].0, "uid");
        assert_eq!(params[1].0, "pid");
    }

    #[test]
    fn match_route_path_mismatch() {
        let path_params = match_route_path("/users/{id}", &["posts", "42"]);
        assert!(path_params.is_none());
    }

    #[test]
    fn match_route_path_wrong_segment_count() {
        let path_params = match_route_path("/users/{id}", &["users"]);
        assert!(path_params.is_none());
    }
}

// ---------------------------------------------------------------------------
// prefer tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod prefer {
    use axum::http::HeaderMap;

    use super::super::prefer::{CountPreference, HandlingPreference, PreferHeader};

    #[test]
    fn prefer_parse_count_exact() {
        let prefer = PreferHeader::parse("count=exact");
        assert!(prefer.count_exact);
        assert!(!prefer.return_representation);
        assert!(!prefer.return_minimal);
    }

    #[test]
    fn prefer_parse_return_representation() {
        let prefer = PreferHeader::parse("return=representation");
        assert!(!prefer.count_exact);
        assert!(prefer.return_representation);
        assert!(!prefer.return_minimal);
    }

    #[test]
    fn prefer_parse_return_minimal() {
        let prefer = PreferHeader::parse("return=minimal");
        assert!(!prefer.count_exact);
        assert!(!prefer.return_representation);
        assert!(prefer.return_minimal);
    }

    #[test]
    fn prefer_parse_combined() {
        let prefer = PreferHeader::parse("count=exact, return=representation");
        assert!(prefer.count_exact);
        assert!(prefer.return_representation);
        assert!(!prefer.return_minimal);
    }

    #[test]
    fn prefer_parse_case_insensitive() {
        let prefer = PreferHeader::parse("Count=Exact");
        assert!(prefer.count_exact);
    }

    #[test]
    fn prefer_parse_unknown_ignored() {
        let prefer = PreferHeader::parse("respond-async, count=exact");
        assert!(prefer.count_exact);
    }

    #[test]
    fn prefer_minimal_overrides_representation() {
        let prefer = PreferHeader::parse("return=representation, return=minimal");
        assert!(prefer.return_minimal);
        assert!(!prefer.return_representation);
    }

    #[test]
    fn prefer_from_headers_multiple() {
        let mut headers = HeaderMap::new();
        headers.append("prefer", axum::http::HeaderValue::from_static("count=exact"));
        headers.append("prefer", axum::http::HeaderValue::from_static("return=representation"));
        let prefer = PreferHeader::from_headers(&headers);
        assert!(prefer.count_exact);
        assert!(prefer.return_representation);
    }

    #[test]
    fn prefer_parse_resolution_merge() {
        let prefer = PreferHeader::parse("resolution=merge-duplicates");
        assert_eq!(prefer.resolution.as_deref(), Some("merge-duplicates"));
    }

    #[test]
    fn prefer_parse_resolution_ignore() {
        let prefer = PreferHeader::parse("resolution=ignore-duplicates");
        assert_eq!(prefer.resolution.as_deref(), Some("ignore-duplicates"));
    }

    #[test]
    fn prefer_parse_tx_rollback() {
        let prefer = PreferHeader::parse("tx=rollback");
        assert!(prefer.tx_rollback);
    }

    #[test]
    fn prefer_parse_tx_commit() {
        let prefer = PreferHeader::parse("tx=commit");
        assert!(!prefer.tx_rollback);
    }

    #[test]
    fn prefer_parse_handling_strict() {
        let prefer = PreferHeader::parse("handling=strict");
        assert_eq!(prefer.handling, Some(HandlingPreference::Strict));
    }

    #[test]
    fn prefer_parse_handling_lenient() {
        let prefer = PreferHeader::parse("handling=lenient");
        assert_eq!(prefer.handling, Some(HandlingPreference::Lenient));
    }

    #[test]
    fn prefer_parse_max_affected() {
        let prefer = PreferHeader::parse("max-affected=100");
        assert_eq!(prefer.max_affected, Some(100));
    }

    #[test]
    fn prefer_count_preference_exact() {
        let prefer = PreferHeader::parse("count=exact");
        assert_eq!(prefer.count_preference(), Some(CountPreference::Exact));
    }

    #[test]
    fn prefer_count_preference_none() {
        let prefer = PreferHeader::parse("return=minimal");
        assert_eq!(prefer.count_preference(), None);
    }

    #[test]
    fn prefer_applied_header_value() {
        let prefer = PreferHeader::parse("count=exact, return=representation");
        let value = prefer.applied_header_value();
        assert!(value.is_some());
        let value_str = value.unwrap();
        assert!(value_str.contains("count=exact"));
        assert!(value_str.contains("return=representation"));
    }
}

// ---------------------------------------------------------------------------
// headers tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod headers {
    use axum::http::HeaderMap;

    use super::super::headers::{set_preference_applied, set_request_id};

    #[test]
    fn set_preference_applied_single() {
        let mut headers = HeaderMap::new();
        set_preference_applied(&mut headers, &["count=exact"]);
        assert_eq!(headers.get("preference-applied").unwrap().to_str().unwrap(), "count=exact");
    }

    #[test]
    fn set_preference_applied_multiple() {
        let mut headers = HeaderMap::new();
        set_preference_applied(&mut headers, &["count=exact", "return=representation"]);
        let value = headers.get("preference-applied").unwrap().to_str().unwrap();
        assert!(value.contains("count=exact"));
        assert!(value.contains("return=representation"));
    }

    #[test]
    fn set_preference_applied_empty() {
        let mut headers = HeaderMap::new();
        set_preference_applied(&mut headers, &[]);
        assert!(headers.get("preference-applied").is_none());
    }

    #[test]
    fn set_preference_applied_filters_empty() {
        let mut headers = HeaderMap::new();
        set_preference_applied(&mut headers, &["", "count=exact", ""]);
        let value = headers.get("preference-applied").unwrap().to_str().unwrap();
        assert_eq!(value, "count=exact");
    }

    #[test]
    fn set_request_id_from_request() {
        let mut request_headers = HeaderMap::new();
        request_headers.insert("x-request-id", "test-id-123".parse().unwrap());
        let mut response_headers = HeaderMap::new();
        set_request_id(&request_headers, &mut response_headers);
        assert_eq!(response_headers.get("x-request-id").unwrap().to_str().unwrap(), "test-id-123");
    }

    #[test]
    fn set_request_id_generate_new() {
        let request_headers = HeaderMap::new();
        let mut response_headers = HeaderMap::new();
        set_request_id(&request_headers, &mut response_headers);
        let id = response_headers.get("x-request-id").unwrap().to_str().unwrap();
        assert!(uuid::Uuid::parse_str(id).is_ok());
    }
}

// ---------------------------------------------------------------------------
// query tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod query {
    use serde_json::json;

    use super::super::query::{build_query_response, extract_relay_page_info};
    use crate::routes::rest::params::PaginationParams;

    #[test]
    fn build_query_response_single() {
        let result = json!({
            "data": {
                "user": {
                    "id": 1,
                    "name": "Alice"
                }
            }
        });
        let response = build_query_response(&result, None, &PaginationParams::None).unwrap();
        assert_eq!(response["data"]["id"], 1);
        assert!(!response.get("meta").is_some_and(|m| m.is_object()));
    }

    #[test]
    fn build_query_response_with_offset_pagination() {
        let result = json!({
            "data": {
                "users": [
                    {"id": 1},
                    {"id": 2}
                ]
            }
        });
        let pagination = PaginationParams::Offset {
            limit:  10,
            offset: 0,
        };
        let response = build_query_response(&result, Some(100), &pagination).unwrap();
        assert_eq!(response["meta"]["limit"], 10);
        assert_eq!(response["meta"]["offset"], 0);
        assert_eq!(response["meta"]["total"], 100);
    }

    #[test]
    fn extract_relay_page_info_present() {
        let data = json!({
            "pageInfo": {
                "hasNextPage": true,
                "hasPreviousPage": false
            }
        });
        let info = extract_relay_page_info(&data);
        assert!(info.is_some());
        assert_eq!(info.unwrap()["hasNextPage"], true);
    }

    #[test]
    fn extract_relay_page_info_missing() {
        let data = json!({"items": []});
        let info = extract_relay_page_info(&data);
        assert!(info.is_none());
    }
}

// ---------------------------------------------------------------------------
// response tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod response {
    use axum::http::StatusCode;
    use serde_json::json;

    use super::super::response::RestError;

    #[test]
    fn rest_error_bad_request() {
        let err = RestError::bad_request("test message");
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert_eq!(err.code, "BAD_REQUEST");
        assert_eq!(err.message, "test message");
    }

    #[test]
    fn rest_error_forbidden() {
        let err = RestError::forbidden();
        assert_eq!(err.status, StatusCode::FORBIDDEN);
        assert_eq!(err.code, "FORBIDDEN");
    }

    #[test]
    fn rest_error_not_found() {
        let err = RestError::not_found("resource not found");
        assert_eq!(err.status, StatusCode::NOT_FOUND);
        assert_eq!(err.code, "NOT_FOUND");
    }

    #[test]
    fn rest_error_unprocessable_entity() {
        let details = json!({"field": "name"});
        let err = RestError::unprocessable_entity("invalid entity", details.clone());
        assert_eq!(err.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(err.details, Some(details));
    }

    #[test]
    fn rest_error_internal() {
        let err = RestError::internal("internal error");
        assert_eq!(err.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.code, "INTERNAL_SERVER_ERROR");
    }

    #[test]
    fn rest_error_to_json() {
        let err = RestError::bad_request("test error");
        let json = err.to_json();
        assert_eq!(json["error"]["code"], "BAD_REQUEST");
        assert_eq!(json["error"]["message"], "test error");
    }

    #[test]
    fn rest_error_to_json_with_details() {
        let details = json!({"field": "email"});
        let err = RestError::unprocessable_entity("validation error", details.clone());
        let json = err.to_json();
        assert_eq!(json["error"]["details"], details);
    }
}

// ---------------------------------------------------------------------------
// coercion tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod coercion {
    use serde_json::json;

    use super::super::coercion::coerce_path_param_value;

    #[test]
    fn coerce_path_param_value_integer() {
        let val = coerce_path_param_value("42");
        assert_eq!(val, json!(42i64));
    }

    #[test]
    fn coerce_path_param_value_boolean_true() {
        let val = coerce_path_param_value("true");
        assert_eq!(val, json!(true));
    }

    #[test]
    fn coerce_path_param_value_boolean_false() {
        let val = coerce_path_param_value("false");
        assert_eq!(val, json!(false));
    }

    #[test]
    fn coerce_path_param_value_string() {
        let val = coerce_path_param_value("hello");
        assert_eq!(val, json!("hello"));
    }
}

// ---------------------------------------------------------------------------
// mutation tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod mutation {
    use axum::http::{HeaderMap, StatusCode};
    use serde_json::json;

    use super::super::mutation::stored_response_to_rest;
    use crate::routes::rest::idempotency::StoredResponse;

    #[test]
    fn stored_response_replay() {
        let stored = StoredResponse {
            status:  201,
            headers: vec![("x-rows-affected".to_string(), "1".to_string())],
            body:    Some(json!({"id": 1})),
        };
        let request_headers = HeaderMap::new();
        let rest = stored_response_to_rest(stored, &request_headers);
        assert_eq!(rest.status, StatusCode::CREATED);
        assert_eq!(rest.headers.get("idempotency-key").unwrap().to_str().unwrap(), "replayed=true");
        assert_eq!(rest.body.unwrap()["id"], 1);
    }
}
