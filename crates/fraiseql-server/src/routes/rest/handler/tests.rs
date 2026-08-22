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

    // #413: REST maps client-input DB faults (SQLSTATE 22xxx/23xxx) to 400, mirroring
    // the GraphQL mapper. Genuine server faults (other classes, none, pool) stay 500.
    #[test]
    fn rest_error_from_sqlstate_22_is_bad_user_input_400() {
        let err = RestError::from(fraiseql_error::FraiseQLError::Database {
            message:   "invalid input syntax for type uuid: \"not-a-uuid\"".into(),
            sql_state: Some("22P02".into()),
        });
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert_eq!(err.code, "BAD_USER_INPUT");
        assert!(err.message.contains("not-a-uuid"));
    }

    #[test]
    fn rest_error_from_sqlstate_23_is_constraint_violation_400() {
        for code in ["23502", "23503", "23505", "23514"] {
            let err = RestError::from(fraiseql_error::FraiseQLError::Database {
                message:   "violates constraint".into(),
                sql_state: Some(code.into()),
            });
            assert_eq!(err.status, StatusCode::BAD_REQUEST, "SQLSTATE {code}");
            assert_eq!(err.code, "CONSTRAINT_VIOLATION", "SQLSTATE {code}");
        }
    }

    #[test]
    fn rest_error_from_non_client_database_stays_500() {
        for sql_state in [Some("08006".to_string()), None] {
            let err = RestError::from(fraiseql_error::FraiseQLError::Database {
                message: "connection failure".into(),
                sql_state,
            });
            assert_eq!(err.status, StatusCode::INTERNAL_SERVER_ERROR);
            assert_eq!(err.code, "INTERNAL_SERVER_ERROR");
        }
        let pool = RestError::from(fraiseql_error::FraiseQLError::ConnectionPool {
            message: "pool exhausted".into(),
        });
        assert_eq!(pool.status, StatusCode::INTERNAL_SERVER_ERROR);
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
    use fraiseql_core::schema::{
        ArgumentDefinition, CompiledSchema, FieldType, MutationDefinition,
    };
    use serde_json::json;

    use super::super::coercion::{coerce_path_param_value, declared_arg_type};

    /// A schema whose `update_thing` mutation declares one argument per type the
    /// coercion has to distinguish.
    fn schema() -> CompiledSchema {
        let mut mutation = MutationDefinition::new("update_thing", "Thing");
        for (name, ty) in [
            ("id", FieldType::Id),
            ("code", FieldType::String),
            ("seq", FieldType::Int),
            ("ratio", FieldType::Float),
            ("active", FieldType::Boolean),
            ("price", FieldType::Decimal),
        ] {
            mutation.arguments.push(ArgumentDefinition::new(name, ty));
        }
        let mut schema = CompiledSchema {
            mutations: vec![mutation],
            ..CompiledSchema::default()
        };
        schema.build_indexes();
        schema
    }

    fn coerce(arg: &str, value: &str) -> serde_json::Value {
        let schema = schema();
        coerce_path_param_value(value, declared_arg_type(&schema, "update_thing", arg))
    }

    #[test]
    fn an_int_argument_is_coerced_to_a_number() {
        assert_eq!(coerce("seq", "42"), json!(42i64));
    }

    #[test]
    fn a_float_argument_is_coerced_to_a_number() {
        assert_eq!(coerce("ratio", "1.5"), json!(1.5f64));
    }

    #[test]
    fn a_boolean_argument_is_coerced_to_a_bool() {
        assert_eq!(coerce("active", "true"), json!(true));
        assert_eq!(coerce("active", "false"), json!(false));
    }

    // ── #731: the defect — coercing by parse-ability, not by declared type ───

    /// The headline case: a leading-zero string ID survives. Under the old
    /// parse-ability heuristic `"0123"` became the integer `123`, so the row the
    /// client addressed and the row the server updated were different rows.
    #[test]
    fn a_numeric_looking_id_stays_a_string() {
        assert_eq!(coerce("id", "0123"), json!("0123"));
        assert_eq!(coerce("code", "42"), json!("42"));
    }

    /// `"true"` is a perfectly ordinary string ID.
    #[test]
    fn a_boolean_looking_string_stays_a_string() {
        assert_eq!(coerce("code", "true"), json!("true"));
        assert_eq!(coerce("id", "false"), json!("false"));
    }

    /// `Decimal` is carried as a string for precision; parsing it to a float
    /// would silently drop digits.
    #[test]
    fn a_decimal_argument_keeps_its_precision_as_a_string() {
        assert_eq!(coerce("price", "10.000000000000000001"), json!("10.000000000000000001"));
    }

    /// An argument the schema does not declare is not a licence to guess.
    #[test]
    fn an_undeclared_argument_stays_a_string() {
        assert_eq!(coerce("nonexistent", "42"), json!("42"));
        assert_eq!(coerce_path_param_value("42", None), json!("42"));
    }

    /// Counterweight: a value that does not parse as its declared type is passed
    /// through unchanged so the input validator can report it, rather than being
    /// silently replaced.
    #[test]
    fn an_unparseable_value_reaches_the_validator_intact() {
        assert_eq!(coerce("seq", "not-a-number"), json!("not-a-number"));
        assert_eq!(coerce("active", "yes"), json!("yes"));
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

// ---------------------------------------------------------------------------
// error→HTTP status convergence (M-rest-error-mapper, L-error-map-triplication)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod error_status {
    use axum::http::StatusCode;
    use fraiseql_error::FraiseQLError;

    use crate::routes::rest::handler::response::RestError;

    /// The REST HTTP status for every error variant equals the canonical
    /// [`FraiseQLError::status_code`] — the single source of truth. Guards against
    /// the reintroduction of a divergent hand-rolled map (L-error-map-triplication)
    /// and the bug where Conflict/Timeout/RateLimited/ServiceUnavailable silently
    /// collapsed to 500 (M-rest-error-mapper). The `Database` SQLSTATE override is
    /// the one documented divergence, checked separately below.
    #[test]
    fn rest_status_matches_canonical_status_code() {
        let cases = vec![
            FraiseQLError::validation("m"),
            FraiseQLError::not_found("User", "1"),
            FraiseQLError::Authorization {
                message:  "m".into(),
                action:   None,
                resource: None,
            },
            FraiseQLError::Authentication {
                message: "m".into(),
            },
            FraiseQLError::Conflict {
                message: "m".into(),
            },
            FraiseQLError::RateLimited {
                message:          "m".into(),
                retry_after_secs: 1,
            },
            FraiseQLError::Timeout {
                timeout_ms: 1,
                query:      None,
            },
            FraiseQLError::cancelled("q", "r"),
            FraiseQLError::Unsupported {
                message: "m".into(),
            },
            FraiseQLError::ServiceUnavailable {
                message:     "m".into(),
                retry_after: None,
            },
            FraiseQLError::Configuration {
                message: "m".into(),
            },
            FraiseQLError::internal("m"),
        ];
        for err in cases {
            let expected = err.status_code();
            let rest = RestError::from(err);
            assert_eq!(
                rest.status.as_u16(),
                expected,
                "REST status must equal the canonical status_code() ({expected}); got {} ({})",
                rest.status.as_u16(),
                rest.code,
            );
        }
    }

    /// The specific variants that previously collapsed to 500 now map correctly.
    #[test]
    fn rest_maps_specific_variants_off_500() {
        assert_eq!(
            RestError::from(FraiseQLError::Conflict {
                message: "x".into(),
            })
            .status,
            StatusCode::CONFLICT,
        );
        assert_eq!(
            RestError::from(FraiseQLError::RateLimited {
                message:          "x".into(),
                retry_after_secs: 1,
            })
            .status,
            StatusCode::TOO_MANY_REQUESTS,
        );
        assert_eq!(
            RestError::from(FraiseQLError::ServiceUnavailable {
                message:     "x".into(),
                retry_after: None,
            })
            .status,
            StatusCode::SERVICE_UNAVAILABLE,
        );
        assert_eq!(
            RestError::from(FraiseQLError::Unsupported {
                message: "x".into(),
            })
            .status,
            StatusCode::NOT_IMPLEMENTED,
        );
    }

    /// #413: client-input DB faults (SQLSTATE 22xxx/23xxx) are 400 — the one place
    /// the REST status intentionally diverges from `status_code()` (which is 500 for
    /// all `Database`). Genuine server DB faults stay 500.
    #[test]
    fn rest_database_client_input_is_400_server_fault_is_500() {
        let client = FraiseQLError::Database {
            message:   "bad input".into(),
            sql_state: Some("22001".into()),
        };
        assert_eq!(RestError::from(client).status, StatusCode::BAD_REQUEST);

        let server = FraiseQLError::Database {
            message:   "boom".into(),
            sql_state: None,
        };
        assert_eq!(RestError::from(server).status, StatusCode::INTERNAL_SERVER_ERROR);
    }
}

// ---------------------------------------------------------------------------
// #1153: which errors carry database-written text
// ---------------------------------------------------------------------------

#[cfg(test)]
mod database_text_provenance {
    use super::super::response::RestError;
    use crate::error::ErrorCode;

    /// The two surfaces must agree on which errors carry database text, or one of
    /// them sanitizes less than the other and the deployment's control means
    /// different things depending on the transport the caller picked. That is #808's
    /// shape, and #966 paid for it once already.
    ///
    /// The REST side asks the question of a wire string because `RestError.code` is
    /// one; this pins that string list to the enum it mirrors, in **both**
    /// directions — a code added to one list and not the other fails here.
    #[test]
    fn database_text_codes_agree_across_surfaces() {
        // Every code, with the answer stated once. Serde renders `ErrorCode` in
        // SCREAMING_SNAKE_CASE, which is exactly the REST spelling.
        let cases = [
            (ErrorCode::InternalServerError, "INTERNAL_SERVER_ERROR", true),
            (ErrorCode::DatabaseError, "DATABASE_ERROR", true),
            (ErrorCode::BadUserInput, "BAD_USER_INPUT", true),
            (ErrorCode::ConstraintViolation, "CONSTRAINT_VIOLATION", true),
            // Client-authored messages: no database text, must keep passing through.
            (ErrorCode::ValidationError, "VALIDATION_ERROR", false),
            (ErrorCode::Unauthenticated, "UNAUTHENTICATED", false),
            (ErrorCode::Forbidden, "FORBIDDEN", false),
            (ErrorCode::NotFound, "NOT_FOUND", false),
            (ErrorCode::Conflict, "CONFLICT", false),
            (ErrorCode::RequestError, "REQUEST_ERROR", false),
        ];

        for (code, wire, expected) in cases {
            assert_eq!(code.carries_database_text(), expected, "ErrorCode::{code:?} provenance");

            // The wire spelling in the table is the one serde actually emits — so a
            // renamed variant fails here rather than silently unpinning the REST side.
            let rendered = serde_json::to_string(&code).unwrap();
            assert_eq!(rendered, format!("\"{wire}\""), "wire spelling of {code:?}");

            let rest = RestError {
                status:  code.status_code(),
                code:    wire,
                message: "x".to_string(),
                details: None,
            };
            assert_eq!(
                rest.carries_database_text(),
                expected,
                "RestError {wire} must agree with ErrorCode::{code:?}"
            );
        }
    }

    /// The message class is chosen from the code, so a 400 is never answered with a
    /// sentence asserting an internal error occurred.
    #[test]
    fn a_client_fault_maps_to_its_own_error_code() {
        let cases = [
            ("BAD_USER_INPUT", ErrorCode::BadUserInput),
            ("CONSTRAINT_VIOLATION", ErrorCode::ConstraintViolation),
            ("DATABASE_ERROR", ErrorCode::InternalServerError),
            ("INTERNAL_SERVER_ERROR", ErrorCode::InternalServerError),
        ];
        for (wire, expected) in cases {
            let rest = RestError {
                status:  expected.status_code(),
                code:    wire,
                message: "x".to_string(),
                details: None,
            };
            assert_eq!(rest.database_error_code(), expected, "{wire}");
        }
    }
}
