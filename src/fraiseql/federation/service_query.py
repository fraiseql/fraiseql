r"""Apollo Federation _service query implementation.

Implements the _service query which returns:
1. SDL: Complete schema definition with federation directives
2. Supported directives: Which Apollo Federation directives are supported

Example:
    The _service query returns:

        query {
          _service {
            sdl
          }
        }

    Response:
        {
          "_service": {
            "sdl": "type User @key(fields: \"id\") { ... }"
          }
        }
"""

from typing import Any

from .sdl_generator import generate_schema_sdl


class ServiceQueryResolver:
    """Resolves the _service query for Apollo Federation.

    Handles:
    - SDL generation and caching
    - Supported directives reporting
    - Gateway schema compatibility
    """

    # Federation directives supported by FraiseQL
    SUPPORTED_DIRECTIVES = {
        "key": True,  # @key for entity keys
        "external": True,  # @external for extended fields
        "requires": True,  # @requires for computed fields
        "provides": True,  # @provides for eager loading
        "extends": True,  # extend type for extensions
        "reference": False,  # @reference (not implemented yet)
    }

    def __init__(self, cache_sdl: bool = True):
        """Initialize service query resolver.

        Args:
            cache_sdl: Whether to cache generated SDL (recommended for production)
        """
        self.cache_sdl = cache_sdl
        self._cached_sdl: str | None = None

    def resolve_service(self) -> dict[str, Any]:
        """Resolve _service query.

        Returns:
            Dictionary with 'sdl' containing the complete schema

        Example:
            >>> resolver = ServiceQueryResolver()
            >>> result = resolver.resolve_service()
            >>> print(result["sdl"][:100])
            'type User @key(fields: "id") { ... }'
        """
        sdl = self.get_sdl()
        return {"sdl": sdl}

    def get_sdl(self) -> str:
        """Get complete SDL schema.

        Generates or returns cached SDL.

        Returns:
            SDL schema string

        Example:
            >>> sdl = resolver.get_sdl()
            >>> assert "@key" in sdl
            >>> assert "type User" in sdl
        """
        if self.cache_sdl and self._cached_sdl is not None:
            return self._cached_sdl

        sdl = generate_schema_sdl()

        if self.cache_sdl:
            self._cached_sdl = sdl

        return sdl

    def clear_cache(self) -> None:
        """Clear cached SDL.

        Use when entities are registered dynamically.
        """
        self._cached_sdl = None

    def get_supported_directives(self) -> dict[str, bool]:
        """Get supported federation directives.

        Returns:
            Dictionary mapping directive names to support status

        Example:
            >>> directives = resolver.get_supported_directives()
            >>> assert directives["key"] is True
            >>> assert directives["external"] is True
        """
        return self.SUPPORTED_DIRECTIVES.copy()

    def is_directive_supported(self, directive_name: str) -> bool:
        """Check if a directive is supported.

        Args:
            directive_name: Name of directive (without @)

        Returns:
            True if directive is supported, False otherwise

        Example:
            >>> resolver.is_directive_supported("key")
            True
            >>> resolver.is_directive_supported("reference")
            False
        """
        return self.SUPPORTED_DIRECTIVES.get(directive_name, False)

    def get_federation_config(self) -> dict[str, Any]:
        """Get federation configuration for gateway integration.

        Returns:
            Configuration dictionary for Apollo Gateway/Router

        Example:
            >>> config = resolver.get_federation_config()
            >>> assert "sdl" in config
            >>> assert "directives" in config
        """
        return {
            "sdl": self.get_sdl(),
            "directives": self.get_supported_directives(),
            "version": 2,  # Apollo Federation 2.0
        }


def create_service_resolver(cache_sdl: bool = True) -> ServiceQueryResolver:
    """Create a service query resolver.

    Convenience function for creating resolvers.

    Args:
        cache_sdl: Whether to cache SDL

    Returns:
        ServiceQueryResolver instance

    Example:
        >>> resolver = create_service_resolver()
        >>> service_data = resolver.resolve_service()
    """
    return ServiceQueryResolver(cache_sdl=cache_sdl)


# Default service resolver instance (production usage)
_default_resolver: ServiceQueryResolver | None = None


def get_default_resolver() -> ServiceQueryResolver:
    """Get or create the default service resolver.

    Uses singleton pattern for production.

    Returns:
        Default ServiceQueryResolver instance

    Example:
        >>> resolver = get_default_resolver()
        >>> sdl = resolver.get_sdl()
    """
    global _default_resolver
    if _default_resolver is None:
        _default_resolver = ServiceQueryResolver(cache_sdl=True)
    return _default_resolver


def reset_default_resolver() -> None:
    """Reset the default service resolver.

    Use in tests or when entities are registered dynamically.

    Example:
        >>> reset_default_resolver()
        >>> resolver = get_default_resolver()
    """
    resolver = get_default_resolver()
    resolver.clear_cache()
