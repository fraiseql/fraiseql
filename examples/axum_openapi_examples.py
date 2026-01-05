#!/usr/bin/env python3
"""FraiseQL Axum OpenAPI and Documentation Examples.

Demonstrates various OpenAPI schema generation and documentation UI
configurations for different environments and use cases.
"""

import logging
from fraiseql import create_axum_fraiseql_app, fraise_type
from fraiseql.axum.openapi import OpenAPIConfig

# Enable logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
)
logger = logging.getLogger(__name__)


# ===== GraphQL Types =====


@fraise_type
class User:
    """Example user type."""

    id: str
    name: str
    email: str


# ===== Example 1: Default OpenAPI Configuration =====


def example_default_configuration() -> None:
    """Default OpenAPI configuration."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 1: Default OpenAPI Configuration")
    logger.info("=" * 60)

    config = OpenAPIConfig()

    logger.info(f"OpenAPI Config: {config}")
    logger.info(f"Title: {config.title}")
    logger.info(f"Version: {config.version}")
    logger.info(f"Swagger UI: {config.enable_swagger_ui} at {config.swagger_ui_path}")
    logger.info(f"ReDoc: {config.enable_redoc} at {config.redoc_path}")
    logger.info(f"OpenAPI JSON: {config.openapi_path}")


# ===== Example 2: Custom API Metadata =====


def example_custom_metadata() -> None:
    """OpenAPI with custom API metadata."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 2: Custom API Metadata")
    logger.info("=" * 60)

    config = OpenAPIConfig(
        title="User Management API",
        description="GraphQL API for managing users, posts, and comments",
        version="2.1.0",
    )

    logger.info(f"OpenAPI Config: {config}")
    logger.info(f"Title: {config.title}")
    logger.info(f"Description: {config.description}")
    logger.info(f"Version: {config.version}")


# ===== Example 3: Documentation Disabled =====


def example_documentation_disabled() -> None:
    """Documentation UIs disabled (production security)."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 3: Documentation Disabled")
    logger.info("=" * 60)

    config = OpenAPIConfig(
        enable_swagger_ui=False,
        enable_redoc=False,
    )

    logger.info(f"OpenAPI Config: {config}")
    logger.info(f"Swagger UI: {config.enable_swagger_ui}")
    logger.info(f"ReDoc: {config.enable_redoc}")
    logger.info("⚠️ Documentation not publicly accessible")
    logger.info("✓ Recommended for security-sensitive APIs")


# ===== Example 4: Swagger UI Only =====


def example_swagger_ui_only() -> None:
    """Only Swagger UI enabled."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 4: Swagger UI Only")
    logger.info("=" * 60)

    config = OpenAPIConfig(
        enable_swagger_ui=True,
        enable_redoc=False,
    )

    logger.info(f"OpenAPI Config: {config}")
    logger.info(f"Swagger UI: {config.enable_swagger_ui} at {config.swagger_ui_path}")
    logger.info(f"ReDoc: {config.enable_redoc}")
    logger.info("✓ Interactive testing with Swagger")


# ===== Example 5: ReDoc Only =====


def example_redoc_only() -> None:
    """Only ReDoc enabled."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 5: ReDoc Only (Read-Only Documentation)")
    logger.info("=" * 60)

    config = OpenAPIConfig(
        enable_swagger_ui=False,
        enable_redoc=True,
    )

    logger.info(f"OpenAPI Config: {config}")
    logger.info(f"Swagger UI: {config.enable_swagger_ui}")
    logger.info(f"ReDoc: {config.enable_redoc} at {config.redoc_path}")
    logger.info("✓ Professional documentation without interactive testing")


# ===== Example 6: Custom Paths =====


def example_custom_paths() -> None:
    """OpenAPI with custom documentation paths."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 6: Custom Documentation Paths")
    logger.info("=" * 60)

    config = OpenAPIConfig(
        swagger_ui_path="/api/docs",
        redoc_path="/api/redoc",
        openapi_path="/api/openapi.json",
        graphql_endpoint="/api/graphql",
    )

    logger.info(f"OpenAPI Config: {config}")
    logger.info(f"GraphQL Endpoint: {config.graphql_endpoint}")
    logger.info(f"Swagger UI: {config.swagger_ui_path}")
    logger.info(f"ReDoc: {config.redoc_path}")
    logger.info(f"OpenAPI JSON: {config.openapi_path}")


