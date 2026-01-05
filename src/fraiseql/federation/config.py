"""Federation configuration and presets.

Defines FederationConfig for customizing federation behavior and
Presets for common use cases (LITE, STANDARD, ADVANCED).
"""

from dataclasses import dataclass, field


@dataclass
class FederationConfig:
    """Configuration for Apollo Federation support.

    Attributes:
        enabled: Enable federation support (default: True)
        version: Apollo Federation version (default: "2.5")
        auto_keys: Auto-detect entity keys from 'id' field (default: True)
        auto_entities_resolver: Auto-generate _entities resolver (default: True)
        auto_service_resolver: Auto-generate _service query (default: True)
        directives: List of supported directives (default: ["key", "external"])
        batch_size: DataLoader batch size (default: 100)
        batch_window_ms: Wait time for batching in milliseconds (default: 10)
        cache_sdl: Cache generated SDL (default: True)
        cache_ttl_seconds: SDL cache TTL in seconds (default: 3600)
    """

    # Basic settings
    enabled: bool = True
    version: str = "2.5"

    # Feature flags
    auto_keys: bool = True
    auto_entities_resolver: bool = True
    auto_service_resolver: bool = True

    # Directives to support
    directives: list[str] = field(default_factory=lambda: ["key", "external"])

    # Performance
    batch_size: int = 100
    batch_window_ms: int = 10

    # Caching
    cache_sdl: bool = True
    cache_ttl_seconds: int | None = 3600

    def __post_init__(self):
        """Validate configuration after initialization."""
        if not self.directives:
            self.directives = ["key", "external"]

        # Ensure required directives are present
        if "key" not in self.directives:
            self.directives.insert(0, "key")

    def __repr__(self) -> str:
        return (
            f"FederationConfig("
            f"enabled={self.enabled}, "
            f"version={self.version!r}, "
            f"auto_keys={self.auto_keys}, "
            f"directives={self.directives}"
            f")"
        )


class Presets:
    """Federation configuration presets for common use cases.

    Three preset modes balance simplicity vs power, accommodating 95% of federation use cases.

    Attributes:
        LITE: Auto-keys only (80% of users) - simplest configuration, recommended for starting
        STANDARD: With extensions (15% of users) - type extensions with @requires/@provides support
        ADVANCED: All 18 directives (5% of users) - full Apollo Federation 2.0 support

    Usage Examples:
        Simple federation with auto-detected keys:
        >>> from fraiseql import Schema
        >>> from fraiseql.federation import Presets
        >>>
        >>> schema = Schema(
        ...     federation=True,
        ...     federation_config=Presets.LITE
        ... )

        Type extensions with computed fields:
        >>> schema = Schema(
        ...     federation=True,
        ...     federation_config=Presets.STANDARD
        ... )

        Full federation support:
        >>> schema = Schema(
        ...     federation=True,
        ...     federation_config=Presets.ADVANCED
        ... )

    Decision Guide:
        Use LITE if:
        - Starting with federation
        - Simple entity keys (e.g., id field)
        - No cross-subgraph dependencies

        Use STANDARD if:
        - Extending types from other subgraphs
        - Need @requires/@provides directives
        - Computing fields from external data

        Use ADVANCED if:
        - Complex multi-subgraph federation
        - Need all GraphQL directives
        - Advanced shareable/override patterns
    """

    # Lite: Auto-keys only (80% of users)
    # Simplest configuration - just @entity with auto-detected keys
    # Use when starting federation or for simple cases
    LITE = FederationConfig(
        version="2.5",
        auto_keys=True,
        auto_entities_resolver=True,
        auto_service_resolver=True,
        directives=["key", "external"],
        batch_size=100,
        batch_window_ms=10,
        cache_sdl=True,
        cache_ttl_seconds=3600,
    )

    # Standard: With extensions (15% of users)
    # Includes type extensions, @requires, @provides for computed fields
    # Use when extending types or computing derived fields
    STANDARD = FederationConfig(
        version="2.5",
        auto_keys=True,
        auto_entities_resolver=True,
        auto_service_resolver=True,
        directives=["key", "external", "requires", "provides"],
        batch_size=100,
        batch_window_ms=10,
        cache_sdl=True,
        cache_ttl_seconds=3600,
    )

    # Advanced: All 18 directives (5% of users, Phase 17b)
    # Full Apollo Federation 2.0 support with all directives
    # Use for complex multi-subgraph scenarios with advanced patterns
    ADVANCED = FederationConfig(
        version="2.5",
        auto_keys=False,
        auto_entities_resolver=True,
        auto_service_resolver=True,
        directives=[
            "key",
            "external",
            "requires",
            "provides",
            "shareable",
            "override",
            "inaccessible",
            "tag",
            "interfaceObject",
        ],
        batch_size=100,
        batch_window_ms=10,
        cache_sdl=True,
        cache_ttl_seconds=3600,
    )
