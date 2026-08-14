#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code — panics are acceptable
//! #846: a per-operation `rest` annotation must survive compilation.
//!
//! Every authoring SDK emits `"rest": {"path": …, "method": …}` on queries and mutations.
//! The compiled `QueryDefinition`/`MutationDefinition` carry `rest_path`/`rest_method`,
//! and the server's route derivation reads them as the path override. In between,
//! `IntermediateQuery`/`IntermediateMutation` declared no such field — and the
//! intermediate schema has no `deny_unknown_fields` — so serde discarded the block and
//! both converter sites wrote `None` unconditionally.
//!
//! The result was a clean compile and a 404. Worse, `detect_conflicts` answers a route
//! collision with "Use `rest_path` override to resolve" — instructing operators to use
//! the one mechanism the compiler could not carry.
//!
//! **Why the SDK cases are enumerated rather than folded into one.** They are byte-identical
//! today, and that is the fact worth pinning: eight independent producers agreeing on one
//! wire shape is what makes a single consumer correct. If one SDK ever renames its key, a
//! test named for that SDK fails and says so — whereas a single generic case would keep
//! passing while that SDK's users silently lost their routes. That is precisely the shape
//! of #806, where three SDKs emitted `inject` against a consumer reading `inject_params`.

use fraiseql_cli::schema::{IntermediateSchema, SchemaConverter};

/// The type + query + mutation scaffolding every case shares, with `$REST_Q` and
/// `$REST_M` substituted into the query and mutation objects.
fn schema_json(rest_query: &str, rest_mutation: &str) -> String {
    format!(
        r#"{{
          "types": [
            {{
              "name": "Order",
              "fields": [
                {{"name": "id", "type": "ID", "nullable": false}},
                {{"name": "total", "type": "Int", "nullable": false}}
              ]
            }}
          ],
          "queries": [
            {{
              "name": "orders",
              "return_type": "Order",
              "returns_list": true,
              "sql_source": "v_order"{rest_query}
            }}
          ],
          "mutations": [
            {{
              "name": "createOrder",
              "return_type": "Order",
              "sql_source": "fn_create_order",
              "operation": "CREATE"{rest_mutation}
            }}
          ]
        }}"#
    )
}

/// Compile raw intermediate JSON and return the compiled `(query, mutation)` REST pair.
///
/// Goes through `IntermediateSchema`'s real deserializer and the real converter — the
/// two components between which the annotation was being dropped. Constructing an
/// `IntermediateQuery` in Rust instead would skip the deserialize step, which is exactly
/// where the loss occurred.
type RestPair = (Option<String>, Option<String>);

fn compile(json: &str) -> (RestPair, RestPair) {
    let intermediate: IntermediateSchema =
        serde_json::from_str(json).expect("intermediate schema must deserialize");
    let compiled = SchemaConverter::convert(intermediate).expect("schema must compile");

    let q = &compiled.queries[0];
    let m = &compiled.mutations[0];
    (
        (q.rest_path.clone(), q.rest_method.clone()),
        (m.rest_path.clone(), m.rest_method.clone()),
    )
}

// ---------------------------------------------------------------------------
// Per-SDK wire shapes
// ---------------------------------------------------------------------------