# ===== Example 7: Multiple Servers =====


def example_multiple_servers() -> None:
    """OpenAPI with multiple server environments."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 7: Multiple Server Environments")
    logger.info("=" * 60)

    servers = [
        {
            "url": "https://api.example.com",
            "description": "Production",
        },
        {
            "url": "https://staging.example.com",
            "description": "Staging",
        },
        {
            "url": "http://localhost:8000",
            "description": "Local Development",
        },
    ]

    config = OpenAPIConfig(
        title="Multi-Environment API",
        servers=servers,
    )

    logger.info(f"OpenAPI Config: {config}")
    logger.info("Servers:")
    for server in servers:
        logger.info(f"  - {server['url']} ({server['description']})")


# ===== Example 8: API Tags =====


def example_api_tags() -> None:
    """OpenAPI with operation tags for organization."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 8: API Operation Tags")
    logger.info("=" * 60)

    tags = [
        {
            "name": "users",
            "description": "User management operations",
        },
        {
            "name": "posts",
            "description": "Post management operations",
        },
        {
            "name": "comments",
            "description": "Comment management operations",
        },
    ]

    config = OpenAPIConfig(
        title="Blog API",
        tags=tags,
    )

    logger.info(f"OpenAPI Config: {config}")
    logger.info("Operation Tags:")
    for tag in tags:
        logger.info(f"  - {tag['name']}: {tag['description']}")


# ===== Example 9: External Documentation =====


def example_external_documentation() -> None:
    """OpenAPI with external documentation reference."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 9: External Documentation Links")
    logger.info("=" * 60)

    external_docs = {
        "url": "https://docs.example.com",
        "description": "Full API documentation and guides",
    }

    config = OpenAPIConfig(
        title="Documented API",
        external_docs=external_docs,
    )

    logger.info(f"OpenAPI Config: {config}")
    logger.info(f"External Docs: {external_docs['url']}")
    logger.info(f"Description: {external_docs['description']}")


# ===== Example 10: Schema Generation =====


def example_schema_generation() -> None:
    """Show OpenAPI schema generation."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 10: OpenAPI Schema Generation")
    logger.info("=" * 60)

    config = OpenAPIConfig(
        title="Generated API",
        description="API with generated schema",
        version="1.0.0",
    )

    schema = config.generate_openapi_schema()

    logger.info("Generated OpenAPI Schema:")
    logger.info(f"  OpenAPI Version: {schema['openapi']}")
    logger.info(f"  API Title: {schema['info']['title']}")
    logger.info(f"  API Version: {schema['info']['version']}")
    logger.info(f"  Paths: {list(schema['paths'].keys())}")


# ===== Example 11: Swagger HTML Generation =====


def example_swagger_html() -> None:
    """Show Swagger UI HTML generation."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 11: Swagger UI HTML Generation")
    logger.info("=" * 60)

    config = OpenAPIConfig(title="Swagger Test API")
    html = config.generate_swagger_html()

    logger.info("Generated Swagger HTML:")
    logger.info(f"  Size: {len(html)} bytes")
    logger.info(f"  Contains Swagger UI: {'swagger-ui' in html}")
    logger.info(f"  Contains OpenAPI reference: {config.openapi_path in html}")
    logger.info(f"  Title in HTML: {config.title in html}")


# ===== Example 12: ReDoc HTML Generation =====


def example_redoc_html() -> None:
    """Show ReDoc HTML generation."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 12: ReDoc HTML Generation")
    logger.info("=" * 60)

    config = OpenAPIConfig(
        title="ReDoc Test API",
        description="API documentation with ReDoc",
    )
    html = config.generate_redoc_html()

    logger.info("Generated ReDoc HTML:")
    logger.info(f"  Size: {len(html)} bytes")
    logger.info(f"  Contains ReDoc: {'redoc' in html}")
    logger.info(f"  Contains OpenAPI reference: {config.openapi_path in html}")
    logger.info(f"  Title in HTML: {config.title in html}")


# ===== Example 13: Development Setup =====


