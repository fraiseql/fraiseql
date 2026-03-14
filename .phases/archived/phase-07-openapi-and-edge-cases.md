# Phase 07: OpenAPI Spec Generation & REST Edge Cases

## Objective
Generate an OpenAPI 3.1 specification dynamically from the compiled schema's
REST routes, and handle all REST response edge cases that Phase 06 deferred.
Requires Phase 06 (REST transport).

## Context

The existing `routes/api/openapi.rs` is a **static, hand-written** OpenAPI
3.0.0 spec covering 10 internal/admin endpoints. This phase adds a **dynamic**
OpenAPI generator that reads `CompiledSchema.rest_routes` and emits a spec
for the user-defined REST endpoints. Both specs coexist.

---

## Cycle 1 — OpenAPI spec generator

### Architecture

The generator is a compile-time function (not a runtime service). It reads the
compiled schema and produces OpenAPI JSON. It runs:

1. **At compilation** (`fraiseql-cli compile`): embeds the spec in
   `schema.compiled.json` as `rest.openapi_spec`
2. **At server startup**: serves the pre-generated spec at a configurable path

```
CompiledSchema
  ├── rest_routes: Vec<RestRoute>     ← input
  ├── rest_config: RestConfig         ← input (prefix, auth)
  ├── types: Vec<TypeDefinition>      ← input (for component schemas)
  └── rest.openapi_spec: String       ← output (generated JSON)
```

### New file: `crates/fraiseql-core/src/schema/compiled/openapi_gen.rs`

```rust
use serde_json::{json, Value};
use super::schema::CompiledSchema;
use super::rest::{RestRoute, RestConfig};

/// Generate an OpenAPI 3.1.0 specification from compiled REST routes.
///
/// # Arguments
///
/// * `schema` - The compiled schema (provides type info for component schemas)
/// * `config` - REST transport configuration (prefix, auth mode)
///
/// # Returns
///
/// OpenAPI 3.1.0 spec as a JSON string.
pub fn generate_openapi_spec(schema: &CompiledSchema, config: &RestConfig) -> String {
    let mut spec = json!({
        "openapi": "3.1.0",
        "info": {
            "title": config.title.as_deref().unwrap_or("FraiseQL REST API"),
            "version": config.api_version.as_deref().unwrap_or("1.0.0"),
            "description": "Auto-generated REST API from FraiseQL schema"
        },
        "paths": {},
        "components": {
            "schemas": {}
        }
    });

    // Build paths from rest_routes
    for route in &schema.rest_routes {
        let full_path = format!("{}{}", config.prefix, route.path);
        let path_item = build_path_item(route, schema, config);
        spec["paths"][&full_path] = path_item;
    }

    // Build component schemas from GraphQL types referenced by routes
    let referenced_types = collect_referenced_types(&schema.rest_routes, schema);
    for type_name in &referenced_types {
        if let Some(type_def) = schema.types.iter().find(|t| t.name == *type_name) {
            spec["components"]["schemas"][type_name] = type_to_json_schema(type_def);
        }
    }

    // Add security scheme if auth is required
    if config.auth == "required" || config.auth == "optional" {
        spec["components"]["securitySchemes"] = json!({
            "BearerAuth": {
                "type": "http",
                "scheme": "bearer",
                "bearerFormat": "JWT"
            }
        });
    }

    serde_json::to_string_pretty(&spec)
        .expect("OpenAPI spec serialization cannot fail")
}
```

### Key helper functions to implement

**`build_path_item(route, schema, config) -> Value`**:
- For each route, emit the appropriate HTTP method object
- Path params → `parameters` array with `in: "path"`
- Query args (non-path-param arguments) → `parameters` array with `in: "query"`
- For POST/PUT/PATCH mutations → `requestBody` with JSON schema
- Response schema: look up `return_type` in `schema.types` and reference it
- Add `security` block if `config.auth == "required"`

**`type_to_json_schema(type_def: &TypeDefinition) -> Value`**:
- Map GraphQL types to JSON Schema types:
  - `String` → `{"type": "string"}`
  - `Int` → `{"type": "integer"}`
  - `Float` → `{"type": "number"}`
  - `Boolean` → `{"type": "boolean"}`
  - `ID` → `{"type": "string"}`
  - Custom objects → `{"type": "object", "properties": {...}}`
  - Lists → `{"type": "array", "items": {...}}`
- `nullable` fields get `"nullable": true` (OpenAPI 3.0) or `"type": ["string", "null"]` (3.1)
- Include `description` from field definitions when present

