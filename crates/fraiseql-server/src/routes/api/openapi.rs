//! `OpenAPI` specification for FraiseQL REST APIs.
//!
//! Provides a static `OpenAPI` 3.0.0 specification documenting all API endpoints,
//! request/response schemas, and authentication requirements.

/// The per-cache result the admin cache endpoints report (#941).
///
/// Built outside the document's own `json!` because that macro expands recursively
/// once per nesting level and the whole spec already sits near the crate's recursion
/// limit; a nested literal here tips it over.
fn cache_operation_result_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "cache": {
                "type": "string",
                "enum": ["query_result", "arrow_flight"],
                "description": "query_result serves GraphQL; arrow_flight caches Flight query plans"
            },
            "configured": {
                "type": "boolean",
                "description": "Whether this cache exists on this server"
            },
            "entries_cleared": {
                "type": "integer",
                "nullable": true,
                "description": "Entries dropped, or absent when the scope does not apply to this cache"
            },
            "note": {
                "type": "string",
                "nullable": true,
                "description": "Why nothing happened, when nothing happened"
            }
        }
    })
}

/// The admin cache-clear response (#941), for the same recursion-depth reason.
fn cache_clear_response_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "success": { "type": "boolean" },
            "entries_cleared": {
                "type": "integer",
                "description": "Total entries dropped, summed across every cache that served the scope"
            },
            "caches": {
                "type": "array",
                "description": "Per-cache outcome: the server can hold a query result cache and an Arrow Flight cache, and one request may apply to only one of them",
                "items": { "$ref": "#/components/schemas/CacheOperationResult" }
            },
            "message": { "type": "string" }
        }
    })
}

