#!/usr/bin/env python3
"""FraiseQL Axum GraphQL Playground Examples.

Demonstrates various GraphQL Playground configurations for different environments
and use cases.
"""

import logging
from fraiseql import create_axum_fraiseql_app, fraise_type
from fraiseql.axum.playground import PlaygroundConfig

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


# ===== Example 1: Default Playground =====


def example_default_playground() -> None:
    """Default GraphQL Playground configuration."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 1: Default Playground")
    logger.info("=" * 60)

    config = PlaygroundConfig()

    logger.info(f"Playground Config: {config}")
    logger.info(f"Enabled: {config.enabled}")
    logger.info(f"Path: {config.path}")
    logger.info(f"Title: {config.title}")
    logger.info(f"Subscriptions: {config.subscriptions_endpoint}")

    # Generate HTML to show it works
    html = config.generate_html()
    logger.info(f"HTML Length: {len(html)} bytes")
    logger.info("✓ Ready to serve at /playground")


# ===== Example 2: Custom Path =====


def example_custom_path() -> None:
    """Playground at custom URL path."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 2: Custom Path")
    logger.info("=" * 60)

    config = PlaygroundConfig(path="/gql", title="Query Explorer")

    logger.info(f"Playground Config: {config}")
    logger.info(f"Served at: {config.path}")
    logger.info(f"Title: {config.title}")


# ===== Example 3: Playground Disabled =====


def example_disabled_playground() -> None:
    """Playground disabled for production."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 3: Playground Disabled")
    logger.info("=" * 60)

    config = PlaygroundConfig(enabled=False)

    logger.info(f"Playground Config: {config}")
    logger.info(f"Enabled: {config.enabled}")
    logger.info("⚠️ Users cannot access GraphQL IDE")
    logger.info("✓ Recommended for production")


# ===== Example 4: Dark Theme =====


def example_dark_theme() -> None:
    """Playground with dark theme."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 4: Dark Theme")
    logger.info("=" * 60)

    config = PlaygroundConfig(
        title="GraphQL Playground",
        settings={
            "editor.theme": "dark",
            "editor.fontSize": 14,
        },
    )

    logger.info(f"Playground Config: {config}")
    logger.info("Settings:")
    logger.info("  - editor.theme: dark")
    logger.info("  - editor.fontSize: 14")


# ===== Example 5: Custom Endpoint =====


def example_custom_endpoint() -> None:
    """Playground with custom GraphQL endpoint."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 5: Custom GraphQL Endpoint")
    logger.info("=" * 60)

    config = PlaygroundConfig(title="API Explorer")

    logger.info(f"Playground Config: {config}")
    logger.info("HTML generated for custom endpoint:")

    html = config.generate_html(graphql_endpoint="/api/v1/graphql")

    logger.info("  endpoint: '/api/v1/graphql'")
    logger.info(f"  HTML contains endpoint: {'/api/v1/graphql' in html}")


# ===== Example 6: Subscriptions Disabled =====


def example_no_subscriptions() -> None:
    """Playground without WebSocket subscriptions."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 6: Subscriptions Disabled")
    logger.info("=" * 60)

    config = PlaygroundConfig(subscriptions_endpoint=None)

    logger.info(f"Playground Config: {config}")
    logger.info(f"Subscriptions Endpoint: {config.subscriptions_endpoint}")
    logger.info("✓ WebSocket subscriptions not available in playground")


# ===== Example 7: WebSocket at Custom Path =====


def example_custom_subscription_endpoint() -> None:
    """Playground with custom WebSocket endpoint."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 7: Custom WebSocket Endpoint")
    logger.info("=" * 60)

    config = PlaygroundConfig(
        subscriptions_endpoint="/graphql/ws",
        title="Advanced API",
    )

    logger.info(f"Playground Config: {config}")
    logger.info(f"REST Endpoint: /graphql")
    logger.info(f"WebSocket Endpoint: {config.subscriptions_endpoint}")


# ===== Example 8: Full Configuration =====


def example_full_configuration() -> None:
    """Complete playground configuration with all options."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 8: Full Configuration")
    logger.info("=" * 60)

    config = PlaygroundConfig(
        enabled=True,
        path="/explore",
        title="FraiseQL GraphQL Explorer",
        subscriptions_endpoint="/graphql/subscriptions",
        settings={
            "editor.theme": "light",
            "editor.fontSize": 13,
            "editor.reuseHeaders": True,
            "general.betaUpdates": False,
            "prettier.printWidth": 120,
            "schema.disableComments": False,
            "tracing.hideTracingResponse": False,
        },
    )

    logger.info(f"Playground Config: {config}")
    logger.info("Settings:")
    for key, value in config.settings.items():
        logger.info(f"  - {key}: {value}")

    html = config.generate_html()
    logger.info(f"Generated HTML: {len(html)} bytes")


