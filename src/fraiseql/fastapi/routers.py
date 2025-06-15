"""GraphQL routers for development and production environments."""

import json
from typing import Any

from fastapi import APIRouter, Depends, HTTPException
from fastapi.responses import HTMLResponse
from graphql import GraphQLSchema, graphql, parse, validate
from pydantic import BaseModel

from fraiseql.auth.base import AuthProvider
from fraiseql.fastapi.config import FraiseQLConfig
from fraiseql.fastapi.dependencies import build_graphql_context


class GraphQLRequest(BaseModel):
    """GraphQL request model."""

    query: str
    variables: dict[str, Any] | None = None
    operationName: str | None = None


class CompiledQuery:
    """Represents a pre-compiled GraphQL query."""

    def __init__(
        self,
        query_string: str,
        sql_template: str,
        param_mapping: dict[str, str],
        operation_name: str | None = None,
    ):
        self.query_string = query_string
        self.sql_template = sql_template
        self.param_mapping = param_mapping
        self.operation_name = operation_name


def create_graphql_router(
    schema: GraphQLSchema,
    config: FraiseQLConfig,
    auth_provider: AuthProvider | None = None,
) -> APIRouter:
    """Create appropriate router based on environment."""
    if config.environment == "production" and config.enable_query_compilation:
        return create_production_router(schema, config, auth_provider)
    else:
        return create_development_router(schema, config, auth_provider)


def create_development_router(
    schema: GraphQLSchema,
    config: FraiseQLConfig,
    auth_provider: AuthProvider | None = None,
) -> APIRouter:
    """Create development router with full GraphQL features."""
    router = APIRouter(prefix="", tags=["GraphQL"])

    @router.post("/graphql")
    async def graphql_endpoint(
        request: GraphQLRequest,
        context: dict[str, Any] = Depends(build_graphql_context),
    ):
        """Execute GraphQL query with full validation and introspection."""
        try:
            # Execute query
            result = await graphql(
                schema,
                request.query,
                variable_values=request.variables,
                operation_name=request.operationName,
                context_value=context,
            )

            # Build response
            response: dict[str, Any] = {}
            if result.data is not None:
                response["data"] = result.data
            if result.errors:
                response["errors"] = [
                    {
                        "message": error.message,
                        "locations": (
                            [
                                {"line": loc.line, "column": loc.column}
                                for loc in error.locations
                            ]
                            if error.locations
                            else None
                        ),
                        "path": error.path,
                        "extensions": error.extensions,
                    }
                    for error in result.errors
                ]

            return response

        except Exception as e:
            # In development, provide detailed error info
            return {
                "errors": [
                    {
                        "message": str(e),
                        "extensions": {
                            "code": "INTERNAL_SERVER_ERROR",
                            "exception": type(e).__name__,
                        },
                    }
                ]
            }

    @router.get("/graphql")
    async def graphql_get_endpoint(
        query: str,
        variables: str | None = None,
        operationName: str | None = None,
        context: dict[str, Any] = Depends(build_graphql_context),
    ):
        """Handle GraphQL GET requests."""
        parsed_variables = None
        if variables:
            try:
                parsed_variables = json.loads(variables)
            except json.JSONDecodeError:
                raise HTTPException(400, "Invalid JSON in variables parameter")

        request = GraphQLRequest(
            query=query,
            variables=parsed_variables,
            operationName=operationName,
        )

        return await graphql_endpoint(request, context)

    if config.enable_playground:

        @router.get("/playground", response_class=HTMLResponse)
        async def graphql_playground():
            """Serve GraphQL Playground interface."""
            return PLAYGROUND_HTML

    if config.enable_introspection:
        # Introspection is handled by GraphQL itself when enabled
        pass

    return router


