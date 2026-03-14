# Phase 06: REST Transport (pre-release)

## Objective
Add an optional REST transport layer that exposes GraphQL queries and mutations
as RESTful HTTP endpoints. Behind the `rest-transport` Cargo feature flag.
No new SQL generation, no new auth logic — purely a translation layer on
top of the existing `Executor`.

**This phase runs on its own branch**, parallel to Phase 05.

## Architecture

```
Client (REST)                    Client (GraphQL)
      │                                │
      ▼                                ▼
GET  /rest/v1/users/{id}          POST /graphql
POST /rest/v1/createUser
      │
      ▼
REST Router ──────────────────── GraphQL Router
(new, routes/rest/)               (existing, routes/graphql/)
      │                                │
      └──────────────┬─────────────────┘
                     ▼
             Executor (unchanged)
                     ▼
           DatabaseAdapter (unchanged)
```

The REST router translates HTTP requests into `GraphQLRequest` values, then
delegates to the shared `Executor`. All auth middleware, rate limiting,
field-level RBAC, error sanitization, and APQ apply identically.

---

## Sub-phase A — SDK annotations (Python + TypeScript first)

### Shared `schema.json` contract

Every SDK emits the same `rest` block on a query or mutation:
```json
{
  "queries": [{
    "name": "getUser",
    "sqlSource": "get_user",
    "rest": {
      "path": "/users/{id}",
      "method": "GET",
      "pathParams": ["id"]
    }
  }],
  "mutations": [{
    "name": "createUser",
    "sqlSource": "create_user",
    "rest": {
      "path": "/users",
      "method": "POST",
      "pathParams": []
    }
  }]
}
```

Validation rule: every `{param}` in `rest_path` must correspond to a declared
argument — fail at schema authoring time, not server startup.

---

### Python SDK

**File**: `sdks/official/fraiseql-python/src/fraiseql/decorators.py`

Current `@fraiseql.query` params (from line ~429): `sql_source`, `auto_params`,
`cache_ttl_seconds`, `additional_views`, `relay`, `deprecated`, `inject`,
`description`.

Current `@fraiseql.mutation` params (from line ~435): `sql_source`, `operation`
(`"CREATE"`, `"UPDATE"`, `"DELETE"`, `"CUSTOM"`), `deprecated`, `inject`,
`invalidates_views`, `invalidates_fact_tables`, `description`.

**Add** to both decorators:
```python
rest_path: str | None = None,    # e.g. "/users/{id}"
rest_method: str | None = None,  # "GET" (default for queries), "POST" (default for mutations)
```

**Validation** (inside the decorator function, before `config` dict is built):
```python
if rest_path:
    import re
    path_params = re.findall(r'\{(\w+)\}', rest_path)
    # sig = inspect.signature(func) — already available in decorator
    for param in path_params:
        if param not in sig.parameters:
            raise ValueError(
                f"REST path param '{{{param}}}' in rest_path='{rest_path}' "
                f"does not match any argument of {func.__name__}(). "
                f"Available args: {list(sig.parameters.keys())}"
            )
```

**schema.json output**: when `rest_path` is set, add a `"rest"` key to the
query/mutation dict:
```python
if rest_path:
    config["rest"] = {
        "path": rest_path,
        "method": rest_method or ("GET" if is_query else "POST"),
        "pathParams": re.findall(r'\{(\w+)\}', rest_path),
    }
```

**Tests** (add to existing test file):
- `test_query_with_rest_annotation` — verify `schema.json` output has `rest` block
- `test_mutation_with_rest_annotation`
- `test_rest_path_param_not_in_args_raises` — `ValueError` on mismatch
- `test_rest_method_defaults` — GET for queries, POST for mutations

```bash
cd sdks/official/fraiseql-python
uv run ruff check --fix && uv run ruff format && uv run pytest
```

---

### TypeScript SDK

**File**: `sdks/official/fraiseql-typescript/src/decorators.ts`

Current `OperationConfig` interface (line 105): `sqlSource?`, `autoParams?`,
`operation?`, `jsonbColumn?`, `relay?`.

Current `MutationConfig extends OperationConfig` (line 176): adds
`operation?: "CREATE" | "UPDATE" | "DELETE" | "CUSTOM"`.

**Add** to `OperationConfig`:
```typescript
restPath?: string;
restMethod?: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';
```

**Validation**: in the decorator factory (the function that processes config),
extract `{param}` from `restPath` and verify each exists as a declared argument.

**Tests**: mirror Python tests.

```bash
cd sdks/official/fraiseql-typescript && npm test
```

---

### Remaining SDKs (after Python + TS pass cross-SDK parity)

Add REST annotations to these SDKs. Each follows the same pattern — add
`restPath`/`restMethod` to the query/mutation builder and emit the `rest`
block in schema.json output.