**`collect_referenced_types(routes, schema) -> HashSet<String>`**:
- Walk all routes' `operation_name` → find the query/mutation → get its `return_type`
- Recursively collect all types referenced by those return types (nested objects)
- Stop recursion at scalar types

### Tests (in `openapi_gen.rs` `#[cfg(test)]` block)

```rust
#[test]
fn test_generates_valid_openapi_31() {
    let schema = test_schema_with_rest_routes();
    let config = RestConfig { enabled: true, prefix: "/rest/v1".into(), .. };
    let spec_json = generate_openapi_spec(&schema, &config);
    let spec: Value = serde_json::from_str(&spec_json).unwrap();
    assert_eq!(spec["openapi"], "3.1.0");
}

#[test]
fn test_path_params_appear_in_parameters() {
    // GET /users/{id} should have id as a path parameter
}

#[test]
fn test_mutation_has_request_body() {
    // POST /users should have a requestBody schema
}

#[test]
fn test_return_type_referenced_in_components() {
    // User type should appear in components/schemas
}

#[test]
fn test_nested_types_included() {
    // If User has an Address field, Address appears in components/schemas
}

#[test]
fn test_list_return_produces_array_schema() {
    // query returning [User] → response schema is array of $ref User
}

#[test]
fn test_security_added_when_auth_required() {
    // config.auth == "required" → security block on every operation
}

#[test]
fn test_no_security_when_auth_none() {
    // config.auth == "none" → no security block
}
```

### Verification
```bash
cargo test -p fraiseql-core -- openapi
cargo clippy -p fraiseql-core -- -D warnings
```

---

## Cycle 2 — `RestConfig` extensions for OpenAPI

### Add optional fields to `RestConfig`

In `crates/fraiseql-core/src/schema/compiled/rest.rs`, extend `RestConfig`:

```rust
pub struct RestConfig {
    pub enabled: bool,
    pub prefix: String,
    pub auth: String,
    pub max_body_bytes: usize,
    // New fields for OpenAPI:
    #[serde(default)]
    pub openapi_enabled: bool,          // serve spec at startup
    #[serde(default = "default_openapi_path")]
    pub openapi_path: String,           // default: "/rest/openapi.json"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,          // OpenAPI info.title
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,    // OpenAPI info.version
}

fn default_openapi_path() -> String { "/rest/openapi.json".to_string() }
```

### `fraiseql.toml` configuration

```toml
[fraiseql.rest]
enabled = true
prefix = "/rest/v1"
auth = "required"

[fraiseql.rest.openapi]
enabled = true
path = "/rest/openapi.json"     # where to serve the spec
title = "My API"                 # info.title
version = "1.0.0"               # info.version
```

### Compiler integration

In the compiler (`fraiseql-cli`), after REST route extraction, call the
generator and embed the result:

```rust
if rest_config.openapi_enabled && !rest_routes.is_empty() {
    let spec = openapi_gen::generate_openapi_spec(&compiled_schema, &rest_config);
    compiled_schema.rest_openapi_spec = Some(spec);
}
```

Add to `CompiledSchema`:
```rust
/// Pre-generated OpenAPI specification for REST endpoints.
/// Generated at compile time, served at runtime.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub rest_openapi_spec: Option<String>,
```

---

## Cycle 3 — Serve OpenAPI spec at runtime

### Server route

In `crates/fraiseql-server/src/routes/rest/mod.rs`, add to `build_rest_router`:

```rust
// Serve OpenAPI spec if available
if rest_config.openapi_enabled {
    if let Some(ref spec) = schema.rest_openapi_spec {
        let spec_clone = spec.clone();
        router = router.route(
            &rest_config.openapi_path,
            get(move || async move {
                (
                    [(header::CONTENT_TYPE, "application/json")],
                    spec_clone.clone(),
                )
            }),
        );
    }
}
```

### Tests

- `test_openapi_endpoint_returns_json` — GET `/rest/openapi.json` returns 200
  with `Content-Type: application/json`
- `test_openapi_endpoint_disabled` — when `openapi_enabled = false`, returns 404
- `test_openapi_spec_matches_routes` — spec's paths match actual mounted routes

### Verification
```bash
cargo test -p fraiseql-server --features rest-transport -- openapi
cargo clippy -p fraiseql-server --features rest-transport -- -D warnings
```

---

## Cycle 4 — REST response edge cases

These edge cases were deferred from Phase 06. Implement them in
`crates/fraiseql-server/src/routes/rest/translator.rs`.

### Edge case 1: Partial GraphQL response (data + errors)