def example_development_setup() -> None:
    """Typical development setup."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 13: Development Setup")
    logger.info("=" * 60)

    config = OpenAPIConfig(
        title="Development GraphQL API",
        description="Local development environment",
        version="0.1.0",
        enable_swagger_ui=True,
        enable_redoc=True,
        servers=[{"url": "http://localhost:8000", "description": "Local"}],
    )

    logger.info(f"OpenAPI Config: {config}")
    logger.info("✓ Full documentation for development")
    logger.info("✓ Local server reference")
    logger.info("✓ Interactive testing enabled")


# ===== Example 14: Production Setup =====


def example_production_setup() -> None:
    """Typical production setup."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 14: Production Setup")
    logger.info("=" * 60)

    config = OpenAPIConfig(
        title="Production GraphQL API",
        description="Official production API",
        version="2.1.0",
        enable_swagger_ui=True,
        enable_redoc=True,
        servers=[
            {"url": "https://api.example.com", "description": "Production"},
            {"url": "https://staging.example.com", "description": "Staging"},
        ],
        tags=[
            {"name": "queries", "description": "GraphQL Queries"},
            {"name": "mutations", "description": "GraphQL Mutations"},
        ],
        external_docs={
            "url": "https://docs.example.com",
            "description": "Complete API documentation",
        },
    )

    logger.info(f"OpenAPI Config: {config}")
    logger.info("✓ Professional documentation")
    logger.info("✓ Multiple server environments")
    logger.info("✓ Tagged operations")
    logger.info("✓ External documentation links")


# ===== Example 15: Full Configuration =====


def example_full_configuration() -> None:
    """Complete configuration with all options."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 15: Full Configuration")
    logger.info("=" * 60)

    config = OpenAPIConfig(
        title="Comprehensive GraphQL API",
        description="Fully configured API with all features",
        version="3.0.0",
        graphql_endpoint="/api/graphql",
        subscriptions_endpoint="/api/subscriptions",
        enable_swagger_ui=True,
        swagger_ui_path="/docs",
        enable_redoc=True,
        redoc_path="/redoc",
        openapi_path="/openapi.json",
        servers=[
            {"url": "https://api.example.com", "description": "Production"},
            {"url": "https://staging.example.com", "description": "Staging"},
            {"url": "http://localhost:8000", "description": "Local"},
        ],
        tags=[
            {"name": "queries", "description": "GraphQL Queries"},
            {"name": "mutations", "description": "GraphQL Mutations"},
            {"name": "subscriptions", "description": "Real-time Subscriptions"},
        ],
        external_docs={
            "url": "https://docs.example.com",
            "description": "Full API documentation and tutorials",
        },
    )

    logger.info(f"OpenAPI Config: {config}")
    logger.info(f"GraphQL Endpoint: {config.graphql_endpoint}")
    logger.info(f"Subscriptions: {config.subscriptions_endpoint}")
    logger.info(f"Documentation UIs: Swagger ({config.swagger_ui_path}) + ReDoc ({config.redoc_path})")
    logger.info(f"OpenAPI JSON: {config.openapi_path}")
    logger.info(f"Servers: {len(config.servers or [])} environments")
    logger.info(f"Tags: {len(config.tags or [])} operation groups")


# ===== Main Entry Point =====


if __name__ == "__main__":
    import sys

    examples = {
        "default": example_default_configuration,
        "custom_metadata": example_custom_metadata,
        "disabled": example_documentation_disabled,
        "swagger_only": example_swagger_ui_only,
        "redoc_only": example_redoc_only,
        "custom_paths": example_custom_paths,
        "multiple_servers": example_multiple_servers,
        "tags": example_api_tags,
        "external_docs": example_external_documentation,
        "schema_generation": example_schema_generation,
        "swagger_html": example_swagger_html,
        "redoc_html": example_redoc_html,
        "development": example_development_setup,
        "production": example_production_setup,
        "full": example_full_configuration,
    }

    if len(sys.argv) > 1:
        example_name = sys.argv[1]
        if example_name in examples:
            examples[example_name]()
        else:
            print(f"Unknown example: {example_name}")
            print(f"Available: {', '.join(examples.keys())}")
            sys.exit(1)
    else:
        # Run all examples
        for name, example in examples.items():
            example()
        print("\n" + "=" * 60)
        print("All OpenAPI examples completed!")
        print("=" * 60)