| SDK | File to modify | Builder pattern |
|-----|---------------|----------------|
| Go | `fraiseql/decorators.go` | Add `RESTPath(string)` and `RESTMethod(string)` methods to `QueryBuilder` (line 91+) |
| Java | Annotation interfaces in `src/main/java/com/fraiseql/core/` | Add `restPath` and `restMethod` annotation elements |
| Rust | `src/` (check existing query/mutation macros) | Add `rest_path` and `rest_method` fields |
| Ruby | `lib/fraiseql/` | Add `rest_path:` and `rest_method:` keyword args |
| PHP | `src/` | Add `restPath` and `restMethod` to PHP 8 attributes |
| C# | `src/FraiseQL/Builders/` | Add `RestPath(string)` and `RestMethod(string)` to builders |
| F# | Check existing pattern | Same as C# if shared |
| Elixir | Check existing pattern | Add `rest_path` and `rest_method` to schema DSL |

**Note**: there is no Dart SDK directory (`sdks/official/fraiseql-dart/` does
not exist despite `dart-sdk.yml` workflow). Skip Dart.

### Cross-SDK parity test

The existing parity workflow (`sdk-parity.yml`) tests Python ↔ TypeScript.
Add a fixture schema that includes `rest_path`/`rest_method` annotations and
verify both SDKs produce identical `schema.json` REST blocks.

---

## Sub-phase B — Compiler: REST route extraction

### RED
Write test in `crates/fraiseql-cli/`:
```rust
#[test]
fn test_compile_schema_with_rest_routes() {
    let schema_json = r#"{ "queries": [{ "name": "getUser", "sqlSource": "get_user",
        "rest": { "path": "/users/{id}", "method": "GET", "pathParams": ["id"] }
    }] }"#;
    let compiled = compile(schema_json).unwrap();
    assert_eq!(compiled.rest_routes.len(), 1);
    assert_eq!(compiled.rest_routes[0].path, "/users/{id}");
}
```

### GREEN

**Step 1**: Add types to `crates/fraiseql-core/src/schema/compiled/schema.rs`.

After the existing fields in `CompiledSchema` (ends at line 183), add:
```rust
/// REST transport route definitions.
/// Each entry maps an HTTP method+path to a GraphQL operation.
/// Compiled from `rest` annotations in the authoring schema.
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub rest_routes: Vec<RestRoute>,

/// REST transport configuration.
/// Compiled from the `[fraiseql.rest]` TOML section.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub rest_config: Option<RestConfig>,
```

Define `RestRoute` and `RestConfig` in a new file
`crates/fraiseql-core/src/schema/compiled/rest.rs`:
```rust
use serde::{Deserialize, Serialize};

/// A single REST route mapping HTTP → GraphQL.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RestRoute {
    pub method: String,           // "GET", "POST", etc.
    pub path: String,             // "/users/{id}"
    pub operation: String,        // "query" or "mutation"
    pub operation_name: String,   // "getUser"
    pub path_params: Vec<RestPathParam>,
}

/// A path parameter with its GraphQL type mapping.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RestPathParam {
    pub name: String,             // "id"
    pub graphql_arg: String,      // "id" (may differ)
    pub graphql_type: String,     // "Int"
}

/// REST transport configuration from `[fraiseql.rest]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RestConfig {
    pub enabled: bool,
    #[serde(default = "default_rest_prefix")]
    pub prefix: String,           // "/rest/v1"
    #[serde(default = "default_rest_auth")]
    pub auth: String,             // "required" | "optional" | "none"
    #[serde(default = "default_max_body_bytes")]
    pub max_body_bytes: usize,    // 1_048_576
}

fn default_rest_prefix() -> String { "/rest/v1".to_string() }
fn default_rest_auth() -> String { "required".to_string() }
fn default_max_body_bytes() -> usize { 1_048_576 }
```

Don't forget to add `mod rest;` to `crates/fraiseql-core/src/schema/compiled/mod.rs`
and re-export the types.

**Step 2**: Add `PartialEq` comparison for the new fields in the existing
`impl PartialEq for CompiledSchema` at line 186.

**Step 3**: In the compiler (`crates/fraiseql-cli/src/compiler/`), add a pass
that extracts `rest` blocks from queries and mutations into `rest_routes`.

Compiler validation:
- Reject duplicate `(method, path)` pairs → `FraiseQLError::Validation`
- Reject path params not present as query/mutation arguments

### `fraiseql.toml` REST configuration

```toml
[fraiseql.rest]
enabled = true
prefix = "/rest/v1"
auth = "required"              # "required" | "optional" | "none"
max_body_bytes = 1_048_576     # 1 MiB
```

Environment overrides:
- `FRAISEQL_REST_ENABLED=false`
- `FRAISEQL_REST_PREFIX=/api/v2`

### Verification
```bash
cargo clippy -p fraiseql-cli -p fraiseql-core -- -D warnings
cargo test -p fraiseql-cli
cargo test -p fraiseql-core
```

---

## Sub-phase C — Server: REST router

### Cargo feature

Add to `crates/fraiseql-server/Cargo.toml` `[features]`:
```toml
rest-transport = []   # no extra deps — axum + serde_json already present
```

### New directory: `crates/fraiseql-server/src/routes/rest/`

Existing route structure for reference:
```
routes/
├── api/          (admin, design, federation, openapi, query, schema, types)
├── graphql/      (app_state, handler, request, tests)
├── auth.rs
├── health.rs
├── introspection.rs
├── metrics.rs
├── mod.rs
├── playground.rs
└── subscriptions.rs
```

Create:
```
routes/rest/
├── mod.rs          — pub fn build_rest_router<A>(...) -> Router
├── router.rs       — iterate rest_routes, register axum routes
├── translator.rs   — HTTP ↔ GraphQL request/response translation
└── tests.rs        — unit tests
```

**`translator.rs`** key functions:

```rust
/// Convert an HTTP REST request into a GraphQL request.
///
/// Path params become GraphQL variables (type-coerced using RestPathParam info).
/// Query params (for GET) become additional variables.
/// Body (for POST/PUT/PATCH) becomes mutation variables.
pub fn translate_request(
    route: &RestRoute,
    path_params: HashMap<String, String>,
    query_params: HashMap<String, String>,
    body: Option<serde_json::Value>,
) -> GraphQLRequest { ... }

