"""OpenAPI schema generation and documentation configuration.

Generates OpenAPI 3.0 schemas from GraphQL and provides documentation UIs.
"""

import logging
from dataclasses import dataclass

logger = logging.getLogger(__name__)


@dataclass
class OpenAPIConfig:
    """OpenAPI schema and documentation configuration.

    Controls OpenAPI schema generation and documentation UI availability.

    Attributes:
        title: API title (default: "GraphQL API")
        description: API description (default: "")
        version: API version (default: "1.0.0")
        graphql_endpoint: GraphQL endpoint path (default: "/graphql")
        subscriptions_endpoint: WebSocket subscriptions endpoint (default: None)
        enable_swagger_ui: Enable Swagger UI at /docs (default: True)
        swagger_ui_path: Path to serve Swagger UI (default: "/docs")
        enable_redoc: Enable ReDoc UI at /redoc (default: True)
        redoc_path: Path to serve ReDoc (default: "/redoc")
        openapi_path: Path to serve OpenAPI JSON (default: "/openapi.json")
        servers: List of server URLs (default: None)
        tags: API tags for grouping (default: None)
        external_docs: External documentation URL (default: None)

    Example:
        ```python
        config = OpenAPIConfig(
            title="My GraphQL API",
            description="Production GraphQL API",
            version="2.0.0",
            enable_swagger_ui=True,
            enable_redoc=True
        )
        ```
    """

    title: str = "GraphQL API"
    description: str = ""
    version: str = "1.0.0"
    graphql_endpoint: str = "/graphql"
    subscriptions_endpoint: str | None = None
    enable_swagger_ui: bool = True
    swagger_ui_path: str = "/docs"
    enable_redoc: bool = True
    redoc_path: str = "/redoc"
    openapi_path: str = "/openapi.json"
    servers: list[dict] | None = None
    tags: list[dict] | None = None
    external_docs: dict | None = None

    def __post_init__(self) -> None:
        """Validate configuration after initialization."""
        # Validate paths start with /
        for path, name in [
            (self.swagger_ui_path, "swagger_ui_path"),
            (self.redoc_path, "redoc_path"),
            (self.openapi_path, "openapi_path"),
            (self.graphql_endpoint, "graphql_endpoint"),
        ]:
            if not path.startswith("/"):
                raise ValueError(f"{name} must start with /: {path}")

        if self.subscriptions_endpoint and not self.subscriptions_endpoint.startswith("/"):
            raise ValueError(
                f"subscriptions_endpoint must start with /: {self.subscriptions_endpoint}",
            )

        logger.debug(
            f"OpenAPIConfig initialized: title={self.title!r}, "
            f"version={self.version}, swagger={self.enable_swagger_ui}, "
            f"redoc={self.enable_redoc}",
        )

    def generate_openapi_schema(self) -> dict:
        """Generate OpenAPI 3.0 schema for GraphQL endpoint.

        Creates a basic OpenAPI schema describing the GraphQL endpoint.
        For a complete schema, consider using graphql-core introspection
        and converting it to OpenAPI format.

        Returns:
            OpenAPI 3.0 schema dictionary

        Example:
            ```python
            config = OpenAPIConfig(title="My API", version="1.0.0")
            schema = config.generate_openapi_schema()
            ```
        """
        # Build servers list
        servers = self.servers or [{"url": self.graphql_endpoint}]

        # Build schema dict
        schema: dict = {
            "openapi": "3.0.0",
            "info": {
                "title": self.title,
                "version": self.version,
            },
            "servers": servers,
            "paths": {
                self.graphql_endpoint: {
                    "post": {
                        "summary": "GraphQL Query",
                        "description": "Execute GraphQL queries and mutations",
                        "tags": ["GraphQL"],
                        "requestBody": {
                            "required": True,
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "query": {
                                                "type": "string",
                                                "description": "GraphQL query string",
                                            },
                                            "variables": {
                                                "type": "object",
                                                "description": "Query variables",
                                            },
                                            "operationName": {
                                                "type": "string",
                                                "description": "Operation name",
                                            },
                                        },
                                        "required": ["query"],
                                    },
                                },
                            },
                        },
                        "responses": {
                            "200": {
                                "description": "Successful GraphQL response",
                                "content": {
                                    "application/json": {
                                        "schema": {
                                            "type": "object",
                                            "properties": {
                                                "data": {
                                                    "type": "object",
                                                    "description": "Query result data",
                                                },
                                                "errors": {
                                                    "type": "array",
                                                    "description": "GraphQL errors if any",
                                                    "items": {
                                                        "type": "object",
                                                        "properties": {
                                                            "message": {"type": "string"},
                                                            "locations": {"type": "array"},
                                                            "path": {"type": "array"},
                                                        },
                                                    },
                                                },
                                            },
                                        },
                                    },
                                },
                            },
                            "400": {
                                "description": "Bad request (invalid GraphQL)",
                            },
                            "500": {
                                "description": "Internal server error",
                            },
                        },
                    },
                },
            },
        }

        # Add description if provided
        if self.description:
            schema["info"]["description"] = self.description

        # Add tags if provided
        if self.tags:
            schema["tags"] = self.tags

        # Add external docs if provided
        if self.external_docs:
            schema["externalDocs"] = self.external_docs

        # Add subscriptions endpoint if provided
        if self.subscriptions_endpoint:
            schema["paths"][self.subscriptions_endpoint] = {
                "get": {
                    "summary": "GraphQL Subscriptions",
                    "description": "WebSocket endpoint for subscriptions",
                    "tags": ["GraphQL"],
                },
            }

        return schema

    def generate_swagger_html(self) -> str:
        """Generate HTML for Swagger UI.

        Creates a complete HTML page with Swagger UI embedded.

        Returns:
            Complete HTML page as string

        Example:
            ```python
            config = OpenAPIConfig()
            html = config.generate_swagger_html()
            ```
        """
        openapi_url = self._escape_html(self.openapi_path)
        title = self._escape_html(self.title)

        html = f"""<!DOCTYPE html>
<html>
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>{title} - Swagger UI</title>
    <link
      rel="stylesheet"
      href="https://cdnjs.cloudflare.com/ajax/libs/swagger-ui/5.10.3/swagger-ui.min.css"
    />
    <link
      rel="icon"
      type="image/png"
      href="https://fastapi.tiangolo.com/img/favicon.png"
    />
    <style>
      html {{
        box-sizing: border-box;
        overflow: -moz-scrollbars-vertical;
        overflow-y: scroll;
      }}
      *, *:before, *:after {{
        box-sizing: inherit;
      }}
      body {{
        margin: 0;
        padding: 0;
        background: #fafafa;
      }}
    </style>
  </head>
  <body>
    <div id="swagger-ui"></div>
    <script src="https://cdnjs.cloudflare.com/ajax/libs/swagger-ui/5.10.3/swagger-ui.min.js"></script>
    <script src="https://cdnjs.cloudflare.com/ajax/libs/swagger-ui/5.10.3/swagger-ui-bundle.min.js"></script>
    <script>
      const ui = SwaggerUIBundle({{
        url: "{openapi_url}",
        dom_id: "#swagger-ui",
        deepLinking: true,
        presets: [
          SwaggerUIBundle.presets.apis,
          SwaggerUIBundle.SwaggerUIStandalonePreset,
        ],
        plugins: [
          SwaggerUIBundle.plugins.DownloadUrl,
        ],
        layout: "BaseLayout",
      }})
      window.ui = ui
    </script>
  </body>
</html>"""

        return html

    def generate_redoc_html(self) -> str:
        """Generate HTML for ReDoc UI.

        Creates a complete HTML page with ReDoc embedded.

        Returns:
            Complete HTML page as string

        Example:
            ```python
            config = OpenAPIConfig()
            html = config.generate_redoc_html()
            ```
        """
        openapi_url = self._escape_html(self.openapi_path)
        title = self._escape_html(self.title)

        html = f"""<!DOCTYPE html>
<html>
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>{title} - ReDoc</title>
    <meta name="description" content="{self._escape_html(self.description or "")}" />
    <style>
      body {{
        margin: 0;
        padding: 0;
      }}
    </style>
  </head>
  <body>
    <redoc spec-url="{openapi_url}"></redoc>
    <script src="https://cdn.jsdelivr.net/npm/redoc@next/bundles/redoc.standalone.js"></script>
  </body>
</html>"""

        return html

    def to_dict(self) -> dict:
        """Convert to dictionary for serialization.

        Returns:
            Configuration as dictionary
        """
        return {
            "title": self.title,
            "description": self.description,
            "version": self.version,
            "graphql_endpoint": self.graphql_endpoint,
            "subscriptions_endpoint": self.subscriptions_endpoint,
            "enable_swagger_ui": self.enable_swagger_ui,
            "swagger_ui_path": self.swagger_ui_path,
            "enable_redoc": self.enable_redoc,
            "redoc_path": self.redoc_path,
            "openapi_path": self.openapi_path,
            "servers": self.servers or [],
            "tags": self.tags or [],
            "external_docs": self.external_docs,
        }

    @staticmethod
    def _escape_html(text: str) -> str:
        """Escape HTML special characters.

        Args:
            text: Text to escape

        Returns:
            HTML-escaped text
        """
        return (
            text.replace("&", "&amp;")
            .replace("<", "&lt;")
            .replace(">", "&gt;")
            .replace('"', "&quot;")
            .replace("'", "&#39;")
        )

    def __repr__(self) -> str:
        """String representation."""
        return (
            f"OpenAPIConfig(title={self.title!r}, version={self.version}, "
            f"swagger={self.enable_swagger_ui}, redoc={self.enable_redoc})"
        )

    def __str__(self) -> str:
        """User-friendly string."""
        docs = []
        if self.enable_swagger_ui:
            docs.append(f"Swagger at {self.swagger_ui_path}")
        if self.enable_redoc:
            docs.append(f"ReDoc at {self.redoc_path}")

        docs_str = ", ".join(docs) if docs else "No docs enabled"
        return f"OpenAPI Documentation [{docs_str}]"