/// The `rest` block as each SDK emits it, with the source that proves the shape.
///
/// All eight are identical. Kept enumerated so a divergence names its SDK.
const SDK_SHAPES: &[(&str, &str)] = &[
    // decorators.py:125 — `cfg["rest"] = {"path": rest_path, "method": method}`
    ("python", r#"{"path": "/api/v1/orders", "method": "GET"}"#),
    // registry.go:38-42 — `Path string \`json:"path"\``, `Method string \`json:"method"\``
    ("go", r#"{"path": "/api/v1/orders", "method": "GET"}"#),
    // Schema/QueryBuilder.php:219-222
    ("php", r#"{"path": "/api/v1/orders", "method": "GET"}"#),
    // Models/RestAnnotation.cs — JsonPropertyName("path") / ("method")
    ("csharp", r#"{"path": "/api/v1/orders", "method": "GET"}"#),
    // SchemaFormatter.java:295-298 — restNode.put("path"/"method")
    ("java", r#"{"path": "/api/v1/orders", "method": "GET"}"#),
    // schema_exporter.ex:186-188 — %{"path" => path, "method" => method}
    ("elixir", r#"{"path": "/api/v1/orders", "method": "GET"}"#),
    // Types.fs:58-66 — RestConfig option
    ("fsharp", r#"{"path": "/api/v1/orders", "method": "GET"}"#),
    // The Rust SDK's serde representation.
    ("rust", r#"{"path": "/api/v1/orders", "method": "GET"}"#),
];

/// Every SDK's emitted `rest` block must reach the compiled artifact.
#[test]
fn every_sdk_rest_annotation_survives_compilation() {
    for (sdk, shape) in SDK_SHAPES {
        let json = schema_json(&format!(r#", "rest": {shape}"#), "");
        let ((path, method), _) = compile(&json);

        assert_eq!(
            path.as_deref(),
            Some("/api/v1/orders"),
            "{sdk}: authored rest.path was dropped during compilation"
        );
        assert_eq!(
            method.as_deref(),
            Some("GET"),
            "{sdk}: authored rest.method was dropped during compilation"
        );
    }
}

/// The same for mutations — a separate converter site, which is why it is asserted
/// separately. Both wrote `None` independently.
#[test]
fn every_sdk_rest_annotation_survives_compilation_on_mutations() {
    for (sdk, _) in SDK_SHAPES {
        let shape = r#"{"path": "/api/v1/orders/new", "method": "PUT"}"#;
        let json = schema_json("", &format!(r#", "rest": {shape}"#));
        let (_, (path, method)) = compile(&json);

        assert_eq!(
            path.as_deref(),
            Some("/api/v1/orders/new"),
            "{sdk}: authored mutation rest.path was dropped during compilation"
        );
        assert_eq!(
            method.as_deref(),
            Some("PUT"),
            "{sdk}: authored mutation rest.method was dropped during compilation"
        );
    }
}

/// The control: an operation with no `rest` block compiles to `None`, so the tests above
/// cannot be satisfied by a converter that invents a path.
#[test]
fn an_operation_without_a_rest_block_carries_no_override() {
    let ((qp, qm), (mp, mm)) = compile(&schema_json("", ""));
    assert_eq!(qp, None);
    assert_eq!(qm, None);
    assert_eq!(mp, None);
    assert_eq!(mm, None);
}

/// `method` is optional; the server derives `GET` for a query and `POST` for a mutation.
#[test]
fn a_rest_block_without_a_method_carries_the_path_alone() {
    let json = schema_json(r#", "rest": {"path": "/api/v1/orders"}"#, "");
    let ((path, method), _) = compile(&json);
    assert_eq!(path.as_deref(), Some("/api/v1/orders"));
    assert_eq!(method, None, "an absent method must stay absent, not become a guess");
}

/// A lowercase verb is normalised rather than rejected — `parse_http_method` on the
/// server matches uppercase only, so passing `"get"` through unchanged would silently
/// fall back to the derived default.
#[test]
fn a_lowercase_method_is_normalised_to_uppercase() {
    let json = schema_json(r#", "rest": {"path": "/api/v1/orders", "method": "get"}"#, "");
    let ((_, method), _) = compile(&json);
    assert_eq!(method.as_deref(), Some("GET"));
}

// ---------------------------------------------------------------------------
// Loud failure, not silent degradation
// ---------------------------------------------------------------------------

/// Assert compilation fails, and that the message names the reason.
///
/// Asserting only `is_err()` would accept a failure for any reason at all — including a
/// malformed fixture — which is the trap that let two earlier suites in this phase pass
/// against unfixed code.
fn assert_compile_error(json: &str, expected_substring: &str) {
    let parsed: Result<IntermediateSchema, _> = serde_json::from_str(json);
    let message = match parsed {
        Err(e) => e.to_string(),
        Ok(intermediate) => match SchemaConverter::convert(intermediate) {
            Ok(_) => panic!("expected compilation to fail, but it succeeded"),
            Err(e) => format!("{e:#}"),
        },
    };
    assert!(
        message.contains(expected_substring),
        "error must name the reason; wanted {expected_substring:?}, got {message:?}"
    );
}

/// An unsupported verb must fail the compile.
///
/// The server reads `rest_method` through `parse_http_method(..).unwrap_or(GET)`, so
/// `"FETCH"` would silently become a `GET` route at the authored path — a route the
/// author never asked for, with no diagnostic anywhere.
#[test]
fn an_unsupported_http_method_is_refused() {
    let json = schema_json(r#", "rest": {"path": "/api/v1/orders", "method": "FETCH"}"#, "");
    assert_compile_error(&json, "not a supported HTTP method");
}

/// A path that is not a path must fail rather than become a route nothing can reach.
#[test]
fn a_path_without_a_leading_slash_is_refused() {
    let json = schema_json(r#", "rest": {"path": "api/v1/orders"}"#, "");
    assert_compile_error(&json, "must start with '/'");
}

#[test]
fn an_empty_path_is_refused() {
    let json = schema_json(r#", "rest": {"path": ""}"#, "");
    assert_compile_error(&json, "must not be empty");
}

/// A query string belongs to the request, not the route template.
#[test]
fn a_path_carrying_a_query_string_is_refused() {
    let json = schema_json(r#", "rest": {"path": "/api/v1/orders?status=open"}"#, "");
    assert_compile_error(&json, "path only");
}

/// `deny_unknown_fields` on the `rest` block: a typo must fail, not degrade.
///
/// This is the property the whole issue is about, applied one level down. `"pathh"` with
/// a permissive deserializer would produce a block with a missing required `path` — or,
/// had `path` been optional, a silently empty override.
#[test]
fn an_unknown_key_inside_the_rest_block_is_refused() {
    let json = schema_json(r#", "rest": {"pathh": "/api/v1/orders"}"#, "");
    assert_compile_error(&json, "pathh");
}

// ---------------------------------------------------------------------------
// `rest_stream` — the per-route streaming opt-in (#958)
// ---------------------------------------------------------------------------

/// Compile raw intermediate JSON and return the first query's `rest_stream`.
fn compile_rest_stream(json: &str) -> bool {
    let intermediate: IntermediateSchema =
        serde_json::from_str(json).expect("intermediate schema must deserialize");
    let compiled = SchemaConverter::convert(intermediate).expect("schema must compile");
    compiled.queries[0].rest_stream
}

/// The authored flag must reach the compiled artifact, because that is where the
/// server reads it. This is the #846 defect class exactly: a key every producer
/// emits and no consumer receives compiles clean and does nothing.
#[test]
fn rest_stream_survives_compilation() {
    assert!(compile_rest_stream(&schema_json(r#", "rest_stream": true"#, "")));
}

/// Absent means off. A streaming export reads the whole relation and holds a
/// database connection for the client's whole read, so the default cannot be "yes".
#[test]
fn an_operation_without_rest_stream_does_not_opt_in() {
    assert!(!compile_rest_stream(&schema_json("", "")));
    assert!(!compile_rest_stream(&schema_json(r#", "rest_stream": false"#, "")));
}

/// `rest_stream` on a single-item query is refused at compile time.
///
/// The streaming representations deliver a sequence of rows; a query that returns
/// one has no sequence. Compile time is where this has to fail — it is the last
/// point at which the authored intent is still visible, the same argument the
/// unsupported-verb refusal above makes.
#[test]
fn rest_stream_on_a_single_item_query_is_refused() {
    let json = r#"{
      "types": [
        {
          "name": "Order",
          "fields": [{"name": "id", "type": "ID", "nullable": false}]
        }
      ],
      "queries": [
        {
          "name": "order",
          "return_type": "Order",
          "returns_list": false,
          "sql_source": "v_order",
          "rest_stream": true
        }
      ],
      "mutations": []
    }"#;
    assert_compile_error(json, "requires a list-returning query");
}
