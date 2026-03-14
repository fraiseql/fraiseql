use serde::{Deserialize, Serialize};

/// REST transport annotation for a single query or mutation.
///
/// When present on a `QueryDefinition` or `MutationDefinition`, the REST
/// transport layer mounts an HTTP route at `{prefix}{path}` that translates
/// the HTTP request into a GraphQL execution without an intermediate HTTP
/// round-trip.
///
/// # Example schema.json snippet
///
/// ```json
/// {
///   "name": "get_user",
///   "rest": { "path": "/users/{id}", "method": "GET" }
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestRoute {
    /// URL path pattern, e.g. `"/users/{id}"`.
    ///
    /// Curly-brace placeholders like `{id}` are expanded from the request path
    /// and passed as GraphQL arguments with the same name. Every placeholder must
    /// correspond to a declared argument of the associated query/mutation.
    pub path: String,

    /// HTTP method for this route.
    ///
    /// Deserialized case-insensitively; serialized as uppercase (e.g. `"GET"`).
    pub method: String,
}

impl RestRoute {
    /// Extract path parameter names from the path pattern.
    ///
    /// e.g. `"/users/{id}/posts/{post_id}"` → `["id", "post_id"]`
    #[must_use]
    pub fn path_params(&self) -> Vec<&str> {
        let mut params = Vec::new();
        let mut remaining = self.path.as_str();
        while let Some(start) = remaining.find('{') {
            remaining = &remaining[start + 1..];
            if let Some(end) = remaining.find('}') {
                params.push(&remaining[..end]);
                remaining = &remaining[end + 1..];
            } else {
                break;
            }
        }
        params
    }
}

/// Global REST transport configuration, from `[fraiseql.rest]` in `fraiseql.toml`.
///
/// Compiled into `CompiledSchema.rest_config` and used by the server's REST router
/// to configure the route prefix and other global settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestConfig {
    /// URL prefix for all REST routes, e.g. `"/api/v1"`.
    ///
    /// Defaults to `"/rest"` when not configured.
    #[serde(default = "default_rest_prefix")]
    pub prefix: String,

    /// Authentication requirement for REST routes.
    ///
    /// - `"none"` (default): no `Authorization` header required.
    /// - `"optional"`: header accepted but not required.
    /// - `"required"`: all routes require a valid `Bearer` JWT.
    ///
    /// When set to `"required"` or `"optional"`, a `BearerAuth` security scheme
    /// is emitted in the generated OpenAPI specification.
    #[serde(default = "default_rest_auth")]
    pub auth: String,

    /// Serve the generated OpenAPI specification at startup.
    ///
    /// When `true`, a `GET` handler is mounted at `openapi_path` that returns the
    /// pre-generated (or dynamically built) OpenAPI 3.1.0 spec for all REST routes.
    #[serde(default)]
    pub openapi_enabled: bool,

    /// URL path at which the OpenAPI spec is served.
    ///
    /// Defaults to `"/rest/openapi.json"`. Only used when `openapi_enabled` is `true`.
    #[serde(default = "default_openapi_path")]
    pub openapi_path: String,

    /// `info.title` for the generated OpenAPI spec.
    ///
    /// Defaults to `"FraiseQL REST API"` when not set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// `info.version` for the generated OpenAPI spec.
    ///
    /// Defaults to `"1.0.0"` when not set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,
}

impl Default for RestConfig {
    fn default() -> Self {
        Self {
            prefix:          default_rest_prefix(),
            auth:            default_rest_auth(),
            openapi_enabled: false,
            openapi_path:    default_openapi_path(),
            title:           None,
            api_version:     None,
        }
    }
}

fn default_rest_prefix() -> String {
    "/rest".to_string()
}

fn default_rest_auth() -> String {
    "none".to_string()
}

fn default_openapi_path() -> String {
    "/rest/openapi.json".to_string()
}
