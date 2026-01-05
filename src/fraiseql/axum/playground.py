"""GraphQL Playground configuration and HTML generation.

Provides interactive GraphQL IDE for development and testing.
"""

import logging
from dataclasses import dataclass, field

logger = logging.getLogger(__name__)


@dataclass
class PlaygroundConfig:
    """GraphQL Playground configuration.

    Controls whether the GraphQL Playground IDE is available and its settings.

    Attributes:
        enabled: Whether to serve GraphQL Playground (default: True)
        path: URL path to serve playground at (default: "/playground")
        title: Page title for playground (default: "GraphQL Playground")
        subscriptions_endpoint: WebSocket endpoint for subscriptions
            (default: "/graphql/subscriptions")
        settings: Custom GraphQL Playground settings dict (default: None)

    Example:
        ```python
        config = PlaygroundConfig(
            enabled=True,
            path="/playground",
            title="My GraphQL API",
            subscriptions_endpoint="/graphql/subscriptions"
        )
        html = config.generate_html()
        ```
    """

    enabled: bool = True
    path: str = "/playground"
    title: str = "GraphQL Playground"
    subscriptions_endpoint: str | None = "/graphql/subscriptions"
    settings: dict | None = field(default=None)

    def __post_init__(self) -> None:
        """Validate configuration after initialization."""
        if not self.path.startswith("/"):
            raise ValueError(f"Playground path must start with /: {self.path}")

        if self.subscriptions_endpoint and not self.subscriptions_endpoint.startswith("/"):
            raise ValueError(
                f"Subscriptions endpoint must start with /: {self.subscriptions_endpoint}",
            )

        logger.debug(
            f"PlaygroundConfig initialized: enabled={self.enabled}, "
            f"path={self.path}, title={self.title}",
        )

    def generate_html(self, graphql_endpoint: str = "/graphql") -> str:
        """Generate HTML for GraphQL Playground.

        Creates a complete HTML page with GraphQL Playground embedded.

        Args:
            graphql_endpoint: GraphQL endpoint URL (default: "/graphql")

        Returns:
            Complete HTML page as string ready to serve

        Example:
            ```python
            config = PlaygroundConfig()
            html = config.generate_html()
            # Returns complete HTML page with playground
            ```
        """
        # Build settings object
        settings = self.settings or {}

        # Ensure default settings are present
        settings_str = self._serialize_settings(settings)

        # Generate HTML with proper escaping
        html = f"""<!DOCTYPE html>
<html>
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>{self._escape_html(self.title)}</title>
    <link
      rel="stylesheet"
      href="https://cdn.jsdelivr.net/npm/graphql-playground-react/build/static/css/index.css"
    />
    <link
      rel="shortcut icon"
      href="https://cdn.jsdelivr.net/npm/graphql-playground-react/build/favicon.png"
    />
    <script src="https://cdn.jsdelivr.net/npm/graphql-playground-react/umd/graphql-playground.min.js"></script>
  </head>
  <body>
    <div id="root"></div>
    <script>
      window.addEventListener('load', function (event) {{
        GraphQLPlayground.init(document.getElementById('root'), {{
          endpoint: '{self._escape_js(graphql_endpoint)}',
          {self._format_subscription_endpoint()}
          settings: {settings_str}
        }})
      }})
    </script>
  </body>
</html>"""

        return html

    def _format_subscription_endpoint(self) -> str:
        """Format subscription endpoint config for HTML."""
        if self.subscriptions_endpoint:
            return f"subscriptionEndpoint: '{self._escape_js(self.subscriptions_endpoint)}',"
        return ""

    def _serialize_settings(self, settings: dict) -> str:
        """Serialize settings dict to JavaScript object.

        Args:
            settings: Settings dictionary

        Returns:
            JavaScript object notation string
        """
        if not settings:
            return "{}"

        # Build JavaScript object
        items = []
        for key, value in settings.items():
            if isinstance(value, str):
                items.append(f"{key}: '{self._escape_js(value)}'")
            elif isinstance(value, bool):
                items.append(f"{key}: {str(value).lower()}")
            elif isinstance(value, (int, float)):
                items.append(f"{key}: {value}")
            elif isinstance(value, dict):
                # Nested object
                nested = self._serialize_settings(value)
                items.append(f"{key}: {nested}")
            elif isinstance(value, list):
                # Array
                items.append(f"{key}: [{', '.join(str(v) for v in value)}]")

        return "{" + ", ".join(items) + "}"

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

    @staticmethod
    def _escape_js(text: str) -> str:
        """Escape JavaScript string special characters.

        Args:
            text: Text to escape

        Returns:
            JavaScript-escaped text
        """
        return (
            text.replace("\\", "\\\\")
            .replace("'", "\\'")
            .replace('"', '\\"')
            .replace("\n", "\\n")
            .replace("\r", "\\r")
        )

    def to_dict(self) -> dict:
        """Convert to dictionary for serialization.

        Returns:
            Configuration as dictionary
        """
        return {
            "enabled": self.enabled,
            "path": self.path,
            "title": self.title,
            "subscriptions_endpoint": self.subscriptions_endpoint,
            "settings": self.settings or {},
        }

    def __repr__(self) -> str:
        """String representation."""
        return f"PlaygroundConfig(enabled={self.enabled}, path={self.path}, title={self.title!r})"

    def __str__(self) -> str:
        """User-friendly string."""
        status = "enabled" if self.enabled else "disabled"
        return f"GraphQL Playground [{status}] at {self.path}"