/// The `components` section of the spec.
///
/// Split out of [`get_openapi_spec`]'s literal because `serde_json::json!` recurses
/// once per entry while parsing an object, and the schema map had reached the crate's
/// recursion limit — adding one key (#941's `CacheOperationResult`) stopped the crate
/// compiling. A separate call starts the expansion budget over.
fn components_schema() -> serde_json::Value {
    serde_json::json!({
            "securitySchemes": {
                "BearerAuth": {
                    "type": "http",
                    "scheme": "bearer",
                    "description": "Bearer token for admin endpoints"
                }
            },
            "schemas": {
                "ExplainRequest": {
                    "type": "object",
                    "required": ["query"],
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "GraphQL query to analyze",
                            "example": "query { users { id name } }"
                        }
                    }
                },
                "ComplexityInfo": {
                    "type": "object",
                    "properties": {
                        "depth": {
                            "type": "integer",
                            "description": "Query nesting depth",
                            "example": 2
                        },
                        "field_count": {
                            "type": "integer",
                            "description": "Total fields requested",
                            "example": 10
                        },
                        "score": {
                            "type": "integer",
                            "description": "Complexity score (depth × field_count)",
                            "example": 45
                        }
                    }
                },
                "ExplainResponse": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string"
                        },
                        "sql": {
                            "type": "string",
                            "nullable": true,
                            "description": "Generated SQL execution plan"
                        },
                        "estimated_cost": {
                            "type": "integer"
                        },
                        "complexity": {
                            "$ref": "#/components/schemas/ComplexityInfo"
                        },
                        "warnings": {
                            "type": "array",
                            "items": {
                                "type": "string"
                            }
                        }
                    }
                },
                "ValidateRequest": {
                    "type": "object",
                    "required": ["query"],
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "GraphQL query to validate"
                        }
                    }
                },
                "ValidateResponse": {
                    "type": "object",
                    "properties": {
                        "valid": {
                            "type": "boolean"
                        },
                        "errors": {
                            "type": "array",
                            "items": {
                                "type": "string"
                            }
                        }
                    }
                },
                "StatsResponse": {
                    "type": "object",
                    "properties": {
                        "query_count": {
                            "type": "integer"
                        },
                        "avg_latency_ms": {
                            "type": "number"
                        }
                    }
                },
                "SubgraphInfo": {
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "example": "users"
                        },
                        "url": {
                            "type": "string",
                            "example": "http://users.local/graphql"
                        },
                        "entities": {
                            "type": "array",
                            "items": {
                                "type": "string"
                            }
                        },
                        "healthy": {
                            "type": "boolean"
                        }
                    }
                },
                "SubgraphsResponse": {
                    "type": "object",
                    "properties": {
                        "subgraphs": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/SubgraphInfo"
                            }
                        }
                    }
                },
                "GraphResponse": {
                    "type": "object",
                    "properties": {
                        "format": {
                            "type": "string",
                            "enum": ["json", "dot", "mermaid"]
                        },
                        "content": {
                            "type": "string",
                            "description": "Graph in requested format"
                        }
                    }
                },
                "JsonSchemaResponse": {
                    "type": "object",
                    "properties": {
                        "schema": {
                            "type": "object",
                            "description": "Compiled schema as JSON"
                        }
                    }
                },
                "ReloadSchemaRequest": {
                    "type": "object",
                    "required": ["schema_path"],
                    "properties": {
                        "schema_path": {
                            "type": "string",
                            "description": "Path to compiled schema file",
                            "example": "/path/to/schema.compiled.json"
                        },
                        "validate_only": {
                            "type": "boolean",
                            "description": "If true, only validate without applying",
                            "default": false
                        }
                    }
                },
                "ReloadSchemaResponse": {
                    "type": "object",
                    "properties": {
                        "success": {
                            "type": "boolean"
                        },
                        "message": {
                            "type": "string"
                        }
                    }
                },
                "CacheClearRequest": {
                    "type": "object",
                    "required": ["scope"],
                    "properties": {
                        "scope": {
                            "type": "string",
                            "enum": ["all", "entity", "pattern"],
                            "description": "Scope for cache clearing"
                        },
                        "entity_type": {
                            "type": "string",
                            "nullable": true,
                            "description": "Required if scope is 'entity'"
                        },
                        "pattern": {
                            "type": "string",
                            "nullable": true,
                            "description": "Required if scope is 'pattern'"
                        }
                    }
                },
                "CacheClearResponse": cache_clear_response_schema(),
                "CacheOperationResult": cache_operation_result_schema(),
                "AdminConfigResponse": {
                    "type": "object",
                    "properties": {
                        "version": {
                            "type": "string",
                            "example": "2.0.0-a1"
                        },
                        "config": {
                            "type": "object",
                            "description": "Sanitized configuration (no secrets)",
                            "additionalProperties": {
                                "type": "string"
                            }
                        }
                    }
                },
                "ApiResponse": {
                    "type": "object",
                    "properties": {
                        "status": {
                            "type": "string",
                            "example": "success"
                        },
                        "data": {
                            "type": "object"
                        }
                    }
                },
                "ApiResponseExplain": {
                    "allOf": [
                        {
                            "$ref": "#/components/schemas/ApiResponse"
                        },
                        {
                            "type": "object",
                            "properties": {
                                "data": {
                                    "$ref": "#/components/schemas/ExplainResponse"
                                }
                            }
                        }
                    ]
                },
                "ApiResponseValidate": {
                    "allOf": [
                        {
                            "$ref": "#/components/schemas/ApiResponse"
                        },
                        {
                            "type": "object",
                            "properties": {
                                "data": {
                                    "$ref": "#/components/schemas/ValidateResponse"
                                }
                            }
                        }
                    ]
                },
                "ApiResponseStats": {
                    "allOf": [
                        {
                            "$ref": "#/components/schemas/ApiResponse"
                        },
                        {
                            "type": "object",
                            "properties": {
                                "data": {
                                    "$ref": "#/components/schemas/StatsResponse"
                                }
                            }
                        }
                    ]
                },
                "ApiResponseSubgraphs": {
                    "allOf": [
                        {
                            "$ref": "#/components/schemas/ApiResponse"
                        },
                        {
                            "type": "object",
                            "properties": {
                                "data": {
                                    "$ref": "#/components/schemas/SubgraphsResponse"
                                }
                            }
                        }
                    ]
                },
                "ApiResponseGraph": {
                    "allOf": [
                        {
                            "$ref": "#/components/schemas/ApiResponse"
                        },
                        {
                            "type": "object",
                            "properties": {
                                "data": {
                                    "$ref": "#/components/schemas/GraphResponse"
                                }
                            }
                        }
                    ]
                },
                "ApiResponseSchemaJson": {
                    "allOf": [
                        {
                            "$ref": "#/components/schemas/ApiResponse"
                        },
                        {
                            "type": "object",
                            "properties": {
                                "data": {
                                    "$ref": "#/components/schemas/JsonSchemaResponse"
                                }
                            }
                        }
                    ]
                },
                "ApiResponseReloadSchema": {
                    "allOf": [
                        {
                            "$ref": "#/components/schemas/ApiResponse"
                        },
                        {
                            "type": "object",
                            "properties": {
                                "data": {
                                    "$ref": "#/components/schemas/ReloadSchemaResponse"
                                }
                            }
                        }
                    ]
                },
                "ApiResponseCacheClear": {
                    "allOf": [
                        {
                            "$ref": "#/components/schemas/ApiResponse"
                        },
                        {
                            "type": "object",
                            "properties": {
                                "data": {
                                    "$ref": "#/components/schemas/CacheClearResponse"
                                }
                            }
                        }
                    ]
                },
                "ApiResponseConfig": {
                    "allOf": [
                        {
                            "$ref": "#/components/schemas/ApiResponse"
                        },
                        {
                            "type": "object",
                            "properties": {
                                "data": {
                                    "$ref": "#/components/schemas/AdminConfigResponse"
                                }
                            }
                        }
                    ]
                }
            }
    })
}

