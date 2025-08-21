"""Simple CamelForge configuration."""

import os
from dataclasses import dataclass


@dataclass
class CamelForgeConfig:
    """Simple CamelForge configuration.

    Environment variables override config values:
    - FRAISEQL_CAMELFORGE_ENABLED=true/false
    - FRAISEQL_CAMELFORGE_FUNCTION=function_name
    - FRAISEQL_CAMELFORGE_FIELD_THRESHOLD=20
    """

    enabled: bool = False
    function: str = "turbo.fn_camelforge"
    field_threshold: int = 20

    @classmethod
    def create(
        cls,
        enabled: bool = False,
        function: str = "turbo.fn_camelforge",
        field_threshold: int = 20,
    ) -> "CamelForgeConfig":
        """Create config with optional environment variable overrides."""
        # Environment variables override config parameters
        enabled = _get_env_bool("FRAISEQL_CAMELFORGE_ENABLED", enabled)
        function = _get_env_str("FRAISEQL_CAMELFORGE_FUNCTION", function)
        field_threshold = _get_env_int("FRAISEQL_CAMELFORGE_FIELD_THRESHOLD", field_threshold)

        return cls(
            enabled=enabled,
            function=function,
            field_threshold=field_threshold,
        )


def _get_env_bool(env_var: str, default: bool) -> bool:
    """Get boolean from environment variable."""
    value = os.getenv(env_var)
    if value is not None:
        return value.lower() in ("true", "1", "yes")
    return default


def _get_env_str(env_var: str, default: str) -> str:
    """Get string from environment variable."""
    return os.getenv(env_var, default)


def _get_env_int(env_var: str, default: int) -> int:
    """Get integer from environment variable."""
    value = os.getenv(env_var)
    if value:
        try:
            return int(value)
        except ValueError:
            pass
    return default