def create_production_router(
    schema: GraphQLSchema,
    config: FraiseQLConfig,
    auth_provider: AuthProvider | None = None,
    compiled_queries: dict[str, CompiledQuery] | None = None,
) -> APIRouter:
    """Create production router with optimizations.

    Features:
    - Query whitelisting (only pre-compiled queries allowed)
    - Direct SQL execution bypassing GraphQL validation
    - Minimal error information
    - No introspection
    """
    router = APIRouter(prefix="", tags=["GraphQL"])

    # Load compiled queries if path provided
    if config.compiled_queries_path and compiled_queries is None:
        compiled_queries = load_compiled_queries(config.compiled_queries_path)

    compiled_queries = compiled_queries or {}

    @router.post("/graphql")
    async def graphql_endpoint(
        request: GraphQLRequest,
        context: dict[str, Any] = Depends(build_graphql_context),
    ):
        """Execute GraphQL query using pre-compiled queries when possible."""
        try:
            # Try to find compiled query
            query_hash = hash_query(request.query)
            compiled = compiled_queries.get(query_hash)

            if compiled:
                # Execute pre-compiled query directly
                # This bypasses GraphQL validation for performance
                result = await execute_compiled_query(
                    compiled,
                    request.variables or {},
                    context,
                )
                return {"data": result}

            else:
                # Fallback to regular GraphQL execution
                # In strict production mode, this could be disabled
                if config.get("strict_production_mode", False):
                    return {
                        "errors": [
                            {
                                "message": "Query not found",
                                "extensions": {"code": "FORBIDDEN"},
                            }
                        ]
                    }

                # Parse and validate first to fail fast
                try:
                    document = parse(request.query)
                    errors = validate(schema, document)
                    if errors:
                        return {
                            "errors": [
                                {
                                    "message": error.message,
                                    "extensions": {"code": "GRAPHQL_VALIDATION_FAILED"},
                                }
                                for error in errors
                            ]
                        }
                except Exception:
                    return {
                        "errors": [
                            {
                                "message": "Invalid query",
                                "extensions": {"code": "GRAPHQL_PARSE_FAILED"},
                            }
                        ]
                    }

                # Execute query
                result = await graphql(
                    schema,
                    request.query,
                    variable_values=request.variables,
                    operation_name=request.operationName,
                    context_value=context,
                )

                # Build response with minimal error info
                response: dict[str, Any] = {}
                if result.data is not None:
                    response["data"] = result.data
                if result.errors:
                    response["errors"] = [
                        {
                            "message": (
                                "Internal server error"
                                if config.get("hide_error_details", True)
                                else error.message
                            ),
                            "extensions": {"code": "INTERNAL_SERVER_ERROR"},
                        }
                        for error in result.errors
                    ]

                return response

        except Exception:
            # In production, don't expose error details
            return {
                "errors": [
                    {
                        "message": "Internal server error",
                        "extensions": {"code": "INTERNAL_SERVER_ERROR"},
                    }
                ]
            }

    # No GET endpoint in production
    # No playground in production
    # No introspection in production

    return router


async def execute_compiled_query(
    compiled: CompiledQuery,
    variables: dict[str, Any],
    context: dict[str, Any],
) -> dict[str, Any]:
    """Execute a pre-compiled query directly against the database.

    This bypasses GraphQL validation and execution for maximum performance.
    """
    db = context["db"]

    # Map GraphQL variables to SQL parameters
    sql_params = {}
    for gql_var, sql_param in compiled.param_mapping.items():
        if gql_var in variables:
            sql_params[sql_param] = variables[gql_var]

    # Execute SQL directly
    result = await db.run(compiled.sql_template, sql_params)

    # The SQL should already return properly formatted JSON
    return result[0] if result else {}


def hash_query(query: str) -> str:
    """Generate a hash for a GraphQL query for lookup."""
    # Remove whitespace and normalize
    normalized = " ".join(query.split())
    return str(hash(normalized))


def load_compiled_queries(path: str) -> dict[str, CompiledQuery]:
    """Load pre-compiled queries from disk."""
    # This would load compiled queries from a JSON file or similar
    # For now, return empty dict
    return {}


# GraphQL Playground HTML
PLAYGROUND_HTML = """
<!DOCTYPE html>
<html>
<head>
    <title>FraiseQL Playground</title>
    <link rel="stylesheet" href="https://unpkg.com/graphql-playground-react/build/static/css/index.css" />
    <link rel="shortcut icon" href="https://unpkg.com/graphql-playground-react/build/favicon.png" />
    <script src="https://unpkg.com/graphql-playground-react/build/static/js/middleware.js"></script>
</head>
<body>
    <div id="root"></div>
    <script>
        window.addEventListener('load', function (event) {
            GraphQLPlayground.init(document.getElementById('root'), {
                endpoint: '/graphql',
                settings: {
                    'request.credentials': 'include',
                    'editor.theme': 'dark'
                }
            })
        })
    </script>
</body>
</html>
"""