/// Get complete `OpenAPI` 3.0.0 specification as JSON string.
#[must_use]
pub fn get_openapi_spec() -> String {
    serde_json::json!({
        "openapi": "3.0.0",
        "info": {
            "title": "FraiseQL Agent APIs",
            "description": "GraphQL query intelligence, federation discovery, and administration APIs for FraiseQL",
            "version": "1.0.0",
            "contact": {
                "name": "FraiseQL Support",
                "url": "https://github.com/fraiseql/fraiseql"
            },
            "license": {
                "name": "MIT OR Apache-2.0"
            }
        },
        "servers": [
            {
                "url": "http://localhost:8080",
                "description": "Local development server"
            },
            {
                "url": "https://api.fraiseql.example.com",
                "description": "Production server"
            }
        ],
        "paths": {
            "/api/v1/query/explain": {
                "post": {
                    "summary": "Analyze GraphQL query complexity",
                    "description": "Analyzes a GraphQL query for depth, field count, and estimated execution cost. Returns complexity metrics and optimization recommendations.",
                    "tags": ["Query Intelligence"],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/ExplainRequest"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Query analysis successful",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiResponseExplain"
                                    }
                                }
                            }
                        },
                        "400": {
                            "description": "Invalid query or validation error"
                        }
                    }
                }
            },
            "/api/v1/query/validate": {
                "post": {
                    "summary": "Validate GraphQL query syntax",
                    "description": "Validates GraphQL query syntax without executing analysis. Fast validation for batch operations.",
                    "tags": ["Query Intelligence"],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/ValidateRequest"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Query validation result",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiResponseValidate"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/api/v1/query/stats": {
                "get": {
                    "summary": "Get query performance statistics",
                    "description": "Retrieves historical performance metrics for queries. Requires metrics collection to be enabled.",
                    "tags": ["Query Intelligence"],
                    "responses": {
                        "200": {
                            "description": "Query statistics",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiResponseStats"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/api/v1/federation/subgraphs": {
                "get": {
                    "summary": "List federation subgraphs",
                    "description": "Returns all federated subgraphs with their URLs, managed entities, and health status.",
                    "tags": ["Federation"],
                    "responses": {
                        "200": {
                            "description": "List of subgraphs",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiResponseSubgraphs"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/api/v1/federation/graph": {
                "get": {
                    "summary": "Export federation dependency graph",
                    "description": "Exports the federation structure showing subgraph relationships and entity resolution paths. Supports multiple output formats.",
                    "tags": ["Federation"],
                    "parameters": [
                        {
                            "name": "format",
                            "in": "query",
                            "description": "Output format: json (default), dot (Graphviz), or mermaid",
                            "schema": {
                                "type": "string",
                                "enum": ["json", "dot", "mermaid"],
                                "default": "json"
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Federation graph in requested format",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiResponseGraph"
                                    }
                                }
                            }
                        },
                        "400": {
                            "description": "Invalid format parameter"
                        }
                    }
                }
            },
            "/api/v1/schema.graphql": {
                "get": {
                    "summary": "Export schema as GraphQL SDL",
                    "description": "Exports the compiled schema in GraphQL Schema Definition Language (SDL) format. Returns text/plain response.",
                    "tags": ["Schema"],
                    "responses": {
                        "200": {
                            "description": "Schema in SDL format",
                            "content": {
                                "text/plain": {
                                    "schema": {
                                        "type": "string",
                                        "example": "type Query { users: [User!]! }\ntype User { id: ID! name: String! }"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/api/v1/schema.json": {
                "get": {
                    "summary": "Export schema as JSON",
                    "description": "Exports the full compiled schema in JSON format with type information and metadata.",
                    "tags": ["Schema"],
                    "responses": {
                        "200": {
                            "description": "Schema as JSON",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiResponseSchemaJson"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/api/v1/admin/reload-schema": {
                "post": {
                    "summary": "Hot reload schema",
                    "description": "Reload schema from file without restarting the server. Supports validation-only mode.",
                    "tags": ["Admin"],
                    "security": [
                        {
                            "BearerAuth": []
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/ReloadSchemaRequest"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Schema reload result",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiResponseReloadSchema"
                                    }
                                }
                            }
                        },
                        "401": {
                            "description": "Unauthorized - admin token required"
                        },
                        "400": {
                            "description": "Invalid schema or validation error"
                        }
                    }
                }
            },
            "/api/v1/admin/cache/clear": {
                "post": {
                    "summary": "Clear cache entries",
                    "description": "Invalidate cache by scope: all (clear everything), entity (by type), or pattern (by glob).",
                    "tags": ["Admin"],
                    "security": [
                        {
                            "BearerAuth": []
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/CacheClearRequest"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Cache clear result",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiResponseCacheClear"
                                    }
                                }
                            }
                        },
                        "401": {
                            "description": "Unauthorized - admin token required"
                        }
                    }
                }
            },
            "/api/v1/admin/config": {
                "get": {
                    "summary": "Get runtime configuration",
                    "description": "Returns sanitized runtime configuration (secrets excluded). Requires admin token.",
                    "tags": ["Admin"],
                    "security": [
                        {
                            "BearerAuth": []
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Runtime configuration",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiResponseConfig"
                                    }
                                }
                            }
                        },
                        "401": {
                            "description": "Unauthorized - admin token required"
                        }
                    }
                }
            },
            "/api/v1/admin/storage/{bucket}/policies": storage_policies_path()
        },
        "components": components_schema()
    }).to_string()
}

/// The `/api/v1/admin/storage/{bucket}/policies` path item (#974).
///
/// Split out of `openapi_spec` because the whole spec is one `json!` literal
/// and the nested request-body schema here pushes it past the macro recursion
/// limit — the same reason `components_schema` is its own function.
fn storage_policies_path() -> serde_json::Value {
    serde_json::json!({
        "parameters": [
            {
                "name": "bucket",
                "in": "path",
                "required": true,
                "schema": { "type": "string" },
                "description": "The configured logical bucket name."
            }
        ],
        "get": {
            "summary": "Read the access policy governing a bucket",
            "description": "Reports the rules in force and which source they came from: \
                             `store` (pushed over this API), `config_file` \
                             (`[[storage.<name>.policies]]`), or `access_mode` (no policy; \
                             the coarse private/public_read mode governs). Requires the \
                             read-only admin token when one is configured.",
            "tags": ["Admin"],
            "security": [{ "BearerAuth": [] }],
            "responses": {
                "200": { "description": "The policy in force, and its source" },
                "401": { "description": "Unauthorized - no admin token presented" },
                "403": { "description": "Forbidden - the presented token is not valid" },
                "404": { "description": "No such bucket is configured" }
            }
        },
        "put": {
            "summary": "Replace a bucket's access policy",
            "description": "Stores the rule list durably and applies it to the next \
                            request. The replacement is WHOLESALE: the rules given here \
                            replace whatever governs the bucket, and are never merged with \
                            the configured policy. A policy this server would not accept \
                            at boot is refused with 400 naming the offending rule, and the \
                            policy already in force keeps serving unchanged. An empty \
                            `rules` list is a valid lock-down that permits nothing; to \
                            hand the bucket back to its configured policy, DELETE. \
                            Requires the write admin token.",
            "tags": ["Admin"],
            "security": [{ "BearerAuth": [] }],
            "requestBody": {
                "required": true,
                "content": {
                    "application/json": {
                        "schema": {
                            "type": "object",
                            "required": ["rules"],
                            "additionalProperties": false,
                            "properties": {
                                "rules": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "required": ["methods", "principal"],
                                        "additionalProperties": false,
                                        "properties": {
                                            "methods": {
                                                "type": "array",
                                                "items": {
                                                    "type": "string",
                                                    "enum": ["read", "write", "overwrite",
                                                             "delete", "list"]
                                                }
                                            },
                                            "principal": {
                                                "type": "string",
                                                "description": "owner | authenticated | \
                                                                anonymous | signed_url | \
                                                                role:<name>"
                                            },
                                            "key_prefix": { "type": "string" },
                                            "not_before": {
                                                "type": "string", "format": "date-time"
                                            },
                                            "not_after": {
                                                "type": "string", "format": "date-time"
                                            },
                                            "require_unexpired": { "type": "boolean" },
                                            "require_claims": {
                                                "type": "object",
                                                "additionalProperties": { "type": "string" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "responses": {
                "200": { "description": "The policy now in force, and its source" },
                "400": {
                    "description": "Invalid policy - nothing was stored and nothing was \
                                    applied; the response carries `rule_index` and \
                                    `policy_in_force: unchanged`"
                },
                "401": { "description": "Unauthorized - no admin token presented" },
                "403": { "description": "Forbidden - the read-only token cannot write" },
                "404": { "description": "No such bucket is configured" }
            }
        },
        "delete": {
            "summary": "Drop a bucket's stored policy",
            "description": "Hands the bucket back to its configured policy, or — with none \
                            configured — to its coarse access mode. This can WIDEN access, \
                            which is why it needs the write admin token; the response \
                            states the source that now governs.",
            "tags": ["Admin"],
            "security": [{ "BearerAuth": [] }],
            "responses": {
                "200": { "description": "The policy now in force, and its source" },
                "401": { "description": "Unauthorized - no admin token presented" },
                "403": { "description": "Forbidden - the read-only token cannot delete" },
                "404": { "description": "No such bucket is configured" }
            }
        }
    })
}