# ===== Example 9: Development Setup =====


def example_development() -> None:
    """Development environment with full playground."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 9: Development Setup")
    logger.info("=" * 60)

    config = PlaygroundConfig(
        enabled=True,
        path="/playground",
        title="Local GraphQL Development",
        subscriptions_endpoint="ws://localhost:8000/graphql/subscriptions",
        settings={
            "editor.theme": "dark",
            "editor.fontSize": 14,
            "editor.cursorShape": "line",
            "general.betaUpdates": True,
        },
    )

    logger.info(f"Playground Config: {config}")
    logger.info("✓ Full featured for development")
    logger.info("  - Dark theme for less eye strain")
    logger.info("  - Larger font size")
    logger.info("  - WebSocket subscriptions enabled")


# ===== Example 10: Production Setup =====


def example_production() -> None:
    """Production environment with playground disabled."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 10: Production Setup")
    logger.info("=" * 60)

    config = PlaygroundConfig(
        enabled=False,
        path="/playground",  # Not served, but path defined
        title="Production API",
    )

    logger.info(f"Playground Config: {config}")
    logger.info("⚠️ Security: Playground disabled in production")
    logger.info("✓ Reduces surface area, improves security")
    logger.info("✓ Users can use external clients (GraphQL IDE, Insomnia, etc)")


# ===== Example 11: HTML Generation =====


def example_html_generation() -> None:
    """Show generated HTML for playground."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 11: HTML Generation")
    logger.info("=" * 60)

    config = PlaygroundConfig(
        title="Sample API",
        subscriptions_endpoint="/graphql/ws",
    )

    html = config.generate_html(graphql_endpoint="/api/graphql")

    logger.info("Generated HTML structure:")
    logger.info(f"  Length: {len(html)} bytes")
    logger.info(f"  Contains <!DOCTYPE html>: {'<!DOCTYPE html>' in html}")
    logger.info(f"  Contains GraphQL Playground JS: {'graphql-playground' in html}")
    logger.info(f"  Contains CSS: {'.css' in html}")
    logger.info(f"  Contains endpoint config: {'/api/graphql' in html}")
    logger.info(f"  Contains subscription config: {'/graphql/ws' in html}")


# ===== Example 12: Settings Examples =====


def example_settings_showcase() -> None:
    """Show various playground settings."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 12: Playground Settings Showcase")
    logger.info("=" * 60)

    settings_examples = {
        "Minimal": {},
        "Dark Theme": {"editor.theme": "dark"},
        "Query-Only": {
            "editor.theme": "dark",
            "general.betaUpdates": False,
        },
        "Development": {
            "editor.theme": "dark",
            "editor.fontSize": 14,
            "editor.reuseHeaders": True,
            "general.betaUpdates": True,
        },
        "Premium": {
            "editor.theme": "light",
            "editor.fontSize": 13,
            "editor.cursorShape": "block",
            "editor.reuseHeaders": True,
            "general.betaUpdates": False,
            "prettier.printWidth": 120,
            "schema.disableComments": False,
            "tracing.hideTracingResponse": False,
        },
    }

    for name, settings in settings_examples.items():
        logger.info(f"\n{name} Configuration:")
        if settings:
            for key, value in settings.items():
                logger.info(f"  {key}: {value}")
        else:
            logger.info("  (No custom settings)")


# ===== Example 13: API Versioning =====


def example_api_versioning() -> None:
    """Playground for different API versions."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 13: API Versioning")
    logger.info("=" * 60)

    configs = {
        "v1": PlaygroundConfig(
            path="/playground/v1",
            title="GraphQL API v1",
            subscriptions_endpoint="/v1/graphql/subscriptions",
        ),
        "v2": PlaygroundConfig(
            path="/playground/v2",
            title="GraphQL API v2",
            subscriptions_endpoint="/v2/graphql/subscriptions",
        ),
    }

    logger.info("Multiple playground instances for different API versions:")
    for version, config in configs.items():
        logger.info(f"\n{version.upper()}:")
        logger.info(f"  Path: {config.path}")
        logger.info(f"  Title: {config.title}")
        logger.info(f"  Subscriptions: {config.subscriptions_endpoint}")


# ===== Main Entry Point =====


if __name__ == "__main__":
    import sys

    examples = {
        "default": example_default_playground,
        "custom_path": example_custom_path,
        "disabled": example_disabled_playground,
        "dark_theme": example_dark_theme,
        "custom_endpoint": example_custom_endpoint,
        "no_subscriptions": example_no_subscriptions,
        "custom_subscription": example_custom_subscription_endpoint,
        "full": example_full_configuration,
        "development": example_development,
        "production": example_production,
        "html_generation": example_html_generation,
        "settings_showcase": example_settings_showcase,
        "api_versioning": example_api_versioning,
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
        print("All playground examples completed!")
        print("=" * 60)