/// Convert a GraphQL response to an HTTP response.
///
/// Mapping rules:
/// - data present, no errors → 200, return data.operationName value directly
/// - data present + errors   → 200, return { "data": ..., "errors": [...] }
/// - null data + errors      → map first error to HTTP status (see below)
/// - no data, no errors      → 204 No Content
///
/// Error-to-status mapping:
/// - validation errors       → 400 Bad Request
/// - auth/permission errors  → 401 Unauthorized or 403 Forbidden
/// - not found               → 404 Not Found
/// - rate limited            → 429 Too Many Requests
/// - all other errors        → 500 Internal Server Error
pub fn translate_response(
    gql_response: GraphQLResponse,
    operation_name: &str,
) -> impl IntoResponse { ... }
```

### Integration in `Server::build_router()`

In `crates/fraiseql-server/src/server/routing.rs`, add after the MCP block
(around line 446) and before the `api::routes` line (line 449):

```rust
// REST transport routes (if enabled and compiled with feature)
#[cfg(feature = "rest-transport")]
if let Some(ref rest_cfg) = self.executor.schema().rest_config {
    if rest_cfg.enabled {
        let rest_routes = &self.executor.schema().rest_routes;
        if !rest_routes.is_empty() {
            let rest_router = crate::routes::rest::build_rest_router::<A>(
                rest_routes,
                rest_cfg,
                state.clone(),
            );
            app = app.nest(&rest_cfg.prefix, rest_router);
            info!(
                prefix = %rest_cfg.prefix,
                route_count = rest_routes.len(),
                "REST transport mounted"
            );
        }
    }
}
```

### Don't forget to update `routes/mod.rs`

Add `#[cfg(feature = "rest-transport")] pub mod rest;` to
`crates/fraiseql-server/src/routes/mod.rs`.

### Verification
```bash
cargo clippy -p fraiseql-server --features rest-transport -- -D warnings
cargo nextest run -p fraiseql-server --features rest-transport
cargo check -p fraiseql-server --no-default-features  # must still pass
```

---

## Sub-phase D — Tests

### Unit (in `routes/rest/tests.rs`)
- `test_path_param_extraction` — `/users/{id}` with id=42 → `variables: {"id": 42}`
- `test_path_param_type_coercion` — string "42" → Int 42 based on graphql_type
- `test_query_param_to_variables` — `?limit=10&offset=0` → variables
- `test_body_to_mutation_variables` — POST JSON body → mutation variables
- `test_graphql_error_maps_to_400` — validation error → HTTP 400
- `test_graphql_auth_error_maps_to_401`
- `test_unknown_rest_path_returns_404`

### Integration (in `tests/rest_transport_test.rs`)
- `test_rest_get_query_round_trip` — full round-trip with mock adapter
- `test_rest_post_mutation_round_trip`
- `test_rest_requires_auth_when_configured` — 401 without token
- `test_rest_rate_limit_enforced` — 429 on excess requests

### Feature flag matrix update

Add to `.github/workflows/feature-flags.yml` matrix:
```yaml
- "rest-transport,auth,cors"
```

---

## Deferred to Phase 07
- OpenAPI spec generation
- REST response edge cases (partial data, streaming, pagination)

---

## Success Criteria
- [ ] Python + TypeScript SDKs: `rest_path` / `rest_method` annotations working
- [ ] All SDKs with REST support pass their test suites
- [ ] Cross-SDK parity test passes with REST annotations
- [ ] Compiler: `restRoutes` in `schema.compiled.json`, path param validation
- [ ] `[fraiseql.rest]` config validated and embedded in compiled schema
- [ ] `rest-transport` feature mounts REST router in Server
- [ ] Auth + rate limiting applies identically to REST and GraphQL
- [ ] All existing tests pass with `rest-transport` disabled
- [ ] Feature Flag Matrix includes `rest-transport,auth,cors`

## Estimated Effort: 8–12 days