When the GraphQL executor returns both `data` and `errors` (e.g., a list query
where some items resolved but one field-level error occurred):

```json
// GraphQL response
{
  "data": { "users": [{"id": 1, "name": "Alice"}, {"id": 2, "name": null}] },
  "errors": [{"message": "Permission denied for field 'name' on User:2", "path": ["users", 1, "name"]}]
}
```

**REST behavior**: Return HTTP 200 with a wrapper:
```json
{
  "data": [{"id": 1, "name": "Alice"}, {"id": 2, "name": null}],
  "errors": [{"message": "Permission denied for field 'name' on User:2", "path": ["users", 1, "name"]}],
  "_partial": true
}
```

The `_partial: true` flag tells REST clients that the response is incomplete.

### Edge case 2: Null data with errors

When `data` is null and errors are present (query-level failure):

```json
{ "data": null, "errors": [{"message": "Not authenticated"}] }
```

**REST behavior**: Map to HTTP status based on error classification:
- `extensions.code == "UNAUTHENTICATED"` → 401
- `extensions.code == "FORBIDDEN"` → 403
- `extensions.code == "NOT_FOUND"` → 404
- `extensions.code == "VALIDATION_ERROR"` → 400
- `extensions.code == "RATE_LIMITED"` → 429
- All others → 500

Return the errors array as the JSON body.

### Edge case 3: Empty list result

When a list query returns empty data:

```json
{ "data": { "users": [] }, "errors": null }
```

**REST behavior**: Return HTTP 200 with empty array `[]`. Not 404.

### Edge case 4: Single-item query returns null

When a single-item query (e.g., `getUser(id: 999)`) returns null:

```json
{ "data": { "getUser": null }, "errors": null }
```

**REST behavior**: Return HTTP 404 Not Found with:
```json
{ "error": "Not found", "operation": "getUser" }
```

This is the expected REST semantic for `GET /users/999` when user doesn't exist.

### Edge case 5: Request body too large

When the POST body exceeds `rest_config.max_body_bytes`:

**REST behavior**: Return HTTP 413 Payload Too Large before reaching the executor.
This is handled by axum's `DefaultBodyLimit` layer (already in `build_router`),
but verify it applies to REST routes nested under the prefix.

### Tests for edge cases

Add to `crates/fraiseql-server/src/routes/rest/tests.rs`:

```rust
#[test]
fn test_partial_response_returns_200_with_partial_flag() { ... }

#[test]
fn test_null_data_auth_error_returns_401() { ... }

#[test]
fn test_null_data_validation_error_returns_400() { ... }

#[test]
fn test_null_data_generic_error_returns_500() { ... }

#[test]
fn test_empty_list_returns_200_empty_array() { ... }

#[test]
fn test_single_item_null_returns_404() { ... }

#[test]
fn test_body_limit_returns_413() { ... }
```

### Verification
```bash
cargo test -p fraiseql-server --features rest-transport -- rest
cargo clippy -p fraiseql-server --features rest-transport -- -D warnings
```

---

## Cycle 5 — Documentation

### Update existing OpenAPI spec

The static spec in `routes/api/openapi.rs` covers admin/internal endpoints.
Add a note to its module doc:

```rust
//! OpenAPI specification for FraiseQL internal APIs (admin, federation, query intelligence).
//!
//! For user-defined REST endpoint specs, see the dynamic OpenAPI generator in
//! `fraiseql-core::schema::compiled::openapi_gen`, served at the path configured
//! in `[fraiseql.rest.openapi]`.
```

### User documentation

Add a section to the REST transport docs explaining:
- How to enable OpenAPI spec generation in `fraiseql.toml`
- Where to find the generated spec (default: `/rest/openapi.json`)
- What type information is included (GraphQL types → JSON Schema)
- Limitations: custom scalars map to `string` by default

---

## Success Criteria
- [ ] `generate_openapi_spec()` produces valid OpenAPI 3.1.0 JSON
- [ ] Path params, query params, and request bodies are correctly represented
- [ ] GraphQL return types are mapped to JSON Schema `components/schemas`
- [ ] Nested types are recursively included
- [ ] `GET /rest/openapi.json` serves the pre-generated spec
- [ ] OpenAPI can be disabled independently of REST transport
- [ ] Partial response (data + errors) returns 200 with `_partial: true`
- [ ] Null data + auth error → 401
- [ ] Empty list → 200 with `[]`
- [ ] Single-item null → 404
- [ ] Body too large → 413
- [ ] All edge case tests pass

## Estimated Effort: 3–5 days
